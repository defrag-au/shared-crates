//! Game numbers for an asset: a rarity tier, and stat scores derived from it.
//!
//! Nothing here comes from the chain. A collection's metadata has traits and
//! a rarity rank, not an attack value; a game decides what those mean. This
//! module is the place that decision is written down, deterministically, so
//! the same asset gets the same numbers in the client that shows them and
//! the worker that checks a result.
//!
//! A [`StatSpec`] says, per stat: its range, how much rarity counts, and which
//! traits add to it. [`StatSpec::roll`] applies it to an [`OwnedAsset`]:
//!
//! ```text
//! value = min + span × ((1 − w) × luck + w × rarity) + trait bonuses, clamped
//! ```
//!
//! where `luck` is the asset's own roll for that stat ([`OwnedAsset::roll`],
//! fixed forever), `rarity` is how rare it is within the collection (1 the
//! rarest, 0 the most common, ½ when unknown) and `w` the stat's
//! `rarity_weight`. Rare assets lean strong; any asset can roll well.

use cardano_assets::AssetRarity;
use serde::{Deserialize, Serialize};

use crate::asset::OwnedAsset;

/// The rarity tier of a rank within a collection of `total`: the bands
/// `cardano-assets` already uses (Legendary the rarest 5 %, then Epic to
/// 25 %, Rare to 50 %, Uncommon to 75 %, Common the rest).
pub fn tier(rank: u32, total: u32) -> AssetRarity {
    AssetRarity::from_rank(rank, total.max(1))
}

/// How rare a rank is, from 1 (rank 1) to 0 (the last rank). `None` without
/// both numbers.
pub fn rarity_fraction(rank: Option<u32>, total: Option<u32>) -> Option<f32> {
    let (rank, total) = (rank?, total?);
    if total <= 1 {
        return Some(1.0);
    }
    let r = rank.clamp(1, total);
    Some(1.0 - (r - 1) as f32 / (total - 1) as f32)
}

impl OwnedAsset {
    /// This asset's tier in a collection of `total`, if it has a rank.
    pub fn tier(&self, total: u32) -> Option<AssetRarity> {
        self.rarity_rank.map(|r| tier(r, total))
    }
}

/// A trait value that adds to a stat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraitBonus {
    /// The trait category, matched without regard to case.
    pub category: String,
    /// The value, matched without regard to case.
    pub value: String,
    /// Added to the stat; negative to take away.
    pub bonus: i32,
}

/// One stat a game derives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatDef {
    /// Stable id: the salt of the asset's roll, so renaming the label keeps
    /// every asset's number.
    pub key: String,
    /// What a card shows.
    pub label: String,
    pub min: i32,
    pub max: i32,
    /// 0 is pure luck, 1 is pure rarity.
    #[serde(default = "half")]
    pub rarity_weight: f32,
    #[serde(default)]
    pub traits: Vec<TraitBonus>,
}

fn half() -> f32 {
    0.5
}

/// Which stats a game derives, in the order a card shows them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatSpec {
    pub stats: Vec<StatDef>,
}

/// One derived stat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stat {
    pub key: String,
    pub label: String,
    pub value: i32,
    pub min: i32,
    pub max: i32,
}

impl Stat {
    /// Where the value sits in its range, 0..=1: a bar's fill.
    pub fn fraction(&self) -> f32 {
        let span = (self.max - self.min).max(1) as f32;
        ((self.value - self.min) as f32 / span).clamp(0.0, 1.0)
    }
}

/// An asset's stats under one spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatBlock {
    pub stats: Vec<Stat>,
}

impl StatBlock {
    /// Every stat's place in its range, averaged, scaled to 0..=100: one
    /// number to sort or match by.
    pub fn power(&self) -> u32 {
        if self.stats.is_empty() {
            return 0;
        }
        let sum: f32 = self.stats.iter().map(Stat::fraction).sum();
        (sum / self.stats.len() as f32 * 100.0).round() as u32
    }

    pub fn get(&self, key: &str) -> Option<&Stat> {
        self.stats.iter().find(|s| s.key == key)
    }
}

impl StatSpec {
    /// Four stats for trying cards out: attack, defence, speed, health.
    /// No traits involved, because which traits matter is collection by
    /// collection. A game writes its own.
    pub fn demo() -> Self {
        let stat = |key: &str, label: &str, min, max| StatDef {
            key: key.to_string(),
            label: label.to_string(),
            min,
            max,
            rarity_weight: 0.4,
            traits: Vec::new(),
        };
        Self {
            stats: vec![
                stat("atk", "ATK", 1, 10),
                stat("def", "DEF", 1, 10),
                stat("spd", "SPD", 1, 10),
                stat("hp", "HP", 10, 30),
            ],
        }
    }

