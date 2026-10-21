//! The frontend half of a session, with no browser in it.
//!
//! A session reaches the page in the URL fragment (`#session=<jwt>`, how
//! [`worker::handle_callback`](crate) delivers it) and is kept in local
//! storage. Reading it is the same logic whatever reaches the browser, so it
//! lives here, against the [`Browser`] trait, and each runtime supplies a thin
//! edge:
//!
//! - `web` ([`crate::web`]), through web-sys, for wasm-bindgen apps;
//! - `miniquad` ([`crate::miniquad`]), through a miniquad plugin, for
//!   macroquad games, whose runtime cannot host wasm-bindgen.
//!
//! The token is decoded **without** verifying its signature: the frontend
//! has no secret and needs none. It reads the claims to show who is logged in
//! and to drop an expired token before presenting it; the server verifies on
//! every use.

use serde::Deserialize;

/// Local storage key of the session token.
pub const SESSION_KEY: &str = "discord_session_jwt";
/// Local storage key of an operator's debug token.
pub const DEBUG_KEY: &str = "discord_debug_token";

/// What the session logic needs from a browser, and nothing more.
pub trait Browser {
    /// The URL fragment, `#` included, or `None` when there is none.
    fn fragment(&self) -> Option<String>;
    /// Remove the fragment from the address bar, without reloading, so a
    /// copied URL does not carry the token.
    fn clear_fragment(&self);
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str);
    fn remove(&self, key: &str);
    /// The wall clock, in unix seconds.
    fn now_secs(&self) -> u64;
    /// Send the browser to `url`.
    fn navigate(&self, url: &str);
}

/// The display claims of a session token. Unverified: see the module docs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Claims {
    /// The Discord user id.
    #[serde(default)]
    pub sub: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    /// Entitlement ids, space separated.
    #[serde(default)]
    pub ent: String,
    /// Expiry, unix seconds.
    #[serde(default)]
    pub exp: Option<u64>,
}

impl Claims {
    /// The Discord identity, when the token names one.
    pub fn identity(&self) -> Option<Identity> {
        (!self.sub.is_empty()).then(|| Identity {
            user_id: self.sub.clone(),
            name: self.name.clone(),
            avatar_hash: self.avatar.clone(),
        })
    }
}

/// The Discord identity carried in a session, for display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub user_id: String,
    pub name: Option<String>,
    pub avatar_hash: Option<String>,
}

impl Identity {
    /// CDN avatar URL, or `None` when the user has no custom avatar (the
    /// caller can fall back to a default badge).
    pub fn avatar_url(&self) -> Option<String> {
        let hash = self.avatar_hash.as_ref()?;
        let ext = if hash.starts_with("a_") { "gif" } else { "png" };
        Some(format!(
            "https://cdn.discordapp.com/avatars/{}/{hash}.{ext}?size=64",
            self.user_id
        ))
    }

    /// Best display label: name, else a short user-id.
    pub fn label(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("user {}", self.user_id))
    }
}

/// What the page holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Held {
    /// A session token, unexpired, and its claims.
    Session {
        token: String,
        claims: Claims,
    },
    /// An operator's debug token: not a session, sent as `X-Debug-Token`.
    Debug {
        token: String,
    },
    Anonymous,
}

/// A [`load`]: what the page holds, and whether it arrived in the fragment on
/// this page load (a login just finished) rather than from storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loaded {
    pub held: Held,
    pub fresh: bool,
}

/// Resolve the session: from the URL fragment (`#session=<jwt>` or
/// `#debug=<token>`), stored and then stripped from the address bar; else
/// from storage. An expired token is dropped from storage and reads as
/// anonymous. Fragments that carry neither (`#/route`) are left alone.
pub fn load(browser: &impl Browser) -> Loaded {
    if let Some(frag) = browser.fragment() {
        for part in frag.trim_start_matches('#').split('&') {
            if let Some(token) = part.strip_prefix("session=") {
                browser.set(SESSION_KEY, token);
                browser.clear_fragment();
                return Loaded {
                    held: session(browser, token),
                    fresh: true,
                };
            }
            if let Some(token) = part.strip_prefix("debug=") {
                browser.set(DEBUG_KEY, token);
                browser.clear_fragment();
                return Loaded {
                    held: Held::Debug {
                        token: token.to_string(),
                    },
                    fresh: true,
                };
            }
        }
    }
    let held = if let Some(token) = browser.get(SESSION_KEY) {
        session(browser, &token)
    } else if let Some(token) = browser.get(DEBUG_KEY) {
        Held::Debug { token }
    } else {
        Held::Anonymous
    };
    Loaded { held, fresh: false }
}

/// Log out: forget every stored token.
pub fn clear(browser: &impl Browser) {
    browser.remove(SESSION_KEY);
    browser.remove(DEBUG_KEY);
}

