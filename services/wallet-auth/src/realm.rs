//! `Realm` — which worker a challenge or session belongs to, and the KV key
//! layout that keeps realms apart inside ONE shared namespace.
//!
//! The session store used to hardcode global key prefixes, so every worker
//! that wanted wallet sign-in needed a KV namespace of its own — otherwise a
//! session issued by one worker (tier decided against ITS allowlist) would
//! have been honoured by another. Scoping the keys by realm removes that
//! coupling: all wallet-authed workers bind the same `WALLET_AUTH` namespace
//! per environment, and a record is only reachable through the realm that
//! wrote it. The record also carries its realm, so a hand-built key for
//! another realm's token still fails the check.
//!
//! ```text
//! auth:{realm}:challenge:{id}      TTL 5 min, single use
//! auth:{realm}:session:{token}     TTL 8 h
//! ```
//!
//! [`Realm::LEGACY`] reproduces the original unscoped prefixes
//! (`auth:challenge:…`, `auth:session:…`) so the client portal's existing
//! namespace keeps working until it migrates.

use std::fmt;

/// A worker's identity in the shared auth namespace. Use the worker's own
/// wrangler `name` (`abandonware-marketplace`, `client-management`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Realm(&'static str);

impl Realm {
    /// The pre-realm key layout, for a namespace that only ever served one
    /// worker. New consumers must not use this.
    pub const LEGACY: Realm = Realm("");

    pub const fn new(name: &'static str) -> Self {
        Realm(name)
    }

    pub fn name(&self) -> &'static str {
        self.0
    }

    pub fn is_legacy(&self) -> bool {
        self.0.is_empty()
    }

    /// KV key for a challenge in this realm.
    pub fn challenge_key(&self, challenge_id: &str) -> String {
        match self.0 {
            "" => format!("auth:challenge:{challenge_id}"),
            realm => format!("auth:{realm}:challenge:{challenge_id}"),
        }
    }

    /// KV key for a session in this realm.
    pub fn session_key(&self, token: &str) -> String {
        match self.0 {
            "" => format!("auth:session:{token}"),
            realm => format!("auth:{realm}:session:{token}"),
        }
    }
}

impl fmt::Display for Realm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_legacy() {
            f.write_str("(legacy)")
        } else {
            f.write_str(self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn realms_do_not_share_keys() {
        let a = Realm::new("abandonware-marketplace");
        let b = Realm::new("client-management");
        assert_ne!(a.session_key("tok"), b.session_key("tok"));
        assert_ne!(a.challenge_key("id"), b.challenge_key("id"));
        assert_eq!(
            a.session_key("tok"),
            "auth:abandonware-marketplace:session:tok"
        );
        assert_eq!(
            a.challenge_key("id"),
            "auth:abandonware-marketplace:challenge:id"
        );
    }

    /// The legacy layout is byte-identical to the prefixes the client portal
    /// shipped with, so its live sessions survive this refactor.
    #[test]
    fn legacy_realm_keeps_the_original_prefixes() {
        assert_eq!(Realm::LEGACY.session_key("tok"), "auth:session:tok");
        assert_eq!(Realm::LEGACY.challenge_key("id"), "auth:challenge:id");
        assert!(Realm::LEGACY.is_legacy());
        assert!(!Realm::new("x").is_legacy());
    }

    /// A realm name cannot be confused with a legacy key by containing the
    /// separator sequence at the front — the scoped form always has the
    /// realm between two colons.
    #[test]
    fn scoped_keys_never_collide_with_legacy_keys() {
        for realm in ["session", "challenge", "auth"] {
            let r = Realm::new(realm);
            assert_ne!(r.session_key("t"), Realm::LEGACY.session_key("t"));
            assert_ne!(r.challenge_key("i"), Realm::LEGACY.challenge_key("i"));
        }
    }
}
