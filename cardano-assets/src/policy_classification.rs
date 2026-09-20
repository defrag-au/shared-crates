//! Deciding what a policy *is* — an NFT collection, a currency, or a set of
//! multi-edition pieces — from a sample of the assets minted under it.
//!
//! A policy carries no declaration of its own kind. The only evidence is the
//! assets: what their names are labelled with, what their metadata says, and
//! how much of each exists. This is the one place that evidence is weighed, so
//! every caller reaches the same verdict about the same policy.
//!
//! Pure: takes a [`PolicyAssetSample`] per asset and returns a verdict. It
//! makes no chain calls and knows about no indexer — the caller adapts
//! whatever rows it has into samples. That is deliberate; this logic used to
//! live inside the Maestro client and went dark with it.

use crate::{Cip67Label, TokenType};

/// What a policy was judged to be, and on what evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyClassification {
    pub token_type: TokenType,
    /// Human-readable reason, for logs and operator-facing status.
    pub reason: String,
}

/// One asset's evidence, reduced to the three things the verdict turns on.
#[derive(Debug, Clone)]
pub struct PolicyAssetSample {
    /// Hex asset name, no policy prefix — read for its CIP-67 label.
    pub name_hex: String,
    /// Whether this asset's CIP-25 metadata carries `fungible` / `ticker` /
    /// `decimals` (see `AssetMetadata::has_fungible_signals`).
    pub has_fungible_signals: bool,
    /// Total supply, where known.
    pub total_supply: Option<u64>,
}

/// Few enough assets that a large supply means a currency, not a collection.
const FT_ASSET_COUNT_CEILING: usize = 3;
/// Supply above which a handful of assets reads as a currency.
const FT_SUPPLY_FLOOR: u64 = 1_000;
/// Supply above which multi-edition stops looking like editions.
const RFT_SUPPLY_CEILING: u64 = 1_000_000;

