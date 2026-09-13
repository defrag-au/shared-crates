//! [`AuthStrategy`] for the `auth.cnft.dev` OAuth worker — an opaque token,
//! validated against a service.
//!
//! # Which Discord this is
//!
//! There are **two** unrelated Discord auth paths in this estate, and mistaking
//! one for the other is the easiest way to break a login:
//!
//! - **`discord_auth::web::Session`** — a self-describing JWT that arrives in a
//!   `#session=` URL fragment and carries its own entitlements in the `ent`
//!   claim. Nothing to validate; restoring is synchronous. Used by
//!   `collection-ownership`, `meme-templates` and the augminted gateway.
//! - **This one** — an *opaque* token that arrives as `?token=` after a
//!   redirect through `{base}/auth/discord`, and means nothing until
//!   `{base}/validate` says who it belongs to. Used by `user-portal` and
//!   `frontends/admin`.
//!
//! Two consequences, both of which the shell has to respect:
//!
//! **Restoring is asynchronous.** A stored token is a *claim* until the service
//! agrees, so boot spends a round trip in [`AuthPhase::Restoring`]. This is the
//! strategy that makes `Restoring` earn its place in the enum — with the JWT
//! path you could almost get away without it.
//!
//! **There are no entitlements and no expiry.** `/validate` returns an identity,
//! not authority, so `ent` is always empty and
//! [`Session::expires_at`] is always `None`. Death is discovered by a 401 rather
//! than predicted. That is a property of the service, not an omission here, and
//! [`Session::is_expired`] returns `false` for it deliberately — anything else
//! would sign every user out on their first frame.
//!
//! # The redirect swallows `Pending`
//!
//! Sign-in navigates the tab away. There is no in-page round trip to watch, so
//! this strategy goes `Anonymous` → *gone*, and comes back in `Restoring` with
//! a `?token=` in the URL. [`AuthPhase::Pending`] is never reported.
//!
//! # Why only *half* of this module is `wasm32`-gated
//!
//! The callback parsing, the claims mapping and the OAuth `state` encoding are
//! ordinary logic with real edge cases, and gating the whole module would mean
//! `cargo test` silently ran none of them — this crate has already been bitten
//! by a native build that compiled a cfg'd-out frontend and reported success.
//! So the browser plumbing is gated and the decisions are not.

// Off-wasm, the callers of the logic below are the tests (and nothing else) —
// the trait impl and every browser helper are `wasm32`-gated. That is the point
// of the split, but it means a native `cargo check` sees the decisions as
// unreached. Silencing it here keeps `-D warnings` honest on both targets
// without hiding the module behind a cfg that would also hide its tests.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::cell::RefCell;
use std::rc::Rc;

use authorizations::SessionClaims;
use serde::{Deserialize, Serialize};

use crate::auth::AuthPhase;
#[cfg(target_arch = "wasm32")]
use crate::auth::{AuthStrategy, Session};

/// Where the auth worker lives, and what to call this app's stored token.
#[derive(Clone, Debug)]
pub struct AuthWorkerConfig {
    /// Base URL of the auth worker — `https://auth.cnft.dev`,
    /// `https://auth.hodlcroft.com`. No trailing slash.
    pub base_url: String,
    /// localStorage key for the token.
    ///
    /// Per-app rather than fixed: two properties served from one origin would
    /// otherwise share a token, and signing out of one would sign you out of
    /// the other. (`discord_auth`'s JWT path *does* use a fixed key, because
    /// that token is deliberately portable across properties. This one is not.)
    pub storage_key: String,
}

impl AuthWorkerConfig {
    pub fn new(base_url: impl Into<String>, storage_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            storage_key: storage_key.into(),
        }
    }
}

/// The identity `/validate` returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub user_id: String,
    pub username: Option<String>,
    pub avatar: Option<String>,
}

impl UserInfo {
    /// Into the estate's shared claims type.
    ///
    /// `ent` is empty and stays empty: this service issues identity, not
    /// authority. An app that needs entitlements from a wallet-style exchange
    /// would add them via [`AuthStrategy::accept_exchange`]; this strategy
    /// declares no exchange.
    fn into_claims(self) -> SessionClaims {
        let mut claims = SessionClaims::for_discord(self.user_id, "");
        if let Some(name) = self.username {
            claims = claims.with_name(name);
        }
        claims.avatar = self.avatar;
        claims
    }
}

