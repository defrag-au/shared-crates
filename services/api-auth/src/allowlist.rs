//! Stake-address allowlist for platform admin auth.
//!
//! Two tiers, configured via comma-delimited env vars:
//!
//! - `SUPER_ADMIN_STAKES` — permanent operator wallets. Can do everything,
//!   including the destructive routes (export, offboard, purge).
//! - `WHITELISTED_STAKES` — test / dev wallets. Can provision-as-self and
//!   manage their own clients' wallets, but cannot export / offboard / purge
//!   even on their own client.
//!
//! Overlap rule: a stake in **both** lists is treated as super-admin (the
//! more-permissive tier wins).
//!
//! This module is **pure helpers** — parsing strings and matching against
//! hash sets. The worker layer reads `Env::var(...)` and passes the strings
//! in. Decoupling from the worker lets the parsing/matching be unit-tested
//! without a runtime, and lets callers cache parsed sets across requests if
//! they want to.
//!
//! This module is the data model only; the gate that reads it belongs to the
//! worker's auth layer.

use std::collections::HashSet;

/// Which allowlist tier a stake address belongs to.
///
/// Returned by [`tier_for`]. Callers gate route access on this — e.g. the
/// super-admin-only routes refuse anything below `Tier::SuperAdmin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Permanent operator wallet — full access.
    SuperAdmin,
    /// Test / dev wallet — limited access (provision-as-self + manage own).
    Whitelisted,
    /// Stake not on either list.
    Rejected,
}

impl Tier {
    /// Convenience for "did this auth attempt pass at all".
    pub fn is_allowed(self) -> bool {
        matches!(self, Tier::SuperAdmin | Tier::Whitelisted)
    }
}

/// Parse a comma-delimited list of bech32 stake addresses into a hash set.
///
/// Whitespace around entries is trimmed; empty entries are dropped (so a
/// trailing comma is harmless). No bech32 validation here — that's the
/// signature-verification step's job. An invalid entry just won't match any
/// real signed stake.
pub fn parse_stakes(input: &str) -> HashSet<String> {
    parse_csv(input)
}

/// Split a comma-delimited list into a set, trimming whitespace and dropping
/// empty entries.
///
/// The same shape the env-delivered lists use — stake allowlists and the JWT
/// admin ids — so one parser decides what a trailing comma means.
pub(crate) fn parse_csv(input: &str) -> HashSet<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(String::from)
        .collect()
}

/// Check a stake address against the parsed allowlists.
///
/// Super-admin wins on overlap. Stakes not in either list return `Rejected`.
/// Matching is exact: the caller must pass the canonical form the signature
/// verifier produced, since nothing is normalised here.
pub fn tier_for(stake: &str, super_admin: &HashSet<String>, whitelisted: &HashSet<String>) -> Tier {
    if super_admin.contains(stake) {
        Tier::SuperAdmin
    } else if whitelisted.contains(stake) {
        Tier::Whitelisted
    } else {
        Tier::Rejected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_returns_empty_set() {
        assert!(parse_stakes("").is_empty());
        assert!(parse_stakes("   ").is_empty());
        assert!(parse_stakes(",,,").is_empty());
    }

    #[test]
    fn parse_single_entry() {
        let set = parse_stakes("stake1uxxx");
        assert_eq!(set.len(), 1);
        assert!(set.contains("stake1uxxx"));
    }

    #[test]
    fn parse_multiple_trims_whitespace() {
        let set = parse_stakes("stake1uaaa , stake1ubbb,stake1uccc  ");
        assert_eq!(set.len(), 3);
        assert!(set.contains("stake1uaaa"));
        assert!(set.contains("stake1ubbb"));
        assert!(set.contains("stake1uccc"));
    }

    #[test]
    fn parse_drops_empty_entries() {
        // Trailing comma, double commas, leading comma — all tolerated.
        let set = parse_stakes(",stake1ua,,stake1ub,");
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn tier_for_super_admin_wins() {
        let super_admin = parse_stakes("stake1ushared");
        let whitelisted = parse_stakes("stake1ushared,stake1uother");
        // stake1ushared is in BOTH — super-admin wins.
        assert_eq!(
            tier_for("stake1ushared", &super_admin, &whitelisted),
            Tier::SuperAdmin
        );
    }

    #[test]
    fn tier_for_whitelisted_falls_through() {
        let super_admin = parse_stakes("stake1usuper");
        let whitelisted = parse_stakes("stake1uwhite");
        assert_eq!(
            tier_for("stake1uwhite", &super_admin, &whitelisted),
            Tier::Whitelisted
        );
    }

    #[test]
    fn tier_for_rejected_when_absent() {
        let super_admin = parse_stakes("stake1usuper");
        let whitelisted = parse_stakes("stake1uwhite");
        assert_eq!(
            tier_for("stake1uother", &super_admin, &whitelisted),
            Tier::Rejected
        );
    }

    #[test]
    fn is_allowed_helper_method() {
        assert!(Tier::SuperAdmin.is_allowed());
        assert!(Tier::Whitelisted.is_allowed());
        assert!(!Tier::Rejected.is_allowed());
    }
}
