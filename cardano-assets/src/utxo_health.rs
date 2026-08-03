//! UTxO health classification — the canonical tier rules.
//!
//! Single source of truth for classifying a wallet's UTxOs by "health"
//! (fragmentation / clutter). Consumed by BOTH the egui `utxo_shelf` widget
//! (shared-crates ui/egui-widgets) and the defrackit planner (cardano-tx), so a
//! wallet diagnosis rendered in a frontend and a defrag plan computed in a
//! worker agree by construction. Pure — no I/O, no UI deps.

use crate::utxo::{UtxoApi, UtxoTag};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Pure-ADA UTxOs in `[COLLATERAL_THRESHOLD, COLLATERAL_CEILING]` are treated as
/// dApp collateral and left untouched by cleanup planning.
pub const COLLATERAL_THRESHOLD: u64 = 5_000_000;
/// Above this, pure ADA is more useful as Liquid than reserved collateral.
pub const COLLATERAL_CEILING: u64 = 15_000_000;
/// Asset-bearing UTxOs at or below this lovelace are Dust — near the min-UTxO
/// floor, costing more to spend than the ADA they free.
pub const DUST_THRESHOLD: u64 = 1_500_000;

/// Health tier of a single UTxO. Ordered healthiest-first (top shelf to bottom
/// in the shelf widget's metaphor).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum UtxoTier {
    /// Pure ADA 5-15 ADA. DApp-ready collateral for Plutus interactions.
    Collateral,
    /// Has a datum, script ref, or sits at a script address — contract-locked.
    ScriptLocked,
    /// Pure ADA (no native assets). Freely spendable, low fees.
    Liquid,
    /// Assets from exactly 1 policy. Minimal locked ADA.
    Clean,
    /// Assets from 2-3 policies. More ADA locked than necessary.
    Cluttered,
    /// Assets from 4+ policies. High locked ADA, bloats TX size.
    Bloated,
    /// Near min-UTxO threshold with assets. Costs more to spend than it holds.
    Dust,
}

impl UtxoTier {
    /// All tiers in display order (healthiest first).
    pub fn all() -> &'static [Self] {
        &[
            Self::Collateral,
            Self::ScriptLocked,
            Self::Liquid,
            Self::Clean,
            Self::Cluttered,
            Self::Bloated,
            Self::Dust,
        ]
    }
}

/// Whether a UTxO is script-locked (datum, script ref, or script address).
pub fn is_script_locked(utxo: &UtxoApi) -> bool {
    utxo.has_tag(UtxoTag::HasDatum)
        || utxo.has_tag(UtxoTag::HasScriptRef)
        || utxo.has_tag(UtxoTag::ScriptAddress)
}

/// Number of distinct policies among a UTxO's native assets.
pub fn policy_count(utxo: &UtxoApi) -> usize {
    let mut policies: HashMap<&str, ()> = HashMap::new();
    for aq in &utxo.assets {
        policies.insert(aq.asset_id.policy_id.as_str(), ());
    }
    policies.len()
}

/// Classify one UTxO into its health tier. THE canonical rule — widget and
/// planner both call this.
pub fn classify_utxo(utxo: &UtxoApi) -> UtxoTier {
    if is_script_locked(utxo) {
        // Script-locked (DEX orders, listings, staking scripts) — not freely spendable.
        UtxoTier::ScriptLocked
    } else if utxo.assets.is_empty() {
        if utxo.lovelace >= COLLATERAL_THRESHOLD && utxo.lovelace <= COLLATERAL_CEILING {
            UtxoTier::Collateral
        } else {
            UtxoTier::Liquid
        }
    } else if utxo.lovelace <= DUST_THRESHOLD {
        UtxoTier::Dust
    } else {
        match policy_count(utxo) {
            1 => UtxoTier::Clean,
            2 | 3 => UtxoTier::Cluttered,
            _ => UtxoTier::Bloated,
        }
    }
}

/// Classify a whole wallet with cross-UTxO context.
///
/// Starts from the per-UTxO [`classify_utxo`] tiers, then demotes Dust → Clean
/// for any near-min UTxO whose assets have NO spendable merge partner (no other
/// non-script UTxO holding the same policy). A near-min singleton is already as
/// small as it can get — spending it achieves nothing, and labelling it Dust
/// would make a defrag look like it *created* waste when it merely produced
/// minimal per-policy outputs.
pub fn classify_wallet(utxos: &[UtxoApi]) -> Vec<UtxoTier> {
    // Per policy: number of distinct spendable (non-script) UTxOs holding it.
    let mut policy_utxo_count: HashMap<&str, u32> = HashMap::new();
    for u in utxos {
        if is_script_locked(u) {
            continue;
        }
        let mut seen: HashMap<&str, ()> = HashMap::new();
        for a in &u.assets {
            let p = a.asset_id.policy_id.as_str();
            if seen.insert(p, ()).is_none() {
                *policy_utxo_count.entry(p).or_default() += 1;
            }
        }
    }

    utxos
        .iter()
        .map(|u| {
            let tier = classify_utxo(u);
            if tier == UtxoTier::Dust {
                let mergeable = u.assets.iter().any(|a| {
                    policy_utxo_count
                        .get(a.asset_id.policy_id.as_str())
                        .copied()
                        .unwrap_or(0)
                        > 1
                });
                if mergeable {
                    UtxoTier::Dust
                } else {
                    UtxoTier::Clean
                }
            } else {
                tier
            }
        })
        .collect()
}

