//! Frontend (wasm/browser) half of Discord auth.
//!
//! - [`begin_login`] redirects the browser to Discord's authorize URL.
//! - [`Session::load`] resolves the session from the URL fragment
//!   (`#session=<jwt>`, how the callback delivers it) or localStorage, and
//!   exposes the parsed [`EntitlementSet`] + the auth header to attach to
//!   gated API calls.
//!
//! The JWT is decoded WITHOUT verifying its signature — the frontend has no
//! secret and doesn't need one; the worker verifies on every gated request.
//! The frontend only needs the `ent` claim to decide what to render.
//!
//! egui-agnostic on purpose: this returns an [`EntitlementSet`], which a UI
//! layer wraps in its own gate type (e.g. `egui_widgets::gated::GateState`).

use authorizations::EntitlementSet;

const SESSION_KEY: &str = "discord_session_jwt";
const DEBUG_KEY: &str = "discord_debug_token";

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
    /// Lets the app route the user back to the gated feature after login.
    pub fresh_login: bool,
}

impl Session {
    /// Resolve from URL fragment (`#session=<jwt>` or `#debug=<token>`)
    /// then localStorage. Strips a fragment token after reading so a shared
    /// URL doesn't leak the session. Fragment routes (`#/...`) are left
    /// untouched.
    pub fn load() -> Self {
        if let Some(frag) = url_fragment() {
            for part in frag.trim_start_matches('#').split('&') {
                if let Some(jwt) = part.strip_prefix("session=") {
                    store(SESSION_KEY, jwt);
                    clear_fragment();
                    return Self {
                        fresh_login: true,
                        ..Self::from_jwt(jwt)
                    };
                }
                if let Some(tok) = part.strip_prefix("debug=") {
                    store(DEBUG_KEY, tok);
                    clear_fragment();
                    return Self {
                        fresh_login: true,
                        ..Self::from_debug(tok)
                    };
                }
            }
        }
        if let Some(jwt) = load_stored(SESSION_KEY) {
            return Self::from_jwt(&jwt);
        }
        if let Some(tok) = load_stored(DEBUG_KEY) {
            return Self::from_debug(&tok);
        }
        Self::default()
    }

    fn from_jwt(jwt: &str) -> Self {
        let ent = decode_ent_claim(jwt).unwrap_or_default();
        Self {
            entitlements: EntitlementSet::from_scope_string(&ent),
            authenticated: true,
            auth_header: Some(("Authorization".into(), format!("Bearer {jwt}"))),
            fresh_login: false,
        }
    }

    /// Operator/debug bypass — sends `X-Debug-Token` and grants everything
    /// locally (`*`) so gated UI unlocks for testing before the bot ships.
    fn from_debug(token: &str) -> Self {
        Self {
            entitlements: EntitlementSet::from_scope_string("*"),
            authenticated: true,
            auth_header: Some(("X-Debug-Token".into(), token.to_string())),
            fresh_login: false,
        }
    }

    /// Log out — clears stored tokens and returns to anonymous.
    pub fn clear(&mut self) {
        remove(SESSION_KEY);
        remove(DEBUG_KEY);
        *self = Self::default();
    }
}

/// Redirect the browser to Discord's authorize URL to begin login.
pub fn begin_login(client_id: &str, redirect_uri: &str, scopes: &[&str]) {
    // A `state` nonce for CSRF; a timestamp-free constant is acceptable for
    // a public read-only gate, but callers may pass their own via the
    // authorize_url builder directly if they want strict CSRF.
    let url = crate::authorize_url(client_id, redirect_uri, scopes, "login");
    if let Some(win) = web_sys::window() {
        let _ = win.location().set_href(&url);
    }
}

/// Extract the `ent` claim from a JWT payload without verifying it.
fn decode_ent_claim(jwt: &str) -> Option<String> {
    let payload_b64 = jwt.split('.').nth(1)?;
    let json = base64url_decode(payload_b64)?;
    #[derive(serde::Deserialize)]
    struct Payload {
        #[serde(default)]
        ent: String,
    }
    serde_json::from_slice::<Payload>(&json).ok().map(|p| p.ent)
}

fn base64url_decode(s: &str) -> Option<Vec<u8>> {
    let mut b64: String = s.replace('-', "+").replace('_', "/");
    while b64.len() % 4 != 0 {
        b64.push('=');
    }
    let decoded = web_sys::window()?.atob(&b64).ok()?;
    Some(decoded.chars().map(|c| c as u8).collect())
}

fn url_fragment() -> Option<String> {
    let hash = web_sys::window()?.location().hash().ok()?;
    (!hash.is_empty()).then_some(hash)
}

fn clear_fragment() {
    if let Some(win) = web_sys::window() {
        if let Ok(history) = win.history() {
            let path = win
                .location()
                .pathname()
                .unwrap_or_else(|_| "/".to_string());
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&path));
        }
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}
fn store(key: &str, val: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(key, val);
    }
}
fn load_stored(key: &str) -> Option<String> {
    storage()?.get_item(key).ok()?
}
fn remove(key: &str) {
    if let Some(s) = storage() {
        let _ = s.remove_item(key);
    }
}
