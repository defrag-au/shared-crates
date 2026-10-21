//! Discord login for a macroquad game: the [`crate::session`] logic, reached
//! through a miniquad plugin rather than web-sys.
//!
//! The game links to its worker's login route (mounted from
//! [`crate::worker::begin_login`] and [`crate::worker::finish_login`]); the
//! worker returns the browser to the game with the session in the fragment,
//! and [`Session::load`] picks it up and keeps it in local storage. The game
//! then presents [`Session::token`] to its own server, which verifies it.
//!
//! ```ignore
//! let me = discord_auth::miniquad::Session::load();
//! match &me {
//!     Some(s) => draw_text(&s.identity.label(), ..),
//!     None if login_clicked => discord_auth::miniquad::login("/auth/discord"),
//!     None => {}
//! }
//! ```
//!
//! # Wiring
//!
//! The page loads [`PLUGIN_JS`] before the wasm, like any miniquad plugin;
//! a game's `build.rs` stamps it (macroquad-gamekit's `Plugin::DiscordAuth`
//! does). Without it every import is missing and the bundler refuses the
//! build.
//!
//! Native builds have no page: no login, and [`Session::load`] reads the
//! token from `DISCORD_SESSION` in the environment, for testing a native
//! client against a local server.

pub use crate::session::Identity;
use crate::session::{self, Browser, Held};

/// The plugin's JavaScript, for the game's page.
pub const PLUGIN_JS: &str = include_str!("../js/discord-auth.js");

/// A player's Discord session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// The session token: what the game's server verifies.
    pub token: String,
    pub identity: Identity,
    /// Expiry, unix seconds.
    pub expires_at: Option<u64>,
    /// Whether the login finished on this page load.
    pub fresh: bool,
}

impl Session {
    /// The player's session, if they are logged in: from a login that just
    /// finished, or from storage. An expired session is forgotten. A debug
    /// token, which names nobody, is not a session here.
    pub fn load() -> Option<Self> {
        let loaded = session::load(&Page);
        let Held::Session { token, claims } = loaded.held else {
            return None;
        };
        Some(Self {
            token,
            identity: claims.identity()?,
            expires_at: claims.exp,
            fresh: loaded.fresh,
        })
    }
}

/// Begin a login: send the browser to `route`, the game's worker route that
/// redirects on to Discord (`/auth/discord`).
pub fn login(route: &str) {
    Page.navigate(route);
}

/// Log out: forget the stored session.
pub fn logout() {
    session::clear(&Page);
}

/// The page, through the plugin.
struct Page;

#[cfg(target_arch = "wasm32")]
mod page {
    use sapp_jsutils::JsObject;

    // `#[link(wasm_import_module = "env")]`: without it these imports do not
    // link on rustc 1.96+ (see miniquad-platform).
    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        fn discord_auth_fragment() -> JsObject;
        fn discord_auth_clear_fragment();
        fn discord_auth_get(key: JsObject) -> JsObject;
        fn discord_auth_set(key: JsObject, value: JsObject);
        fn discord_auth_remove(key: JsObject);
        fn discord_auth_navigate(url: JsObject);
        fn discord_auth_now_ms() -> f64;
    }

    fn text(js: JsObject) -> Option<String> {
        let mut s = String::new();
        js.to_string(&mut s);
        (!s.is_empty()).then_some(s)
    }

    impl crate::session::Browser for super::Page {
        fn fragment(&self) -> Option<String> {
            text(unsafe { discord_auth_fragment() })
        }
        fn clear_fragment(&self) {
            unsafe { discord_auth_clear_fragment() }
        }
        fn get(&self, key: &str) -> Option<String> {
            text(unsafe { discord_auth_get(JsObject::string(key)) })
        }
        fn set(&self, key: &str, value: &str) {
            unsafe { discord_auth_set(JsObject::string(key), JsObject::string(value)) }
        }
        fn remove(&self, key: &str) {
            unsafe { discord_auth_remove(JsObject::string(key)) }
        }
        fn now_secs(&self) -> u64 {
            (unsafe { discord_auth_now_ms() } / 1000.0) as u64
        }
        fn navigate(&self, url: &str) {
            unsafe { discord_auth_navigate(JsObject::string(url)) }
        }
    }

    /// The plugin handshake `gl.js` checks: JS-side `version: 1`.
    #[unsafe(no_mangle)]
    extern "C" fn discord_auth_crate_version() -> u32 {
        1
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Browser for Page {
    fn fragment(&self) -> Option<String> {
        None
    }
    fn clear_fragment(&self) {}
    fn get(&self, key: &str) -> Option<String> {
        if key == session::SESSION_KEY {
            std::env::var("DISCORD_SESSION")
                .ok()
                .filter(|t| !t.is_empty())
        } else {
            None
        }
    }
    fn set(&self, _key: &str, _value: &str) {}
    fn remove(&self, _key: &str) {}
    fn now_secs(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
    fn navigate(&self, _url: &str) {}
}
