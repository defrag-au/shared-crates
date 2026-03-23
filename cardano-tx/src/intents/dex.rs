use serde::{Deserialize, Serialize};

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
