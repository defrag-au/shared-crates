//! Mint supply — how many copies of a master may be minted.

use serde::{Deserialize, Serialize};

/// How many copies of a master may be minted — the per-row supply ceiling
/// (e.g. a launchpad's `collection_assets.max_supply` column, or a mint
/// manifest's `max_supply` field).
///
/// A unique 1-of-1 is `Quota(1)`; an editioned / TCG master is `Quota(n)`;
/// `Uncapped` mints on demand with no ceiling. Modelled as an explicit enum
/// (not a bare `Option<i64>`) so "no ceiling" reads as `MintSupply::Uncapped`
/// rather than an ambiguous `None`.
///
/// **DB / nullable-column mapping:** `Quota(n)` ↔ the integer `n`; `Uncapped` ↔
/// SQL `NULL`. The JSON/wire form mirrors that — a number is a quota, JSON
/// `null` is uncapped — see the custom `Serialize`/`Deserialize`. Convert at the
/// storage boundary with [`to_db`](Self::to_db) / [`from_db`](Self::from_db).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MintSupply {
    /// Finite print run of `n` copies (`n >= 1`).
    Quota(u32),
    /// No ceiling — mint on demand. Stored as SQL `NULL`.
    Uncapped,
}

impl Default for MintSupply {
    /// A unique 1-of-1 — the safe default for a drop.
    fn default() -> Self {
        MintSupply::Quota(1)
    }
}

impl MintSupply {
    /// Value to bind to a nullable supply column: `Some(n)` for a quota, `None`
    /// (→ SQL `NULL`) for uncapped.
    pub fn to_db(self) -> Option<i64> {
        match self {
            MintSupply::Quota(n) => Some(n as i64),
            MintSupply::Uncapped => None,
        }
    }

    /// Rehydrate from a nullable supply column: a non-`NULL` value is a quota
    /// (clamped to `>= 0`), `NULL` is uncapped.
    pub fn from_db(value: Option<i64>) -> Self {
        match value {
            Some(n) => MintSupply::Quota(n.max(0) as u32),
            None => MintSupply::Uncapped,
        }
    }
}

impl Serialize for MintSupply {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            MintSupply::Quota(n) => serializer.serialize_u32(*n),
            MintSupply::Uncapped => serializer.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for MintSupply {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // A number → quota; JSON `null` → uncapped. Mirrors the nullable column.
        Ok(match Option::<u32>::deserialize(deserializer)? {
            Some(n) => MintSupply::Quota(n),
            None => MintSupply::Uncapped,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The three JSON cases must stay distinguishable: an absent key is the
    // default unique 1-of-1, a number is a quota, explicit `null` is uncapped.
    #[derive(serde::Deserialize)]
    struct SupplyHolder {
        #[serde(default)]
        max_supply: MintSupply,
    }

    #[test]
    fn absent_key_is_unique_quota_1() {
        let h: SupplyHolder = serde_json::from_str("{}").unwrap();
        assert_eq!(h.max_supply, MintSupply::Quota(1));
    }

    #[test]
    fn number_is_quota() {
        let h: SupplyHolder = serde_json::from_str(r#"{"max_supply":50}"#).unwrap();
        assert_eq!(h.max_supply, MintSupply::Quota(50));
        assert_eq!(
            serde_json::from_str::<MintSupply>("7").unwrap(),
            MintSupply::Quota(7)
        );
    }

    #[test]
    fn explicit_null_is_uncapped() {
        let h: SupplyHolder = serde_json::from_str(r#"{"max_supply":null}"#).unwrap();
        assert_eq!(h.max_supply, MintSupply::Uncapped);
    }

    #[test]
    fn serialize_mirrors_db() {
        assert_eq!(serde_json::to_string(&MintSupply::Quota(50)).unwrap(), "50");
        assert_eq!(serde_json::to_string(&MintSupply::Uncapped).unwrap(), "null");
    }

    #[test]
    fn db_roundtrip() {
        assert_eq!(MintSupply::Quota(50).to_db(), Some(50));
        assert_eq!(MintSupply::Uncapped.to_db(), None);
        assert_eq!(MintSupply::from_db(Some(50)), MintSupply::Quota(50));
        assert_eq!(MintSupply::from_db(None), MintSupply::Uncapped);
    }
}
