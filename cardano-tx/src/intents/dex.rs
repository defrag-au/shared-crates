use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Supported DEX platforms
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DexPlatform {
    Splash,
    Cswap,
}

impl std::fmt::Display for DexPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Splash => write!(f, "Splash"),
            Self::Cswap => write!(f, "CSWAP"),
        }
    }
}

/// Optimization fee charged when split routing provides benefit.
///
/// Only applied when the split across multiple pools provably yields
/// more tokens than routing through any single pool. Configurable per
/// project — different projects can set different fee amounts and
/// treasury addresses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexSplitFee {
    /// Fee amount in lovelace
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub amount_lovelace: u64,
    /// Treasury/fee recipient address (bech32)
    pub target_address: String,
}

/// DEX order type: market (auto-resolve price with slippage) or limit (explicit price)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "order_type", rename_all = "snake_case")]
pub enum DexOrderType {
    /// Market order: resolve price from the DEX's order book and apply slippage
    Market {
        /// Slippage tolerance in basis points (e.g. 300 = 3%). Default: 500 (5%)
        #[serde(default = "default_slippage_bps")]
        slippage_bps: u32,
    },
    /// Limit order: use an explicit price ratio
    Limit {
        /// Price numerator (output units per input unit)
        #[serde(with = "wasm_safe_serde::u64_required")]
        price_numerator: u64,
        /// Price denominator
        #[serde(with = "wasm_safe_serde::u64_required")]
        price_denominator: u64,
    },
}

fn default_slippage_bps() -> u32 {
    500
}

/// Amount of lovelace to send
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LovelaceAmount {
    /// Send a specific amount
    Specific {
        #[serde(with = "wasm_safe_serde::u64_required")]
        amount: u64,
    },
    /// Send maximum available (wallet balance minus fees and min UTxO requirements)
    WalletMax,
}

/// The literal that selects [`LovelaceAmount::WalletMax`] in text input.
pub const WALLET_MAX_LITERAL: &str = "max";

/// Why a lovelace amount string could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LovelaceAmountParseError {
    input: String,
}

impl fmt::Display for LovelaceAmountParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "expected a lovelace amount or '{WALLET_MAX_LITERAL}', got '{}'",
            self.input
        )
    }
}

impl std::error::Error for LovelaceAmountParseError {}

/// Parse a user-entered amount: a lovelace figure, or `max` to sweep the wallet.
///
/// Every text input for this field advertises `max`, so the parse belongs on the
/// type rather than in each UI — the admin panel promised it in three places
/// while its own handler parsed straight to `u64` and rejected it.
impl FromStr for LovelaceAmount {
    type Err = LovelaceAmountParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();

        if trimmed.eq_ignore_ascii_case(WALLET_MAX_LITERAL) {
            return Ok(Self::WalletMax);
        }

        trimmed
            .parse::<u64>()
            .map(|amount| Self::Specific { amount })
            .map_err(|_| LovelaceAmountParseError {
                input: trimmed.to_string(),
            })
    }
}

/// Renders back into the form [`FromStr`] accepts, so a parsed value can be put
/// straight back into an input box.
impl fmt::Display for LovelaceAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Specific { amount } => write!(f, "{amount}"),
            Self::WalletMax => write!(f, "{WALLET_MAX_LITERAL}"),
        }
    }
}

/// Policy filter for narrowing eligible assets in a wallet.
/// Used by `SendRandomAsset` to select which NFTs are candidates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "filter_type", rename_all = "snake_case")]
pub enum AssetPolicyFilter {
    /// Only assets from these policy IDs
    AnyOf { policy_ids: Vec<String> },
    /// All assets except those from these policy IDs
    AnyExcept { policy_ids: Vec<String> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dex_order_type_market_default_slippage() {
        let json = r#"{"order_type":"market"}"#;
        let order: DexOrderType = serde_json::from_str(json).unwrap();
        match order {
            DexOrderType::Market { slippage_bps } => assert_eq!(slippage_bps, 500),
            _ => panic!("Expected Market"),
        }
    }

    #[test]
    fn test_dex_order_type_limit() {
        let json = r#"{"order_type":"limit","price_numerator":100,"price_denominator":1}"#;
        let order: DexOrderType = serde_json::from_str(json).unwrap();
        match order {
            DexOrderType::Limit {
                price_numerator,
                price_denominator,
            } => {
                assert_eq!(price_numerator, 100);
                assert_eq!(price_denominator, 1);
            }
            _ => panic!("Expected Limit"),
        }
    }

    #[test]
    fn test_lovelace_amount_specific() {
        let json = r#"{"type":"specific","amount":5000000}"#;
        let amount: LovelaceAmount = serde_json::from_str(json).unwrap();
        assert_eq!(amount, LovelaceAmount::Specific { amount: 5_000_000 });
    }

    #[test]
    fn test_lovelace_amount_wallet_max() {
        let json = r#"{"type":"wallet_max"}"#;
        let amount: LovelaceAmount = serde_json::from_str(json).unwrap();
        assert_eq!(amount, LovelaceAmount::WalletMax);
    }

    /// The admin panel's own hint text promises `max`; parsing must honour it,
    /// case and surrounding whitespace included (input boxes deliver both).
    #[test]
    fn lovelace_amount_parses_max() {
        for input in ["max", "MAX", "Max", "  max  ", "\tmax\n"] {
            assert_eq!(
                input.parse::<LovelaceAmount>(),
                Ok(LovelaceAmount::WalletMax),
                "should accept {input:?}"
            );
        }
    }

    #[test]
    fn lovelace_amount_parses_numbers() {
        assert_eq!(
            "5000000".parse::<LovelaceAmount>(),
            Ok(LovelaceAmount::Specific { amount: 5_000_000 })
        );
        assert_eq!(
            " 42 ".parse::<LovelaceAmount>(),
            Ok(LovelaceAmount::Specific { amount: 42 })
        );
    }

    #[test]
    fn lovelace_amount_rejects_garbage_with_a_useful_message() {
        let err = "maximum".parse::<LovelaceAmount>().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("'max'"), "should name the literal: {msg}");
        assert!(msg.contains("maximum"), "should echo the input: {msg}");

        assert!("".parse::<LovelaceAmount>().is_err());
        assert!("-1".parse::<LovelaceAmount>().is_err());
        assert!("1.5".parse::<LovelaceAmount>().is_err());
    }

    /// A parsed value must render back into something `FromStr` accepts, so a
    /// form can round-trip its own state.
    #[test]
    fn lovelace_amount_display_roundtrips() {
        for value in [
            LovelaceAmount::WalletMax,
            LovelaceAmount::Specific { amount: 5_000_000 },
        ] {
            assert_eq!(value.to_string().parse::<LovelaceAmount>(), Ok(value));
        }
    }

    #[test]
    fn test_asset_policy_filter_any_of() {
        let json = r#"{"filter_type":"any_of","policy_ids":["abc123"]}"#;
        let filter: AssetPolicyFilter = serde_json::from_str(json).unwrap();
        match filter {
            AssetPolicyFilter::AnyOf { policy_ids } => {
                assert_eq!(policy_ids, vec!["abc123"]);
            }
            _ => panic!("Expected AnyOf"),
        }
    }
}
