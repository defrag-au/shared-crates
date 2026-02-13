//! Web-specific loading orchestrator (requires `web` feature)
//!
//! This module provides the [`LoadingOrchestrator`] for wasm-bindgen based frameworks.
//!
//! ## Token Persistence for SPAs
//!
//! When using client-side routing (e.g., Leptos Router), the URL query params can be
//! lost during navigation. To handle this, the loader stores the token in sessionStorage
//! after successful auth validation. On subsequent page loads or refreshes, the token
//! is read from sessionStorage if not present in the URL.
//!
//! This allows SPAs to navigate freely without losing authentication.

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

use maud::{html, Markup, PreEscaped};
use ui_core::auth::AuthState;
use ui_core::error::WidgetError;
use ui_core::runtime::{get_query_param, init_widget_with_level};
use wasm_bindgen::JsCast;

pub use tracing::Level;

/// sessionStorage key for persisted token
const TOKEN_STORAGE_KEY: &str = "auth_token";

/// Get the auth token, checking URL params first then sessionStorage
fn get_token() -> Option<String> {
    // First check URL query params (fresh link from Discord)
    if let Some(token) = get_query_param("token") {
        if !token.is_empty() {
            return Some(token);
        }
    }

    // Fall back to sessionStorage (SPA navigation/refresh)
    get_session_storage_token()
}

/// Get token from sessionStorage
fn get_session_storage_token() -> Option<String> {
    web_sys::window()
        .and_then(|w| w.session_storage().ok())
        .flatten()
        .and_then(|storage| storage.get_item(TOKEN_STORAGE_KEY).ok())
        .flatten()
        .filter(|t| !t.is_empty())
}

/// Store token in sessionStorage for SPA persistence
fn store_token(token: &str) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.session_storage().ok())
        .flatten()
    {
        let _ = storage.set_item(TOKEN_STORAGE_KEY, token);
        tracing::debug!("Token stored in sessionStorage for SPA persistence");
    }
}

/// Clear token from sessionStorage (e.g., on logout or expiry)
pub fn clear_stored_token() {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.session_storage().ok())
        .flatten()
    {
        let _ = storage.remove_item(TOKEN_STORAGE_KEY);
        tracing::debug!("Token cleared from sessionStorage");
    }
}

/// Configuration for the loading orchestrator
#[derive(Debug, Clone)]
pub struct LoaderConfig {
    /// Whether authentication is required (default: true)
    pub auth_required: bool,
    /// Log level for tracing (default: DEBUG)
    pub log_level: Level,
    /// Initial loading message
    pub initial_message: String,
    /// Optional hook called after auth validation, before load
    pub on_before_load: Option<fn()>,
}

impl Default for LoaderConfig {
    fn default() -> Self {
        Self {
            auth_required: true,
            log_level: Level::DEBUG,
            initial_message: "Loading...".to_string(),
            on_before_load: None,
        }
    }
}

impl LoaderConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn auth_required(mut self, required: bool) -> Self {
        self.auth_required = required;
        self
    }

    pub fn log_level(mut self, level: Level) -> Self {
        self.log_level = level;
        self
    }

    pub fn initial_message(mut self, msg: impl Into<String>) -> Self {
        self.initial_message = msg.into();
        self
    }

    /// Set a hook to be called after auth validation, before the load function.
    /// Useful for registering web components or other initialization.
    pub fn on_before_load(mut self, hook: fn()) -> Self {
        self.on_before_load = Some(hook);
        self
    }
}

/// Result of successful loading - handed to the framework
pub struct LoadResult<T> {
    /// Authentication state (always present, may be Anonymous)
    pub auth: AuthState,
    /// The loaded data
    pub data: T,
    /// World ID extracted from URL (if present)
    pub world_id: Option<String>,
    /// Discord channel URL for "return to Discord" (from JWT claims)
    pub discord_url: Option<String>,
}