/// The `state` round-tripped through the OAuth redirect.
///
/// A typed struct rather than an ad-hoc JSON literal, so the field name cannot
/// drift from what the worker reads on the way back.
#[derive(Serialize)]
struct OAuthState<'a> {
    redirect: &'a str,
}

#[derive(Deserialize)]
struct ValidationResponse {
    valid: bool,
    user: Option<UserInfo>,
    error: Option<String>,
}

/// What a finished `/validate` call produced.
enum Validated {
    Ok(Box<UserInfo>),
    Refused(String),
}

pub struct AuthWorkerStrategy {
    config: AuthWorkerConfig,
    phase: AuthPhase,
    /// Where the async validation lands. `Rc<RefCell<_>>` rather than a channel
    /// because the result is a single value that replaces itself — a queue
    /// would let two stale validations resolve in the wrong order.
    inbox: Rc<RefCell<Option<Validated>>>,
    /// Retained so the session can be rebuilt after a successful validate; the
    /// token is the credential, and `/validate` does not echo it back.
    token: Option<String>,
    /// Heading and subheading on the gate.
    title: String,
    subtitle: String,
}

impl AuthWorkerStrategy {
    pub fn new(config: AuthWorkerConfig) -> Self {
        Self {
            config,
            // Not `Anonymous`: nothing has been read yet, and an app that
            // painted before `restore()` would flash its login screen at a
            // signed-in user.
            phase: AuthPhase::Restoring,
            inbox: Rc::new(RefCell::new(None)),
            token: None,
            title: "Sign in".to_string(),
            subtitle: String::new(),
        }
    }

    /// Branding for the gate screen.
    #[must_use]
    pub fn titled(mut self, title: impl Into<String>, subtitle: impl Into<String>) -> Self {
        self.title = title.into();
        self.subtitle = subtitle.into();
        self
    }

    /// Where the session is.
    ///
    /// Inherent as well as on the trait so it is readable off-target — the
    /// trait impl is `wasm32`-only, and the phase transitions are worth
    /// asserting in an ordinary `cargo test`.
    pub fn phase(&self) -> &AuthPhase {
        &self.phase
    }

    /// Whether this strategy exchanges for entitlements. It does not — the
    /// auth worker issues identity, never authority.
    pub fn has_exchange(&self) -> bool {
        false
    }
}

#[cfg(target_arch = "wasm32")]
impl AuthWorkerStrategy {
    /// Send the browser to Discord. Does not return in practice.
    pub fn begin_login(&self) {
        let current = current_url();
        let state = OAuthState { redirect: &current };
        let Ok(encoded) = serde_json::to_string(&state) else {
            log::error!("could not encode OAuth state");
            return;
        };
        let state_param = js_sys::encode_uri_component(&encoded);
        let url = format!("{}/auth/discord?state={state_param}", self.config.base_url);
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href(&url);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn start_validation(&mut self, token: String) {
        self.phase = AuthPhase::Restoring;
        self.token = Some(token.clone());
        let inbox = Rc::clone(&self.inbox);
        let url = format!("{}/validate", self.config.base_url);

        let mut request = ehttp::Request::get(&url);
        request
            .headers
            .insert("Authorization".to_string(), format!("Bearer {token}"));

        // `fetch_async` + `spawn_local`, not `ehttp::fetch` — the callback form
        // requires `Send`, which `Rc<RefCell<_>>` is not. On wasm there is one
        // thread and `spawn_local` imposes no such bound, so the inbox can stay
        // a plain `Rc` rather than growing an `Arc<Mutex<_>>` that would only
        // ever be contended by itself.
        wasm_bindgen_futures::spawn_local(async move {
            let outcome = match ehttp::fetch_async(request).await {
                Err(e) => Validated::Refused(format!("Request failed: {e}")),
                Ok(response) if !response.ok => {
                    Validated::Refused(format!("HTTP error: {}", response.status))
                }
                Ok(response) => {
                    match serde_json::from_slice::<ValidationResponse>(&response.bytes) {
                        Err(e) => Validated::Refused(format!("Failed to parse response: {e}")),
                        Ok(v) if !v.valid => Validated::Refused(
                            v.error.unwrap_or_else(|| "Token validation failed".into()),
                        ),
                        Ok(v) => match v.user {
                            Some(user) => Validated::Ok(Box::new(user)),
                            None => Validated::Refused("Valid token but no user data".into()),
                        },
                    }
                }
            };
            *inbox.borrow_mut() = Some(outcome);
        });
    }
}

#[cfg(target_arch = "wasm32")]
impl AuthStrategy for AuthWorkerStrategy {
    fn restore(&mut self) {
        // A token in the URL is a fresh callback and wins over stored state —
        // it is the newer fact, and leaving it in the address bar would put a
        // credential in the browser history.
        if let Some(token) = token_from_url() {
            strip_query();
            self.start_validation(token);
            return;
        }
        if let Some(token) = load_token(&self.config.storage_key) {
            self.start_validation(token);
            return;
        }
        self.phase = AuthPhase::Anonymous;
    }