/// Weigh a sample of a policy's assets and decide what the policy is.
///
/// Evidence in descending authority:
/// 1. **CIP-67 labels.** A policy that mints `333` and no collectible labels
///    is a currency; `444` without `222` is editions. These are declarations,
///    not guesses, so they settle it.
/// 2. **CIP-25 fungible signals** — `ticker`, `decimals`, `fungible` in the
///    metadata. Weaker: a collection may carry a stray `decimals`.
/// 3. **Supply and count heuristics.** Only reached when nothing declared
///    itself, and the weakest evidence there is.
///
/// Best-effort: an empty or uninformative sample returns
/// [`TokenType::Unknown`] rather than guessing. Callers should treat a verdict
/// as advisory unless it came from rule 1.
#[must_use]
pub fn classify_policy(assets: &[PolicyAssetSample]) -> PolicyClassification {
    let total_assets = assets.len();

    // 1. CIP-67 labels — the most authoritative signal.
    let mut has_collectible = false;
    let mut has_fungible = false;
    let mut has_rich_fungible = false;
    for asset in assets {
        match Cip67Label::of(&asset.name_hex) {
            // `100` is a collectible's metadata twin — its presence means the
            // policy mints collectibles just as surely as `222` does.
            Some(Cip67Label::UserNft | Cip67Label::Reference) => has_collectible = true,
            Some(Cip67Label::FungibleToken) => has_fungible = true,
            Some(Cip67Label::RichFungible) => has_rich_fungible = true,
            None => {}
        }
    }

    if has_fungible && !has_collectible && !has_rich_fungible {
        return PolicyClassification {
            token_type: TokenType::Ft,
            reason: "CIP-68 fungible token prefix (label 333)".into(),
        };
    }
    if has_rich_fungible && !has_collectible {
        return PolicyClassification {
            token_type: TokenType::Rft,
            reason: "CIP-68 rich fungible token prefix (label 444)".into(),
        };
    }
    if has_collectible {
        return PolicyClassification {
            token_type: TokenType::Nft,
            reason: "CIP-68 NFT prefix (label 222/100)".into(),
        };
    }

    // 2. CIP-25 metadata signals.
    if assets.iter().any(|a| a.has_fungible_signals) {
        return PolicyClassification {
            token_type: TokenType::Ft,
            reason: "CIP-25 metadata contains fungible/ticker/decimals".into(),
        };
    }

    // 3. Supply and count heuristics.
    let supplies: Vec<u64> = assets.iter().filter_map(|a| a.total_supply).collect();
    let max_supply = supplies.iter().copied().max().unwrap_or(0);

    if total_assets <= FT_ASSET_COUNT_CEILING && max_supply > FT_SUPPLY_FLOOR {
        return PolicyClassification {
            token_type: TokenType::Ft,
            reason: format!("{total_assets} asset(s), max supply {max_supply}"),
        };
    }

    if total_assets > FT_ASSET_COUNT_CEILING {
        let multi_supply_count = supplies.iter().filter(|&&s| s > 1).count();
        let multi_ratio = multi_supply_count as f64 / supplies.len().max(1) as f64;
        if multi_ratio > 0.5 && max_supply < RFT_SUPPLY_CEILING {
            return PolicyClassification {
                token_type: TokenType::Rft,
                reason: format!(
                    "{multi_supply_count}/{} assets have supply > 1 (max {max_supply})",
                    supplies.len()
                ),
            };
        }
    }

    if total_assets > FT_ASSET_COUNT_CEILING && max_supply <= 1 {
        return PolicyClassification {
            token_type: TokenType::Nft,
            reason: format!("{total_assets} assets, all supply 1"),
        };
    }

    PolicyClassification {
        token_type: TokenType::Unknown,
        reason: format!("{total_assets} asset(s), max supply {max_supply}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name_hex: &str, total_supply: Option<u64>) -> PolicyAssetSample {
        PolicyAssetSample {
            name_hex: name_hex.to_owned(),
            has_fungible_signals: false,
            total_supply,
        }
    }

    /// `hex("Thing N")`-ish: an unlabelled CIP-25 name.
    fn plain(n: u32, supply: u64) -> PolicyAssetSample {
        sample(&hex::encode(format!("Thing{n}")), Some(supply))
    }

    #[test]
    fn a_333_only_policy_is_a_currency() {
        let verdict = classify_policy(&[sample("0014df10534e454b", Some(1_000_000_000))]);
        assert_eq!(verdict.token_type, TokenType::Ft);
    }

    #[test]
    fn a_333_beside_collectibles_is_not_a_currency() {
        // A collection that also issues a currency under the same policy is
        // still a collection — classifying it FT auto-disables the collection.
        let verdict = classify_policy(&[
            sample("0014df10534e454b", Some(1_000_000_000)),
            sample("000de1405468696e6731", Some(1)),
        ]);
        assert_eq!(verdict.token_type, TokenType::Nft);
    }

    #[test]
    fn a_444_policy_is_rich_fungible_until_a_222_appears() {
        assert_eq!(
            classify_policy(&[sample("001bc2805468696e6731", Some(10))]).token_type,
            TokenType::Rft
        );
        assert_eq!(
            classify_policy(&[
                sample("001bc2805468696e6731", Some(10)),
                sample("000de1405468696e6732", Some(1)),
            ])
            .token_type,
            TokenType::Nft
        );
    }

    /// A reference token alone still proves the policy mints collectibles.
    #[test]
    fn a_lone_reference_token_reads_as_a_collection() {
        assert_eq!(
            classify_policy(&[sample("000643b05468696e6731", Some(1))]).token_type,
            TokenType::Nft
        );
    }

    #[test]
    fn cip25_fungible_signals_beat_the_supply_heuristics() {
        let mut ticker = plain(1, 1);
        ticker.has_fungible_signals = true;
        let verdict = classify_policy(&[ticker, plain(2, 1), plain(3, 1), plain(4, 1)]);
        assert_eq!(verdict.token_type, TokenType::Ft);
    }

    #[test]
    fn a_handful_of_high_supply_assets_is_a_currency() {
        let verdict = classify_policy(&[plain(1, 10_000_000)]);
        assert_eq!(verdict.token_type, TokenType::Ft);
    }

    #[test]
    fn many_single_supply_assets_are_a_collection() {
        let assets: Vec<_> = (0..50).map(|n| plain(n, 1)).collect();
        assert_eq!(classify_policy(&assets).token_type, TokenType::Nft);
    }

    #[test]
    fn mostly_multi_supply_assets_are_editions() {
        let assets: Vec<_> = (0..20).map(|n| plain(n, 25)).collect();
        assert_eq!(classify_policy(&assets).token_type, TokenType::Rft);
    }

    #[test]
    fn no_evidence_is_unknown_not_a_guess() {
        assert_eq!(classify_policy(&[]).token_type, TokenType::Unknown);
        // One unlabelled asset of supply 1 is genuinely ambiguous: it could be
        // a one-of-one or the first mint of anything.
        assert_eq!(
            classify_policy(&[plain(1, 1)]).token_type,
            TokenType::Unknown
        );
    }
}