/// Error during loading
#[derive(Debug, Clone)]
pub enum LoaderError {
    /// Authentication required but not provided or invalid
    AuthRequired(String),
    /// Token has expired
    TokenExpired,
    /// Data fetch failed
    FetchFailed(String),
    /// Other error
    Other(String),
}

impl std::fmt::Display for LoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthRequired(msg) => write!(f, "Authentication required: {msg}"),
            Self::TokenExpired => write!(f, "Session has expired"),
            Self::FetchFailed(msg) => write!(f, "Failed to load data: {msg}"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl From<WidgetError> for LoaderError {
    fn from(err: WidgetError) -> Self {
        match err {
            WidgetError::TokenExpired => Self::TokenExpired,
            WidgetError::NotAuthenticated => Self::AuthRequired("Not authenticated".to_string()),
            WidgetError::Http { status, message } => {
                Self::FetchFailed(format!("HTTP {status}: {message}"))
            }
            WidgetError::Network(msg) => Self::FetchFailed(msg),
            WidgetError::Parse(msg) => Self::FetchFailed(format!("Parse error: {msg}")),
            WidgetError::Other(msg) => Self::Other(msg),
        }
    }
}

/// Handle to update loading progress
///
/// This is passed to the fetch callback so it can update the loading message
/// as it progresses through different fetch stages.
#[derive(Clone)]
pub struct LoadingHandle {
    inner: Rc<RefCell<LoadingState>>,
}

impl LoadingHandle {
    fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(LoadingState::new())),
        }
    }

    /// Update the loading message displayed to the user
    pub fn set_message(&self, msg: &str) {
        let mut state = self.inner.borrow_mut();
        state.message = msg.to_string();
        state.update_dom();
    }

    /// Get the current message
    pub fn message(&self) -> String {
        self.inner.borrow().message.clone()
    }

    fn show_loading(&self, msg: &str) {
        let mut state = self.inner.borrow_mut();
        state.message = msg.to_string();
        state.visible = true;
        state.error = None;
        state.inject_and_update();
    }

    fn show_error(&self, msg: &str, return_url: Option<&str>) {
        let mut state = self.inner.borrow_mut();
        state.visible = true;
        state.error = Some(ErrorDisplay {
            message: msg.to_string(),
            return_url: return_url.map(|s| s.to_string()),
        });
        state.inject_and_update();
    }

    fn hide(&self) {
        let mut state = self.inner.borrow_mut();
        state.visible = false;
        state.remove_from_dom();
    }
}

#[derive(Debug)]
struct ErrorDisplay {
    message: String,
    return_url: Option<String>,
}

#[derive(Debug)]
struct LoadingState {
    message: String,
    visible: bool,
    error: Option<ErrorDisplay>,
    element_id: &'static str,
}

impl LoadingState {
    fn new() -> Self {
        Self {
            message: "Loading...".to_string(),
            visible: false,
            error: None,
            element_id: "__widget_loader__",
        }
    }

    fn inject_and_update(&self) {
        let document = match web_sys::window().and_then(|w| w.document()) {
            Some(doc) => doc,
            None => return,
        };

        // Check if element already exists
        if document.get_element_by_id(self.element_id).is_none() {
            // Create and inject the loading overlay
            if let Ok(element) = document.create_element("div") {
                element.set_id(self.element_id);
                // Inject at start of body
                if let Some(body) = document.body() {
                    let _ = body.insert_before(&element, body.first_child().as_ref());
                }
            }
        }

        self.update_dom();
    }

    fn update_dom(&self) {
        let document = match web_sys::window().and_then(|w| w.document()) {
            Some(doc) => doc,
            None => return,
        };

        let element = match document.get_element_by_id(self.element_id) {
            Some(el) => el,
            None => return,
        };

        // Update visibility
        if let Some(html_el) = element.dyn_ref::<web_sys::HtmlElement>() {
            let style = html_el.style();
            let _ = style.set_property("display", if self.visible { "flex" } else { "none" });
        }

        // Update content
        let markup = if let Some(error) = &self.error {
            self.render_error(error)
        } else {
            self.render_loading()
        };
        element.set_inner_html(&markup.into_string());
    }

