//! One asset a player holds, reduced to what a game needs.

use std::collections::BTreeMap;

use cardano_assets::{Asset, AssetRole, MetadataKind, NftPurpose, PolicyId, UnitStanding};
use image_core::IiifUrl;
use ownership_client::{BundleEntry, TraitData};

/// One asset the player holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedAsset {
    pub policy_id: PolicyId,
    /// The token the player actually **owns**, as the ownership service
    /// reported it.
    ///
    /// Key anything persistent on this one: wounds, levels, cooldowns. For a
    /// CIP-68 collection it differs from [`Self::image_name_hex`], and keying
    /// state on the image token writes a second row for the same asset.
    pub asset_name_hex: String,
    /// The token whose image represents this asset. Art only.
    pub image_name_hex: String,
    /// What to call it on screen. Never raw hex; see [`display_name`].
    pub name: String,
    /// Decoded traits, category to values. Ordered, so iteration is stable.
    pub traits: BTreeMap<String, Vec<String>>,
    /// Rank 1 is the rarest. `None` when the collection has no rarity yet.
    pub rarity_rank: Option<u32>,
    /// The stake address holding it, as the ownership service last saw it.
    pub owner_account: String,
}

impl OwnedAsset {
    /// Build from one ownership-bundle entry.
    ///
    /// `None` for an entry that is not a collectible: a CIP-68 reference
    /// token, the CIP-27 royalty token or a CIP-68 fungible token. Those live
    /// under the policy but are nobody's holding, and a game that offered one
    /// as a champion would be offering the project's metadata vault.
    pub fn from_entry(policy_id: &PolicyId, entry: &BundleEntry) -> Option<Self> {
        if AssetRole::of(&entry.asset_name_hex).standing() != UnitStanding::Unit {
            return None;
        }

        let traits: BTreeMap<String, Vec<String>> = match &entry.trait_data {
            Some(TraitData::Decoded(map)) => {
                map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            }
            // A bitmap needs the collection's schema to read, and a request
            // made by this crate always asks for decoded traits. An asset
            // without them is still a holding; it just has no traits.
            Some(TraitData::Bitmap(_)) | None => BTreeMap::new(),
        };

        // Same order as the ownership service's own `RarityRanks::best`:
        // statistical is what marketplaces show, so it is the number a
        // holder recognises. The legacy top-level rank is the last resort.
        let rarity_rank = entry
            .rarity
            .as_ref()
            .and_then(|r| r.statistical.or(r.information_content).or(r.cnft_tools))
            .or(entry.rarity_rank);

        Some(Self {
            policy_id: policy_id.clone(),
            name: display_name(entry.name.as_deref(), &traits, &entry.asset_name_hex),
            image_name_hex: image_asset_name(&entry.asset_name_hex),
            asset_name_hex: entry.asset_name_hex.clone(),
            traits,
            rarity_rank,
            owner_account: entry.owner_account.clone(),
        })
    }

    /// The first value of a trait category, matching the category name
    /// without regard to case.
    ///
    /// Collections disagree about case (`Background` beside `background`), and
    /// a game's trait mapping should not have to know which one it is
    /// reading.
    pub fn trait_value(&self, category: &str) -> Option<&str> {
        self.trait_values(category)
            .and_then(|values| values.first())
            .map(String::as_str)
    }

    /// Every value of a trait category, matched without regard to case.
    pub fn trait_values(&self, category: &str) -> Option<&[String]> {
        self.traits
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(category))
            .map(|(_, v)| v.as_slice())
    }

    /// Does the asset carry this value in this category? Both compared
    /// without regard to case.
    pub fn has_trait(&self, category: &str, value: &str) -> bool {
        self.trait_values(category)
            .is_some_and(|values| values.iter().any(|v| v.eq_ignore_ascii_case(value)))
    }

    /// An IIIF URL builder for this asset's art, on the default host at the
    /// thumbnail size.
    ///
    /// Aimed at [`Self::image_name_hex`], so a CIP-68 asset resolves to the
    /// reference token's image. Chain `.size()`, `.format()` and `.on()` to
    /// change the rest. A surface under a strict content security policy
    /// passes its own same-origin base to `.on()`.
    pub fn art(&self) -> IiifUrl<'_> {
        IiifUrl::new(self.policy_id.as_str(), &self.image_name_hex)
    }

    /// A number fixed for this asset forever.
    ///
    /// FNV-1a over the policy and the **owned** asset name, so it is the same
    /// on every platform, every build and every run. `std`'s hasher is
    /// neither: it is seeded per process. Use it to seed anything a game
    /// derives from an asset's identity, so the same NFT always gets the same
    /// numbers.
    pub fn seed(&self) -> u64 {
        let mut h = Fnv::new();
        h.write(self.policy_id.as_str().as_bytes());
        h.write(b":");
        h.write(self.asset_name_hex.as_bytes());
        h.finish()
    }

    /// A deterministic number in `lo..=hi` for this asset, per `salt`.
    ///
    /// Different salts give independent rolls from the same asset, so
    /// `roll("attack", 1, 10)` and `roll("speed", 1, 10)` do not move
    /// together. The same asset and salt give the same answer forever. A
    /// reversed range is read the right way round rather than panicking.
    pub fn roll(&self, salt: &str, lo: i32, hi: i32) -> i32 {
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        let mut h = Fnv::new();
        h.write(&self.seed().to_le_bytes());
        h.write(salt.as_bytes());
        let span = (i64::from(hi) - i64::from(lo) + 1) as u64;
        (i64::from(lo) + (h.finish() % span) as i64) as i32
    }
}