/// Estimate min-UTxO lovelace for an output holding `num_assets` assets across
/// `num_policies` policies (Babbage/Conway `coinsPerUTxOByte * (160 + size)`).
/// A sizing *estimate* for planning/display — the tx builder computes the exact
/// figure at build time.
pub fn estimate_min_lovelace(
    coins_per_utxo_byte: u64,
    num_assets: usize,
    num_policies: usize,
) -> u64 {
    const FIXED_OVERHEAD: u64 = 160; // base output size in bytes

    if num_assets == 0 {
        // Pure ADA output: ~27 bytes address + 8 bytes value
        return coins_per_utxo_byte * FIXED_OVERHEAD;
    }

    // Each policy adds ~28 bytes (policy hash), each asset ~12 bytes (name + qty).
    let policy_bytes = num_policies as u64 * 28;
    let asset_bytes = num_assets as u64 * 12;
    coins_per_utxo_byte * (FIXED_OVERHEAD + policy_bytes + asset_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utxo::AssetQuantity;
    use crate::AssetId;

    fn asset(policy: char, name: &str) -> AssetQuantity {
        AssetQuantity {
            asset_id: AssetId::new_unchecked(policy.to_string().repeat(56), hex::encode(name)),
            quantity: 1,
        }
    }

    fn utxo(lovelace: u64, assets: Vec<AssetQuantity>, tags: Vec<UtxoTag>) -> UtxoApi {
        UtxoApi {
            tx_hash: "a".repeat(64),
            output_index: 0,
            lovelace,
            assets,
            tags,
        }
    }

    #[test]
    fn classifies_all_tiers() {
        // Script tag wins over everything else.
        assert_eq!(
            classify_utxo(&utxo(10_000_000, vec![], vec![UtxoTag::HasDatum])),
            UtxoTier::ScriptLocked
        );
        // Pure ADA in the collateral band.
        assert_eq!(
            classify_utxo(&utxo(5_000_000, vec![], vec![])),
            UtxoTier::Collateral
        );
        assert_eq!(
            classify_utxo(&utxo(15_000_000, vec![], vec![])),
            UtxoTier::Collateral
        );
        // Pure ADA outside the band.
        assert_eq!(
            classify_utxo(&utxo(2_000_000, vec![], vec![])),
            UtxoTier::Liquid
        );
        assert_eq!(
            classify_utxo(&utxo(50_000_000, vec![], vec![])),
            UtxoTier::Liquid
        );
        // Asset-bearing near the min-UTxO floor.
        assert_eq!(
            classify_utxo(&utxo(1_200_000, vec![asset('a', "x")], vec![])),
            UtxoTier::Dust
        );
        // Policy-count bands.
        assert_eq!(
            classify_utxo(&utxo(
                3_000_000,
                vec![asset('a', "x"), asset('a', "y")],
                vec![]
            )),
            UtxoTier::Clean
        );
        assert_eq!(
            classify_utxo(&utxo(
                3_000_000,
                vec![asset('a', "x"), asset('b', "y")],
                vec![]
            )),
            UtxoTier::Cluttered
        );
        assert_eq!(
            classify_utxo(&utxo(
                9_000_000,
                vec![
                    asset('a', "1"),
                    asset('b', "2"),
                    asset('c', "3"),
                    asset('d', "4")
                ],
                vec![]
            )),
            UtxoTier::Bloated
        );
    }

    #[test]
    fn wallet_context_demotes_unmergeable_dust() {
        // Two dust UTxOs of the SAME policy: mergeable → both stay Dust.
        // One dust UTxO of a policy found nowhere else: already minimal → Clean.
        let wallet = vec![
            utxo(1_200_000, vec![asset('a', "x")], vec![]),
            utxo(1_200_000, vec![asset('a', "y")], vec![]),
            utxo(1_200_000, vec![asset('b', "z")], vec![]),
        ];
        let tiers = classify_wallet(&wallet);
        assert_eq!(tiers[0], UtxoTier::Dust);
        assert_eq!(tiers[1], UtxoTier::Dust);
        assert_eq!(
            tiers[2],
            UtxoTier::Clean,
            "no merge partner — already minimal"
        );

        // A script-locked partner does NOT make dust mergeable (can't spend it).
        let wallet = vec![
            utxo(1_200_000, vec![asset('c', "x")], vec![]),
            utxo(10_000_000, vec![asset('c', "y")], vec![UtxoTag::HasDatum]),
        ];
        let tiers = classify_wallet(&wallet);
        assert_eq!(tiers[0], UtxoTier::Clean);
    }

    #[test]
    fn min_lovelace_scales_with_contents() {
        let pure = estimate_min_lovelace(4310, 0, 0);
        let one = estimate_min_lovelace(4310, 1, 1);
        let bag = estimate_min_lovelace(4310, 250, 12);
        assert!(pure < one && one < bag);
        assert_eq!(pure, 4310 * 160);
    }
}