    fn remove_from_dom(&self) {
        let document = match web_sys::window().and_then(|w| w.document()) {
            Some(doc) => doc,
            None => return,
        };

        if let Some(element) = document.get_element_by_id(self.element_id) {
            element.remove();
        }
    }

    fn render_loading(&self) -> Markup {
        let id = self.element_id;
        html! {
            style {
                (PreEscaped(format!(r#"
                    #{id} {{
                        position: fixed;
                        inset: 0;
                        background: rgba(15, 17, 26, 0.98);
                        display: flex;
                        align-items: center;
                        justify-content: center;
                        z-index: 99999;
                        flex-direction: column;
                        font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
                    }}
                    #{id} .spinner {{
                        width: 3rem;
                        height: 3rem;
                        border: 3px solid #3a3f55;
                        border-top-color: #4a9eff;
                        border-radius: 50%;
                        animation: __loader_spin__ 0.75s linear infinite;
                    }}
                    @keyframes __loader_spin__ {{
                        to {{ transform: rotate(360deg); }}
                    }}
                    #{id} .message {{
                        color: #8b8fa3;
                        font-size: 1rem;
                        margin-top: 1.5rem;
                        text-align: center;
                    }}
                "#)))
            }
            div.spinner {}
            div.message { (&self.message) }
        }
    }

    fn render_error(&self, error: &ErrorDisplay) -> Markup {
        let id = self.element_id;
        html! {
            style {
                (PreEscaped(format!(r#"
                    #{id} {{
                        position: fixed;
                        inset: 0;
                        background: rgba(15, 17, 26, 0.98);
                        display: flex;
                        align-items: center;
                        justify-content: center;
                        z-index: 99999;
                        flex-direction: column;
                        font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
                        padding: 2rem;
                    }}
                    #{id} .error-icon {{
                        font-size: 3rem;
                        margin-bottom: 1rem;
                    }}
                    #{id} .error-title {{
                        color: #ef4444;
                        font-size: 1.25rem;
                        font-weight: 600;
                        margin-bottom: 0.75rem;
                    }}
                    #{id} .error-message {{
                        color: #8b8fa3;
                        font-size: 0.95rem;
                        text-align: center;
                        max-width: 400px;
                        line-height: 1.5;
                        margin-bottom: 1.5rem;
                    }}
                    #{id} .return-btn {{
                        display: inline-flex;
                        align-items: center;
                        gap: 0.5rem;
                        background: #5865F2;
                        color: white;
                        padding: 0.75rem 1.5rem;
                        border-radius: 0.5rem;
                        text-decoration: none;
                        font-weight: 500;
                        transition: background 0.2s;
                    }}
                    #{id} .return-btn:hover {{
                        background: #4752C4;
                    }}
                "#)))
            }
            div.error-icon { "\u{26A0}\u{FE0F}" }
            div.error-title { "Unable to Load" }
            div.error-message { (&error.message) }
            @if let Some(url) = &error.return_url {
                a.return-btn href=(url) { "Return" }
            }
        }
    }
}

/// The main loading orchestrator
pub struct LoadingOrchestrator;