    /// Apply the spec to an asset. `total` is the collection's size, for
    /// placing its rank; without it rarity counts as middling.
    pub fn roll(&self, asset: &OwnedAsset, total: Option<u32>) -> StatBlock {
        let rarity = rarity_fraction(asset.rarity_rank, total).unwrap_or(0.5);
        let stats = self
            .stats
            .iter()
            .map(|d| {
                let (lo, hi) = if d.min <= d.max {
                    (d.min, d.max)
                } else {
                    (d.max, d.min)
                };
                let w = d.rarity_weight.clamp(0.0, 1.0);
                let luck = asset.roll(&d.key, 0, 1000) as f32 / 1000.0;
                let base = lo as f32 + (hi - lo) as f32 * ((1.0 - w) * luck + w * rarity);
                let bonus: i32 = d
                    .traits
                    .iter()
                    .filter(|t| asset.has_trait(&t.category, &t.value))
                    .map(|t| t.bonus)
                    .sum();
                Stat {
                    key: d.key.clone(),
                    label: d.label.clone(),
                    value: (base.round() as i32 + bonus).clamp(lo, hi),
                    min: lo,
                    max: hi,
                }
            })
            .collect();
        StatBlock { stats }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use cardano_assets::PolicyId;

    use super::*;

    fn asset(name_hex: &str, rank: Option<u32>, role: &str) -> OwnedAsset {
        OwnedAsset {
            policy_id: PolicyId::new("285c0b8e91ba323da4ca083c9db837e111dafbf3143ece4d03eba8f4")
                .unwrap(),
            asset_name_hex: name_hex.to_string(),
            image_name_hex: name_hex.to_string(),
            name: name_hex.to_string(),
            traits: BTreeMap::from([("role".to_string(), vec![role.to_string()])]),
            rarity_rank: rank,
            owner_account: String::new(),
        }
    }

    #[test]
    fn tiers_follow_the_cardano_assets_bands() {
        assert_eq!(tier(1, 6600), AssetRarity::Legendary);
        assert_eq!(tier(330, 6600), AssetRarity::Legendary, "5 %");
        assert_eq!(tier(331, 6600), AssetRarity::Epic);
        assert_eq!(tier(3300, 6600), AssetRarity::Rare, "50 %");
        assert_eq!(tier(6600, 6600), AssetRarity::Common);
        // A zero total does not divide by zero.
        let _ = tier(1, 0);
    }

    #[test]
    fn rarity_runs_from_one_at_the_top_to_zero_at_the_bottom() {
        assert_eq!(rarity_fraction(Some(1), Some(100)), Some(1.0));
        assert_eq!(rarity_fraction(Some(100), Some(100)), Some(0.0));
        assert_eq!(rarity_fraction(Some(1), Some(1)), Some(1.0));
        assert_eq!(rarity_fraction(None, Some(100)), None);
        assert_eq!(rarity_fraction(Some(5), None), None);
        // A rank past the end is read as the end.
        assert_eq!(rarity_fraction(Some(500), Some(100)), Some(0.0));
    }

    #[test]
    fn stats_are_fixed_per_asset_and_inside_their_ranges() {
        let spec = StatSpec::demo();
        let a = asset("546f6f6c6865616434393537", Some(1), "Star Legion");
        let block = spec.roll(&a, Some(6600));
        assert_eq!(block, spec.roll(&a, Some(6600)), "deterministic");
        assert_eq!(block.stats.len(), 4);
        for (s, d) in block.stats.iter().zip(&spec.stats) {
            assert!((d.min..=d.max).contains(&s.value), "{s:?}");
        }
        assert!(block.power() <= 100);
    }

    #[test]
    fn the_same_asset_is_never_weaker_for_being_rarer() {
        let spec = StatSpec::demo();
        let common = asset("546f6f6c6865616430303031", Some(6600), "x");
        let rare = OwnedAsset {
            rarity_rank: Some(1),
            ..common.clone()
        };
        let (c, r) = (spec.roll(&common, Some(6600)), spec.roll(&rare, Some(6600)));
        for (cs, rs) in c.stats.iter().zip(&r.stats) {
            assert!(rs.value >= cs.value, "{cs:?} vs {rs:?}");
        }
        assert!(r.power() > c.power());
    }

    #[test]
    fn trait_bonuses_apply_by_case_insensitive_match_and_clamp() {
        let mut spec = StatSpec::demo();
        spec.stats[0].traits.push(TraitBonus {
            category: "ROLE".into(),
            value: "star legion".into(),
            bonus: 100,
        });
        let a = asset("546f6f6c6865616434393537", Some(3000), "Star Legion");
        let block = spec.roll(&a, Some(6600));
        assert_eq!(block.stats[0].value, 10, "clamped to the max");
        // A trait that does not match adds nothing: the same as no bonus at all.
        let b = asset("546f6f6c6865616434393537", Some(3000), "Hacker");
        assert_eq!(
            spec.roll(&b, Some(6600)).stats[0],
            StatSpec::demo().roll(&b, Some(6600)).stats[0]
        );
    }

    #[test]
    fn a_spec_reads_from_json_with_defaults() {
        let spec: StatSpec =
            serde_json::from_str(r#"{"stats":[{"key":"atk","label":"ATK","min":1,"max":5}]}"#)
                .unwrap();
        assert_eq!(spec.stats[0].rarity_weight, 0.5);
        assert!(spec.stats[0].traits.is_empty());
    }
}
