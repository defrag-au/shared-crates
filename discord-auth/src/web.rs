//! Frontend half of Discord auth for wasm-bindgen apps, through web-sys.
//!
//! - [`begin_login`] redirects the browser to Discord's authorize URL.
//! - [`Session::load`] resolves the session from the URL fragment
//!   (`#session=<jwt>`, how the callback delivers it) or localStorage, and
//!   exposes the parsed [`EntitlementSet`] + the auth header to attach to
//!   gated API calls.
//!
//! The logic is [`crate::session`]'s; this is the web-sys edge of it. A
//! macroquad game uses [`crate::miniquad`] instead.
//!
//! egui-agnostic on purpose: this returns an [`EntitlementSet`], which a UI
//! layer wraps in its own gate type (e.g. `egui_widgets::gated::GateState`).

use authorizations::EntitlementSet;

pub use crate::session::Identity;
use crate::session::{self, Browser, Held};

/// Resolved session for the frontend.
#[derive(Clone, Default)]
pub struct Session {
    /// Entitlements from the JWT `ent` claim (empty when anonymous).
    pub entitlements: EntitlementSet,
    /// True once a session (or debug token) has been resolved.
    pub authenticated: bool,
    /// `(header_name, header_value)` to attach to gated requests, if any.
    pub auth_header: Option<(String, String)>,
    /// True only when the session was loaded from a URL fragment THIS
    /// page-load (a fresh login redirect), vs restored from localStorage.
    pub fresh_login: bool,
    /// Discord identity (from the JWT `name`/`avatar`/`sub` claims), for
    /// the "logged in as" UI. `None` for anonymous / debug sessions.
    pub identity: Option<Identity>,
    /// JWT `exp` claim, unix seconds. `None` for debug/anonymous sessions.
    /// Apps can use this to prompt re-login before a request bounces.
    pub expires_at: Option<u64>,
}

impl Session {
    /// Resolve from URL fragment (`#session=<jwt>` or `#debug=<token>`)
    /// then localStorage. Strips a fragment token after reading so a shared
    /// URL doesn't leak the session. Fragment routes (`#/...`) are left
    /// untouched.
    pub fn load() -> Self {
        let loaded = session::load(&WebSys);
        let session = match loaded.held {
            Held::Session { token, claims } => Self {
                entitlements: EntitlementSet::from_scope_string(&claims.ent),
                authenticated: true,
                auth_header: Some(("Authorization".into(), format!("Bearer {token}"))),
                fresh_login: false,
                identity: claims.identity(),
                expires_at: claims.exp,
            },
            // Operator/debug bypass — sends `X-Debug-Token` and grants
            // everything locally (`*`) so gated UI unlocks for testing.
            Held::Debug { token } => Self {
                entitlements: EntitlementSet::from_scope_string("*"),
                authenticated: true,
                auth_header: Some(("X-Debug-Token".into(), token)),
                ..Self::default()
            },
            Held::Anonymous => Self::default(),
        };
        Self {
            fresh_login: loaded.fresh,
            ..session
        }
    }

    /// Log out — clears stored tokens and returns to anonymous.
    pub fn clear(&mut self) {
        session::clear(&WebSys);
        *self = Self::default();
    }
}

/// Redirect the browser to Discord's authorize URL to begin login.
///
/// This carries a constant `state`, so it has no CSRF check. A worker that
/// mounts [`crate::worker::begin_login`] is the stronger route: link to it
/// instead, and the worker sets and checks a `state` nonce.
pub fn begin_login(client_id: &str, redirect_uri: &str, scopes: &[&str]) {
    WebSys.navigate(&crate::authorize_url(
        client_id,
        redirect_uri,
        scopes,
        "login",
    ));
}

/// The browser, through web-sys.
struct WebSys;

impl Browser for WebSys {
    fn fragment(&self) -> Option<String> {
        let hash = web_sys::window()?.location().hash().ok()?;
        (!hash.is_empty()).then_some(hash)
    }

    fn clear_fragment(&self) {
        if let Some(win) = web_sys::window()
            && let Ok(history) = win.history()
        {
            let path = win
                .location()
                .pathname()
                .unwrap_or_else(|_| "/".to_string());
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&path));
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        storage()?.get_item(key).ok()?
    }

    fn set(&self, key: &str, value: &str) {
        if let Some(s) = storage() {
            let _ = s.set_item(key, value);
        }
    }

    fn remove(&self, key: &str) {
        if let Some(s) = storage() {
            let _ = s.remove_item(key);
        }
    }

    fn now_secs(&self) -> u64 {
        (js_sys::Date::now() / 1000.0) as u64
    }

    fn navigate(&self, url: &str) {
        if let Some(win) = web_sys::window() {
            let _ = win.location().set_href(url);
        }
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}