/// FNV-1a, 64-bit. Small, stable and specified, which is all a seed needs.
struct Fnv(u64);

impl Fnv {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        Self(Self::OFFSET)
    }

    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// Trait keys that commonly carry a display name, most specific first.
///
/// Collections are inconsistent about this: some put the readable name in
/// metadata (which the ownership service surfaces as `name`), some only in a
/// trait, and some nowhere at all.
const NAME_TRAIT_KEYS: &[&str] = &["name", "Name", "title", "Title"];

/// The name to show for an asset.
///
/// Order: metadata name, then a name-ish trait, then the on-chain asset name
/// decoded from hex, then the hex itself. The last is unreachable in practice
/// but beats panicking or dropping a holding. Blank names count as absent.
///
/// Moved from the missions Activity's roster, where raw hex had shipped mid
/// sentence as a squad member's name.
pub fn display_name(
    metadata_name: Option<&str>,
    traits: &BTreeMap<String, Vec<String>>,
    asset_name_hex: &str,
) -> String {
    if let Some(name) = metadata_name.map(str::trim).filter(|n| !n.is_empty()) {
        return name.to_string();
    }

    for key in NAME_TRAIT_KEYS {
        if let Some(value) = traits
            .get(*key)
            .and_then(|values| values.first())
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            return value.to_string();
        }
    }

    decode_asset_name(asset_name_hex)
}

/// Decode an on-chain asset name to UTF-8, stripping a CIP-68 label first.
///
/// A CIP-68 user token's hex starts with a 4-byte label (`000de140…`), which
/// decodes to control characters and would prepend garbage to every name.
/// Falls back to the hex when it is not hex or not UTF-8.
pub fn decode_asset_name(asset_name_hex: &str) -> String {
    let kind = MetadataKind::guess_id_kind(asset_name_hex);
    let stripped = Asset::strip_metadata_prefix(asset_name_hex, &kind);

    let Ok(bytes) = hex::decode(&stripped) else {
        return asset_name_hex.to_string();
    };
    match String::from_utf8(bytes) {
        // Some collections pad names with NULs to a fixed width.
        Ok(name) => {
            let trimmed = name.trim_matches(char::from(0)).trim().to_string();
            if trimmed.is_empty() {
                asset_name_hex.to_string()
            } else {
                trimmed
            }
        }
        Err(_) => asset_name_hex.to_string(),
    }
}

