//! How a collection plays: which trait is an asset's role, what each role
//! does, and how its stats are derived.
//!
//! A card is about one thing: what the asset *is* in the game. A collection's
//! metadata has many traits (background, accessory, strap…), and showing them
//! all reads as a spreadsheet, not a card. Most collections name one trait
//! for what the character is — a role, a class, a type — and that is the one
//! a card leads with.
//!
//! Which trait that is differs by collection, so it is data: a
//! [`CardProfile`] per collection, read from JSON or TOML, written by whoever
//! designs the game. Nothing here invents a role's rules: a role with no text
//! in the profile has none, and a card shows only its name.
//!
//! ```json
//! {
//!   "role": {
//!     "traits": ["role"],
//!     "rules": { "Hacker": "Draw a card when this enters play." }
//!   },
//!   "stats": { "stats": [ { "key": "atk", "label": "ATK", "min": 1, "max": 10,
//!     "traits": [ { "category": "role", "value": "Hacker", "bonus": -1 } ] } ] }
//! }
//! ```
//!
//! A role that should change a stat does it through the stat spec's trait
//! bonuses, matched on the role's category, as above.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::asset::OwnedAsset;
use crate::stats::StatSpec;

/// Which trait is an asset's role, and what each role does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleRule {
    /// Trait categories to take the role from, first match wins, without
    /// regard to case.
    #[serde(default = "RoleRule::default_traits")]
    pub traits: Vec<String>,
    /// Rules text per role, keyed by the role's value (matched without regard
    /// to case). A role missing here has no rules text.
    #[serde(default)]
    pub rules: BTreeMap<String, String>,
}

impl RoleRule {
    fn default_traits() -> Vec<String> {
        ["role", "class", "type"].map(String::from).to_vec()
    }

    /// The asset's role: the first value of the first listed category it has.
    pub fn role<'a>(&self, asset: &'a OwnedAsset) -> Option<&'a str> {
        self.traits.iter().find_map(|k| asset.trait_value(k))
    }

    /// The category the role was read from, for a caller that lists the
    /// other traits and should leave this one out.
    pub fn category<'a>(&'a self, asset: &OwnedAsset) -> Option<&'a str> {
        self.traits
            .iter()
            .find(|k| asset.trait_value(k).is_some())
            .map(String::as_str)
    }

    /// The rules text for a role, if the profile gives one.
    pub fn rules_for(&self, role: &str) -> Option<&str> {
        self.rules
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(role))
            .map(|(_, v)| v.as_str())
    }
}

impl Default for RoleRule {
    /// The role from `role`, else `class`, else `type`; no rules text.
    fn default() -> Self {
        Self {
            traits: Self::default_traits(),
            rules: BTreeMap::new(),
        }
    }
}

/// How one collection plays: its role rule and its stat spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardProfile {
    #[serde(default)]
    pub role: RoleRule,
    pub stats: StatSpec,
}

impl CardProfile {
    /// The default role rule and [`StatSpec::demo`]: for seeing how cards
    /// read before a game has written its own.
    pub fn demo() -> Self {
        Self {
            role: RoleRule::default(),
            stats: StatSpec::demo(),
        }
    }
}

#[cfg(test)]
mod tests {
    use cardano_assets::PolicyId;

    use super::*;

    fn asset(traits: &[(&str, &str)]) -> OwnedAsset {
        OwnedAsset {
            policy_id: PolicyId::new("285c0b8e91ba323da4ca083c9db837e111dafbf3143ece4d03eba8f4")
                .unwrap(),
            asset_name_hex: "546f6f6c6865616430303031".into(),
            image_name_hex: "546f6f6c6865616430303031".into(),
            name: "Toolhead #0001".into(),
            traits: traits
                .iter()
                .map(|(k, v)| (k.to_string(), vec![v.to_string()]))
                .collect(),
            rarity_rank: None,
            owner_account: String::new(),
        }
    }

    #[test]
    fn the_role_is_the_first_listed_category_the_asset_has() {
        let rule = RoleRule::default();
        // Toolheads say `role`; the default finds it.
        let a = asset(&[("role", "Quantum Chemist"), ("body", "Chemical")]);
        assert_eq!(rule.role(&a), Some("Quantum Chemist"));
        assert_eq!(rule.category(&a), Some("role"));
        // A collection that says `Class` (any case) is found too.
        let b = asset(&[("Class", "Mage")]);
        assert_eq!(rule.role(&b), Some("Mage"));
        // No listed category: no role, rather than some other trait.
        assert_eq!(rule.role(&asset(&[("body", "Chemical")])), None);
    }

    #[test]
    fn a_profile_names_its_own_category_and_rules() {
        let profile: CardProfile = serde_json::from_str(
            r#"{
                "role": { "traits": ["body"], "rules": { "chemical": "Heals 1 each turn." } },
                "stats": { "stats": [] }
            }"#,
        )
        .unwrap();
        let a = asset(&[("role", "Quantum Chemist"), ("body", "Chemical")]);
        assert_eq!(profile.role.role(&a), Some("Chemical"));
        assert_eq!(
            profile.role.rules_for("Chemical"),
            Some("Heals 1 each turn.")
        );
        assert_eq!(
            profile.role.rules_for("Hacker"),
            None,
            "no text is invented"
        );
    }

    #[test]
    fn a_profile_without_a_role_rule_uses_the_default() {
        let profile: CardProfile = serde_json::from_str(r#"{ "stats": { "stats": [] } }"#).unwrap();
        assert_eq!(profile.role, RoleRule::default());
    }
}
