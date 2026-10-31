//! What a bearer token says about its holder, read for display only.
//!
//! auth-service checks the token; this does not, and nothing here is trusted.
//! It reads the payload of the reader's own token so the screen can greet
//! them by name before `/auth/me` answers, and say when the token runs out.
//! The two token families name the same things differently: a `/link-wallet`
//! widget token carries `display_name` and `avatar_hash`, a `discord-auth`
//! session `name` and `avatar`.

use serde::Deserialize;

/// The display claims of a bearer token.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct TokenClaims {
    /// The Discord user id.
    #[serde(default)]
    pub sub: Option<String>,
    #[serde(default, alias = "name")]
    pub display_name: Option<String>,
    #[serde(default, alias = "avatar")]
    pub avatar_hash: Option<String>,
    /// Unix seconds.
    #[serde(default)]
    pub exp: Option<u64>,
}

impl TokenClaims {
    /// The payload of a JWT, read without checking its signature. `None` for
    /// anything that is not three dot-separated parts with a JSON middle.
    pub fn peek(token: &str) -> Option<Self> {
        let mut parts = token.trim().split('.');
        let (_, payload, _) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() {
            return None;
        }
        serde_json::from_slice(&base64url(payload)?).ok()
    }

    /// The CDN URL of the holder's avatar, when the token names one.
    pub fn avatar_url(&self) -> Option<String> {
        let (id, hash) = (self.sub.as_ref()?, self.avatar_hash.as_ref()?);
        let ext = if hash.starts_with("a_") { "gif" } else { "png" };
        Some(format!(
            "https://cdn.discordapp.com/avatars/{id}/{hash}.{ext}?size=128"
        ))
    }
}

/// Base64url without padding (RFC 4648 §5), as JWT segments are written.
fn base64url(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode, the inverse of [`base64url`], for building test tokens.
    fn encode(bytes: &[u8]) -> String {
        const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
            for i in 0..=chunk.len() {
                out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
            }
        }
        out
    }

    fn token(payload: &str) -> String {
        format!("e30.{}.c2ln", encode(payload.as_bytes()))
    }

    #[test]
    fn a_widget_token_names_its_holder() {
        let t = token(r#"{"sub":"179744071361757184","display_name":"boef","avatar_hash":"a_12","exp":1761900000,"action":{"type":"link_wallet"}}"#);
        let c = TokenClaims::peek(&t).expect("readable");
        assert_eq!(c.display_name.as_deref(), Some("boef"));
        assert_eq!(c.exp, Some(1_761_900_000));
        assert_eq!(
            c.avatar_url().as_deref(),
            Some("https://cdn.discordapp.com/avatars/179744071361757184/a_12.gif?size=128")
        );
    }

    #[test]
    fn a_session_token_names_its_holder_under_other_keys() {
        let t = token(r#"{"sub":"1","name":"boef","avatar":"ff"}"#);
        let c = TokenClaims::peek(&t).expect("readable");
        assert_eq!(c.display_name.as_deref(), Some("boef"));
        assert_eq!(c.avatar_hash.as_deref(), Some("ff"));
    }

    #[test]
    fn anything_else_reads_as_nothing() {
        assert_eq!(TokenClaims::peek("not a token"), None);
        assert_eq!(TokenClaims::peek("a.b"), None);
        assert_eq!(TokenClaims::peek("a.!!.c"), None);
        assert_eq!(TokenClaims::peek(&format!("a.{}.c", encode(b"[1]"))), None);
    }
}