impl LoadingOrchestrator {
    /// Run the loading sequence
    ///
    /// This is the main entry point. It:
    /// 1. Initializes the widget runtime (panic hooks, tracing)
    /// 2. Shows the loading screen
    /// 3. Parses URL and validates token
    /// 4. Calls your fetch function with auth and a loading handle
    /// 5. Returns the loaded data or shows an error screen
    ///
    /// The `fetch_fn` receives:
    /// - `AuthState` - the validated auth state
    /// - `LoadingHandle` - to update the loading message
    ///
    /// Returns `Ok(LoadResult)` on success, or `Err(LoaderError)` on failure.
    /// On error, the error screen is automatically shown.
    pub async fn run<T, F, Fut>(
        config: LoaderConfig,
        fetch_fn: F,
    ) -> Result<LoadResult<T>, LoaderError>
    where
        F: FnOnce(AuthState, LoadingHandle) -> Fut,
        Fut: Future<Output = Result<T, WidgetError>>,
    {
        // Initialize runtime
        init_widget_with_level(config.log_level);

        tracing::debug!("LoadingOrchestrator starting");

        // Create loading handle and show loading screen
        let handle = LoadingHandle::new();
        handle.show_loading(&config.initial_message);

        // Parse URL params - check URL first, then sessionStorage for SPA persistence
        let token = get_token();
        let world_id = get_query_param("world");

        // Validate auth
        let auth = AuthState::from_token(token.clone());

        // Store valid token in sessionStorage for SPA navigation/refresh
        if let (Some(token), AuthState::Authenticated(_)) = (&token, &auth) {
            store_token(token);
        }

        // Extract return URL for error screens (before we check auth status)
        let discord_url = auth
            .context()
            .and_then(|ctx| ctx.claims().discord_channel_url());

        // Check auth requirements
        if config.auth_required {
            match &auth {
                AuthState::Anonymous => {
                    let msg = "Missing authentication token. Please access this widget through the appropriate channel.";
                    handle.show_error(msg, discord_url.as_deref());
                    return Err(LoaderError::AuthRequired(msg.to_string()));
                }
                AuthState::TokenExpired => {
                    let msg = "Your session has expired. Please return and click the link again to get a fresh session.";
                    handle.show_error(msg, discord_url.as_deref());
                    return Err(LoaderError::TokenExpired);
                }
                AuthState::AuthError(e) => {
                    let msg = format!("Authentication error: {e}");
                    handle.show_error(&msg, discord_url.as_deref());
                    return Err(LoaderError::AuthRequired(msg));
                }
                AuthState::Authenticated(_) => {
                    tracing::debug!("Auth validated successfully");
                }
                AuthState::Authenticating => {
                    // Shouldn't happen in this flow, but handle it
                    tracing::warn!("Auth in unexpected Authenticating state");
                }
            }
        }

        // Call optional before-load hook
        if let Some(hook) = config.on_before_load {
            hook();
        }

        // Run the fetch function
        let data = match fetch_fn(auth.clone(), handle.clone()).await {
            Ok(data) => data,
            Err(err) => {
                let loader_err = LoaderError::from(err);
                let msg = loader_err.to_string();
                handle.show_error(&msg, discord_url.as_deref());
                return Err(loader_err);
            }
        };

        // Success! Hide the loader
        handle.hide();

        // Set up token expiry watcher (checks when tab becomes visible)
        if config.auth_required {
            setup_token_expiry_watcher(auth.clone(), discord_url.clone());
        }

        tracing::debug!("LoadingOrchestrator complete");

        Ok(LoadResult {
            auth,
            data,
            world_id,
            discord_url,
        })
    }

    /// Simplified run for cases where you just need to fetch one thing
    pub async fn run_simple<T, F, Fut>(
        config: LoaderConfig,
        fetch_fn: F,
    ) -> Result<LoadResult<T>, LoaderError>
    where
        F: FnOnce(AuthState) -> Fut,
        Fut: Future<Output = Result<T, WidgetError>>,
    {
        Self::run(config, |auth, _handle| async move { fetch_fn(auth).await }).await
    }
}

