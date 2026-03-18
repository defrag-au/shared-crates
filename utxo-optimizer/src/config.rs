use serde::{Deserialize, Serialize};

/// How to handle ADA-only UTxOs during optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdaStrategy {
    /// Leave ADA-only UTxOs untouched.
    Leave,
    /// Consolidate all ADA-only UTxOs into a single output.
    Rollup,
    /// Split excess ADA (>100 ADA) into 7 outputs at 50/15/10/10/5/5/5%.
    Split,
}

/// Optimization settings — maps to the UI controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizeConfig {
    /// Max tokens from the same policy per output UTxO (10-60, default 30).
    /// Cross-policy mixing uses `bundle_size / 2`.
    pub bundle_size: u32,
    /// Isolate each fungible token policy into its own output(s).
    pub isolate_fungible: bool,
    /// Isolate each non-fungible token policy into its own output(s).
    pub isolate_nonfungible: bool,
    /// How to handle ADA-only UTxOs.
    pub ada_strategy: AdaStrategy,
    /// Collateral preservation settings.
    pub collateral: CollateralConfig,
}

/// Settings for preserving collateral UTxOs during optimization.
///
/// Pure-ADA UTxOs within the target ranges are reserved for DApp interactions
/// and excluded from the optimization working set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollateralConfig {
    /// Number of collateral UTxOs to preserve (0 = don't preserve any).
    pub count: u32,
    /// Target ADA amounts for collateral UTxOs (in lovelace).
    /// The optimizer will try to preserve pure-ADA UTxOs closest to these targets.
    /// Default: [5_000_000] (5 ADA — standard Plutus collateral).
    pub targets_lovelace: Vec<u64>,
    /// Maximum ADA for a UTxO to be considered collateral (lovelace).
    /// Pure-ADA UTxOs above this are treated as regular liquid ADA.
    pub ceiling_lovelace: u64,
}

impl Default for CollateralConfig {
    fn default() -> Self {
        Self {
            count: 3,
            targets_lovelace: vec![5_000_000, 10_000_000],
            ceiling_lovelace: 15_000_000,
        }
    }
}

impl Default for OptimizeConfig {
    fn default() -> Self {
        Self {
            bundle_size: 30,
            isolate_fungible: false,
            isolate_nonfungible: false,
            ada_strategy: AdaStrategy::Rollup,
            collateral: CollateralConfig::default(),
        }
    }
}

/// Cardano protocol parameters needed for size and fee estimation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeParams {
    /// Per-byte fee coefficient (mainnet: 44).
    pub min_fee_coefficient: u64,
    /// Fixed fee constant (mainnet: 155381).
    pub min_fee_constant: u64,
    /// Coins per UTxO byte for min-UTxO calculation (mainnet: 4310).
    pub coins_per_utxo_byte: u64,
    /// Maximum transaction size in bytes (mainnet: 16384).
    pub max_tx_size: u32,
}

impl Default for FeeParams {
    fn default() -> Self {
        Self {
            min_fee_coefficient: 44,
            min_fee_constant: 155381,
            coins_per_utxo_byte: 4310,
            max_tx_size: 16384,
        }
    }
}

impl FeeParams {
    /// Cardano mainnet protocol parameters.
    pub fn mainnet() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = OptimizeConfig::default();
        assert_eq!(config.bundle_size, 30);
        assert!(!config.isolate_fungible);
        assert!(!config.isolate_nonfungible);
        assert_eq!(config.ada_strategy, AdaStrategy::Rollup);
    }

    #[test]
    fn test_default_fee_params() {
        let params = FeeParams::default();
        assert_eq!(params.min_fee_coefficient, 44);
        assert_eq!(params.min_fee_constant, 155381);
        assert_eq!(params.coins_per_utxo_byte, 4310);
        assert_eq!(params.max_tx_size, 16384);
    }
}