    fn tick(&mut self) {
        let Some(result) = self.inbox.borrow_mut().take() else {
            return;
        };
        match result {
            Validated::Ok(user) => {
                let token = self.token.clone().unwrap_or_default();
                store_token(&self.config.storage_key, &token);
                self.phase = AuthPhase::authenticated(Session {
                    claims: user.into_claims(),
                    credentials: Some(("Authorization".to_string(), format!("Bearer {token}"))),
                    // This service states no expiry. See the module header.
                    expires_at: None,
                });
            }
            Validated::Refused(msg) => {
                // The stored token is now known bad, so drop it — otherwise
                // every reload repeats the same failing round trip.
                clear_token(&self.config.storage_key);
                self.token = None;
                log::warn!("auth validation failed: {msg}");
                // `Anonymous`, not `Failed`: a rejected *stored* token is the
                // ordinary end of a session, not an error the user caused.
                // `Failed` is for a sign-in they just attempted.
                self.phase = AuthPhase::Anonymous;
            }
        }
    }

    fn phase(&self) -> &AuthPhase {
        &self.phase
    }

    fn gate_ui(&mut self, ui: &mut egui::Ui) {
        use crate::theme::{Space, SpaceExt, ThemeExt};
        use egui::{Align, Layout, RichText};

        let color = ui.tokens().color;
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.add_space(ui.available_height() / 3.0);
            ui.label(
                RichText::new(&self.title)
                    .color(color.accent)
                    .heading()
                    .strong(),
            );
            if !self.subtitle.is_empty() {
                ui.add_space(ui.space(Space::Sm));
                ui.label(
                    RichText::new(&self.subtitle)
                        .color(color.text_primary)
                        .size(16.0),
                );
            }
            ui.add_space(ui.space(Space::Xl3));

            if let AuthPhase::Failed(msg) = &self.phase {
                ui.label(RichText::new(msg).color(color.error));
                ui.add_space(ui.space(Space::Xl));
            }

            let button = egui::Button::new(
                RichText::new("Login with Discord")
                    .color(color.text_primary)
                    .size(16.0),
            )
            .fill(color.accent);

            if ui
                .add(button)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.begin_login();
            }
        });
    }

    fn sign_out(&mut self) {
        clear_token(&self.config.storage_key);
        self.token = None;
        self.phase = AuthPhase::Anonymous;
    }
}

// ─── browser plumbing ────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
fn load_token(key: &str) -> Option<String> {
    local_storage()?
        .get_item(key)
        .ok()?
        .filter(|t| !t.is_empty())
}

#[cfg(target_arch = "wasm32")]
fn store_token(key: &str, token: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(key, token);
    }
}

#[cfg(target_arch = "wasm32")]
fn clear_token(key: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(key);
    }
}

#[cfg(target_arch = "wasm32")]
fn current_url() -> String {
    web_sys::window()
        .and_then(|w| w.location().href().ok())
        .unwrap_or_else(|| "/".to_string())
}