/// A stored token as what it holds: an expired one is dropped, since it
/// would only bounce off the server.
fn session(browser: &impl Browser, token: &str) -> Held {
    let claims = decode_claims(token).unwrap_or_default();
    if claims.exp.is_some_and(|exp| browser.now_secs() >= exp) {
        browser.remove(SESSION_KEY);
        return Held::Anonymous;
    }
    Held::Session {
        token: token.to_string(),
        claims,
    }
}

/// A JWT's payload claims, without verifying its signature.
pub fn decode_claims(jwt: &str) -> Option<Claims> {
    let payload = jwt.split('.').nth(1)?;
    serde_json::from_slice(&base64url_decode(payload)?).ok()
}

/// Base64url, padded or not.
fn base64url_decode(s: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        } as u32)
    };
    let digits = s.trim_end_matches('=').as_bytes();
    if digits.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= value(*c)? << (18 - 6 * i);
        }
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Fake {
        fragment: RefCell<Option<String>>,
        store: RefCell<BTreeMap<String, String>>,
        now: u64,
        went: RefCell<Option<String>>,
    }

    impl Browser for Fake {
        fn fragment(&self) -> Option<String> {
            self.fragment.borrow().clone()
        }
        fn clear_fragment(&self) {
            *self.fragment.borrow_mut() = None;
        }
        fn get(&self, key: &str) -> Option<String> {
            self.store.borrow().get(key).cloned()
        }
        fn set(&self, key: &str, value: &str) {
            self.store
                .borrow_mut()
                .insert(key.to_string(), value.to_string());
        }
        fn remove(&self, key: &str) {
            self.store.borrow_mut().remove(key);
        }
        fn now_secs(&self) -> u64 {
            self.now
        }
        fn navigate(&self, url: &str) {
            *self.went.borrow_mut() = Some(url.to_string());
        }
    }

    fn base64url(bytes: &[u8]) -> String {
        const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut s = String::new();
        for chunk in bytes.chunks(3) {
            let mut n = 0u32;
            for (i, b) in chunk.iter().enumerate() {
                n |= u32::from(*b) << (16 - 8 * i);
            }
            for i in 0..=chunk.len() {
                s.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
        s
    }

    fn jwt(payload: &str) -> String {
        format!("e30.{}.sig", base64url(payload.as_bytes()))
    }

    #[test]
    fn base64url_round_trips_every_length() {
        for len in 0..40u8 {
            let bytes: Vec<u8> = (0..len).map(|i| i.wrapping_mul(37)).collect();
            assert_eq!(base64url_decode(&base64url(&bytes)), Some(bytes));
        }
        assert_eq!(base64url_decode("a"), None);
        assert_eq!(base64url_decode("ab!c"), None);
    }

    #[test]
    fn a_fresh_login_is_stored_and_stripped_from_the_address_bar() {
        let token = jwt(r#"{"sub":"42","name":"Ada","exp":2000}"#);
        let page = Fake {
            now: 1000,
            ..Fake::default()
        };
        *page.fragment.borrow_mut() = Some(format!("#session={token}"));
        let loaded = load(&page);
        assert!(loaded.fresh);
        let Held::Session {
            token: held,
            claims,
        } = loaded.held
        else {
            panic!("expected a session");
        };
        assert_eq!(held, token);
        assert_eq!(claims.identity().map(|i| i.label()), Some("Ada".into()));
        assert_eq!(page.fragment(), None);
        assert_eq!(page.get(SESSION_KEY), Some(token));
        // The next page load finds it in storage.
        assert!(!load(&page).fresh);
    }

    #[test]
    fn an_expired_token_reads_as_anonymous_and_is_forgotten() {
        let page = Fake {
            now: 3000,
            ..Fake::default()
        };
        page.set(SESSION_KEY, &jwt(r#"{"sub":"42","exp":2000}"#));
        assert_eq!(load(&page).held, Held::Anonymous);
        assert_eq!(page.get(SESSION_KEY), None);
    }

    #[test]
    fn a_route_fragment_is_left_alone() {
        let page = Fake::default();
        *page.fragment.borrow_mut() = Some("#/settings".into());
        assert_eq!(load(&page).held, Held::Anonymous);
        assert_eq!(page.fragment(), Some("#/settings".into()));
    }

    #[test]
    fn logging_out_forgets_both_tokens() {
        let page = Fake::default();
        page.set(SESSION_KEY, "a.b.c");
        page.set(DEBUG_KEY, "d");
        clear(&page);
        assert_eq!(load(&page).held, Held::Anonymous);
        page.navigate("/auth/discord");
        assert_eq!(page.went.borrow().as_deref(), Some("/auth/discord"));
    }
}