/// Set up a visibility change listener that checks token expiry when user returns to tab
///
/// This is called automatically by LoadingOrchestrator for authenticated widgets.
/// When the tab becomes visible, it checks if the token has expired and shows
/// the session expired overlay if so.
fn setup_token_expiry_watcher(auth: AuthState, return_url: Option<String>) {
    use wasm_bindgen::prelude::*;

    let document = match web_sys::window().and_then(|w| w.document()) {
        Some(doc) => doc,
        None => return,
    };

    // Create closure that checks token expiry
    let callback = Closure::wrap(Box::new(move || {
        // Check if document is now visible
        let is_visible = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| {
                js_sys::Reflect::get(&d, &wasm_bindgen::JsValue::from_str("visibilityState")).ok()
            })
            .and_then(|v| v.as_string())
            .map(|s| s == "visible")
            .unwrap_or(false);

        if !is_visible {
            return;
        }

        // Check if token has expired
        let is_expired = match &auth {
            AuthState::Authenticated(ctx) => ctx.is_expired(),
            AuthState::TokenExpired => true,
            _ => false,
        };

        if is_expired {
            tracing::info!("Token expired while tab was hidden, showing session expired overlay");
            show_session_expired_overlay(return_url.as_deref());
        }
    }) as Box<dyn Fn()>);

    // Add event listener
    let _ = document
        .add_event_listener_with_callback("visibilitychange", callback.as_ref().unchecked_ref());

    // Leak the closure so it lives forever (this is intentional - we want it to persist)
    callback.forget();
}

/// Show the session expired overlay
///
/// This reuses the loader's error display infrastructure to show a friendly
/// "session expired" message with a link back.
fn show_session_expired_overlay(return_url: Option<&str>) {
    let document = match web_sys::window().and_then(|w| w.document()) {
        Some(doc) => doc,
        None => return,
    };

    let element_id = "__widget_loader__";

    // Create or get the overlay element
    let element = match document.get_element_by_id(element_id) {
        Some(el) => el,
        None => match document.create_element("div") {
            Ok(el) => {
                el.set_id(element_id);
                if let Some(body) = document.body() {
                    let _ = body.insert_before(&el, body.first_child().as_ref());
                }
                el
            }
            Err(_) => return,
        },
    };

    // Show the element
    if let Some(html_el) = element.dyn_ref::<web_sys::HtmlElement>() {
        let _ = html_el.style().set_property("display", "flex");
    }

    // Render the expired session message
    let markup = render_session_expired(element_id, return_url);
    element.set_inner_html(&markup.into_string());
}

/// Render the session expired overlay HTML
fn render_session_expired(id: &str, return_url: Option<&str>) -> Markup {
    html! {
        style {
            (PreEscaped(format!(r#"
                #{id} {{
                    position: fixed;
                    inset: 0;
                    background: rgba(15, 17, 26, 0.98);
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    z-index: 99999;
                    flex-direction: column;
                    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
                    padding: 2rem;
                }}
                #{id} .error-icon {{
                    font-size: 3rem;
                    margin-bottom: 1rem;
                }}
                #{id} .error-title {{
                    color: #f59e0b;
                    font-size: 1.25rem;
                    font-weight: 600;
                    margin-bottom: 0.75rem;
                }}
                #{id} .error-message {{
                    color: #8b8fa3;
                    font-size: 0.95rem;
                    text-align: center;
                    max-width: 400px;
                    line-height: 1.5;
                    margin-bottom: 1.5rem;
                }}
                #{id} .return-btn {{
                    display: inline-flex;
                    align-items: center;
                    gap: 0.5rem;
                    background: #5865F2;
                    color: white;
                    padding: 0.75rem 1.5rem;
                    border-radius: 0.5rem;
                    text-decoration: none;
                    font-weight: 500;
                    transition: background 0.2s;
                }}
                #{id} .return-btn:hover {{
                    background: #4752C4;
                }}
            "#)))
        }
        div.error-icon { "\u{23F0}" }
        div.error-title { "Session Expired" }
        div.error-message {
            "Your session has expired. Please return and click the link again to get a fresh session."
        }
        @if let Some(url) = return_url {
            a.return-btn href=(url) { "Return" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loader_config_builder() {
        let config = LoaderConfig::new()
            .auth_required(false)
            .log_level(Level::WARN)
            .initial_message("Starting up...");

        assert!(!config.auth_required);
        assert_eq!(config.initial_message, "Starting up...");
    }
}