#[cfg(target_arch = "wasm32")]
fn token_from_url() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    parse_token(&search)
}

/// Pull `token` out of a query string.
///
/// Split out from the `web_sys` call so it can be tested — this ran untested in
/// `user-portal` and is the one piece of the callback path with real edge cases.
fn parse_token(search: &str) -> Option<String> {
    search
        .trim_start_matches('?')
        .split('&')
        .find_map(|param| param.strip_prefix("token="))
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// Drop the query string, keeping the path.
///
/// `replace_state` rather than assigning `location.search`, which would reload
/// the page and kill the wasm app mid-frame.
#[cfg(target_arch = "wasm32")]
fn strip_query() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(pathname) = window.location().pathname() else {
        return;
    };
    let _ = window
        .history()
        .and_then(|h| h.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&pathname)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Session;

    #[test]
    fn a_callback_token_is_found_wherever_it_sits_in_the_query() {
        assert_eq!(parse_token("?token=abc").as_deref(), Some("abc"));
        assert_eq!(parse_token("token=abc").as_deref(), Some("abc"));
        assert_eq!(
            parse_token("?state=xyz&token=abc&other=1").as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn a_query_with_no_token_yields_none_rather_than_an_empty_session() {
        assert_eq!(parse_token(""), None);
        assert_eq!(parse_token("?"), None);
        assert_eq!(parse_token("?state=xyz"), None);
    }

    #[test]
    fn an_empty_token_is_not_a_token() {
        // `?token=` with nothing after it would otherwise start a validation
        // round trip guaranteed to fail, and land the user on an error screen
        // instead of the login button.
        assert_eq!(parse_token("?token="), None);
        assert_eq!(parse_token("?token=&x=1"), None);
    }

    #[test]
    fn a_param_merely_ending_in_token_is_not_the_token() {
        // `strip_prefix` on the whole param, not `contains`. The original
        // checked `search.contains("token=")` before looping, which would have
        // been satisfied by `csrf_token=`.
        assert_eq!(parse_token("?csrf_token=abc"), None);
        assert_eq!(parse_token("?access_token=abc"), None);
    }

    #[test]
    fn the_oauth_state_names_its_field_redirect() {
        // The worker reads `state.redirect`. A rename here silently sends every
        // user back to the wrong page after login.
        let json = serde_json::to_string(&OAuthState {
            redirect: "https://portal.example/x",
        })
        .unwrap();
        assert_eq!(json, r#"{"redirect":"https://portal.example/x"}"#);
    }

    #[test]
    fn a_validated_user_becomes_claims_with_no_entitlements() {
        let claims = UserInfo {
            user_id: "1234".into(),
            username: Some("Damon".into()),
            avatar: Some("abcdef".into()),
        }
        .into_claims();

        assert_eq!(claims.sub.as_deref(), Some("1234"));
        assert_eq!(claims.name.as_deref(), Some("Damon"));
        assert_eq!(claims.avatar.as_deref(), Some("abcdef"));
        // The whole point of strategy B: identity, never authority.
        assert!(claims.entitlements().is_empty());
    }

    #[test]
    fn a_user_with_no_username_still_produces_usable_claims() {
        let claims = UserInfo {
            user_id: "1234".into(),
            username: None,
            avatar: None,
        }
        .into_claims();
        assert_eq!(claims.sub.as_deref(), Some("1234"));
        assert_eq!(claims.name, None);
        // The badge falls back rather than rendering blank — see
        // `Session::label`.
        assert_eq!(Session::anonymous_ent(claims).label(), "Signed in");
    }

    #[test]
    fn a_fresh_strategy_is_restoring_not_anonymous() {
        // If this ever flips, every reload flashes the login screen before the
        // stored token has been read.
        let s = AuthWorkerStrategy::new(AuthWorkerConfig::new("https://auth.example", "k"));
        assert!(s.phase().is_settling());
        assert!(!s.phase().is_authenticated());
    }

    #[test]
    fn this_strategy_never_exchanges() {
        let s = AuthWorkerStrategy::new(AuthWorkerConfig::new("https://auth.example", "k"));
        assert!(!s.has_exchange());
    }
}