/// The asset name whose image represents this asset.
///
/// For a CIP-68 user token (label 222) that is the reference token (label
/// 100), where the metadata and so the image live. Everything else is its own
/// image.
pub fn image_asset_name(asset_name_hex: &str) -> String {
    match MetadataKind::guess_id_kind(asset_name_hex) {
        MetadataKind::Cip68(NftPurpose::UserNft) => Asset::get_full_id(
            asset_name_hex,
            &MetadataKind::Cip68(NftPurpose::ReferenceNft),
        ),
        _ => asset_name_hex.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn traits(pairs: &[(&str, &str)]) -> BTreeMap<String, Vec<String>> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), vec![v.to_string()]))
            .collect()
    }

    /// `Toolhead0351`, the hex that shipped to Discord as a member name.
    const TOOLHEAD_HEX: &str = "546f6f6c6865616430333531";
    /// The Toolheads policy.
    const TOOLHEADS: &str = "285c0b8e91ba323da4ca083c9db837e111dafbf3143ece4d03eba8f4";

    fn asset(name_hex: &str) -> OwnedAsset {
        OwnedAsset {
            policy_id: PolicyId::new(TOOLHEADS).unwrap(),
            asset_name_hex: name_hex.to_string(),
            image_name_hex: image_asset_name(name_hex),
            name: decode_asset_name(name_hex),
            traits: traits(&[("Background", "Titanium"), ("role", "Hacker")]),
            rarity_rank: None,
            owner_account: String::new(),
        }
    }

    #[test]
    fn falls_back_to_the_decoded_on_chain_name() {
        assert_eq!(
            display_name(None, &BTreeMap::new(), TOOLHEAD_HEX),
            "Toolhead0351"
        );
    }

    #[test]
    fn metadata_name_wins_then_a_name_trait() {
        assert_eq!(
            display_name(
                Some("Rust-Eye Jane"),
                &traits(&[("name", "Ignored")]),
                TOOLHEAD_HEX
            ),
            "Rust-Eye Jane"
        );
        assert_eq!(
            display_name(None, &traits(&[("Name", "Half-Track")]), TOOLHEAD_HEX),
            "Half-Track"
        );
    }

    #[test]
    fn blank_names_are_not_treated_as_present() {
        assert_eq!(
            display_name(Some("   "), &traits(&[("name", "")]), TOOLHEAD_HEX),
            "Toolhead0351"
        );
    }

    #[test]
    fn cip68_label_is_stripped_before_decoding() {
        let hex = format!("000de140{TOOLHEAD_HEX}");
        assert_eq!(decode_asset_name(&hex), "Toolhead0351");
    }

    #[test]
    fn undecodable_names_fall_back_to_hex_rather_than_panicking() {
        assert_eq!(decode_asset_name("fffe"), "fffe");
        assert_eq!(decode_asset_name("zzzz"), "zzzz");
        // NUL padding to a fixed width is trimmed.
        assert_eq!(
            decode_asset_name(&format!("{TOOLHEAD_HEX}0000")),
            "Toolhead0351"
        );
    }

    #[test]
    fn cip68_art_is_the_reference_token_and_ownership_is_not() {
        // `000de140` is the user token (222), `000643b0` the reference (100).
        let held = asset(&format!("000de140{TOOLHEAD_HEX}"));
        assert_eq!(held.image_name_hex, format!("000643b0{TOOLHEAD_HEX}"));
        assert_eq!(held.asset_name_hex, format!("000de140{TOOLHEAD_HEX}"));
        assert!(
            held.art()
                .build()
                .contains(&format!("{TOOLHEADS}:000643b0{TOOLHEAD_HEX}/full/400,/"))
        );

        // A CIP-25 asset is its own image.
        assert_eq!(asset(TOOLHEAD_HEX).image_name_hex, TOOLHEAD_HEX);
    }

    #[test]
    fn traits_match_without_regard_to_case() {
        let a = asset(TOOLHEAD_HEX);
        assert_eq!(a.trait_value("background"), Some("Titanium"));
        assert_eq!(a.trait_value("ROLE"), Some("Hacker"));
        assert!(a.has_trait("Role", "hacker"));
        assert!(!a.has_trait("Role", "Necromancer"));
        assert_eq!(a.trait_value("missing"), None);
    }

    #[test]
    fn the_seed_is_fixed_for_an_asset_and_differs_between_assets() {
        // Pinned: a change here changes every stat every game derived from
        // it, for every player. FNV-1a 64 of "<policy>:<asset hex>"; the
        // expected value was computed outside this crate.
        let a = asset(TOOLHEAD_HEX);
        assert_eq!(a.seed(), asset(TOOLHEAD_HEX).seed());
        assert_eq!(a.seed(), 0x0f0a_879b_b623_8596, "seed {:#x}", a.seed());
        assert_ne!(a.seed(), asset("546f6f6c6865616430333532").seed());
    }

    #[test]
    fn fnv_matches_the_published_test_vectors() {
        // From the FNV reference test suite: the empty string is the offset
        // basis, and "a" is 0xaf63dc4c8601ec8c.
        assert_eq!(Fnv::new().finish(), 0xcbf2_9ce4_8422_2325);
        let mut h = Fnv::new();
        h.write(b"a");
        assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn rolls_stay_in_range_and_salts_are_independent() {
        let a = asset(TOOLHEAD_HEX);
        for salt in ["attack", "speed", "luck", ""] {
            let r = a.roll(salt, 3, 7);
            assert!((3..=7).contains(&r), "{salt}: {r}");
            assert_eq!(r, a.roll(salt, 3, 7), "deterministic");
        }
        // A reversed range is the same range.
        assert_eq!(a.roll("attack", 7, 3), a.roll("attack", 3, 7));
        // A single-value range has one answer, and the full range does not
        // overflow.
        assert_eq!(a.roll("x", 5, 5), 5);
        let _ = a.roll("x", i32::MIN, i32::MAX);
        // Independence: across many salts, a 1..=10 roll is not constant.
        let distinct: std::collections::BTreeSet<i32> =
            (0..40).map(|i| a.roll(&format!("s{i}"), 1, 10)).collect();
        assert!(distinct.len() > 3, "{distinct:?}");
    }
}
