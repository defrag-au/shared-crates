//! Protocol parameters abstraction for transaction building
//!
//! [`TxBuildParams`] captures the minimum protocol parameters needed to build
//! transactions. Consumers convert from their source-specific types (Maestro,
//! Blockfrost, Koios, etc.) into this common representation.

/// Minimum protocol parameters needed for transaction building.
///
/// These values come from the Cardano node's protocol parameters and are
/// used for fee calculation, min UTxO computation, and size validation.
#[derive(Debug, Clone, Default)]
pub struct TxBuildParams {
    /// Per-byte fee multiplier (Cardano parameter `a`)
    pub min_fee_coefficient: u64,
    /// Fixed fee constant (Cardano parameter `b`) in lovelace
    pub min_fee_constant: u64,
    /// Coins per UTxO byte (Babbage/Conway parameter `coinsPerUTxOByte`)
    pub coins_per_utxo_byte: u64,
    /// Maximum transaction size in bytes
    pub max_tx_size: u32,
    /// Maximum serialised size of the *value* portion of a single UTxO output
    /// (Cardano Conway parameter `maxValueSize`). Outputs whose value exceeds
    /// this limit are rejected by the ledger (`OutputTooBigUTxO`).
    pub max_value_size: u64,
    /// Script execution memory price as (numerator, denominator).
    /// Fee contribution per redeemer = mem_units × numerator / denominator.
    /// `None` for callers that don't need Plutus fee calculation.
    pub price_mem: Option<(u64, u64)>,
    /// Script execution CPU step price as (numerator, denominator).
    /// Fee contribution per redeemer = step_units × numerator / denominator.
    pub price_step: Option<(u64, u64)>,
    /// Cost per byte of reference scripts (Conway parameter `minFeeRefScriptCostPerByte`).
    /// Added to the fee for each byte of script referenced via CIP-33 reference inputs.
    /// Mainnet default: 15 lovelace/byte.
    pub min_fee_ref_script_cost_per_byte: u64,
    /// Total size in bytes of all scripts in reference inputs.
    /// Set by the caller when building Plutus TXs with reference scripts.
    pub ref_script_size: u64,
}

impl From<&maestro::ProtocolParameters> for TxBuildParams {
    fn from(pp: &maestro::ProtocolParameters) -> Self {
        let (price_mem, price_step) = pp
            .script_execution_prices
            .as_ref()
            .map(|ep| (ep.parse_memory(), ep.parse_cpu()))
            .unwrap_or((None, None));

        Self {
            min_fee_coefficient: pp.min_fee_coefficient,
            min_fee_constant: pp.min_fee_constant.ada.lovelace,
            coins_per_utxo_byte: pp.min_utxo_deposit_coefficient,
            // Maestro doesn't expose max_tx_size directly; use Cardano mainnet default
            max_tx_size: 16384,
            max_value_size: 5000,
            price_mem,
            price_step,
            min_fee_ref_script_cost_per_byte: 15,
            ref_script_size: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_maestro_params() {
        let maestro_params = maestro::ProtocolParameters {
            min_fee_coefficient: 44,
            min_fee_constant: maestro::AdaLovelace {
                ada: maestro::AdaAmount { lovelace: 155381 },
            },
            min_utxo_deposit_coefficient: 4310,
            script_execution_prices: Some(maestro::ExecutionPrices {
                memory: "577/10000".to_string(),
                cpu: "721/10000000".to_string(),
            }),
        };

        let params = TxBuildParams::from(&maestro_params);
        assert_eq!(params.min_fee_coefficient, 44);
        assert_eq!(params.min_fee_constant, 155381);
        assert_eq!(params.coins_per_utxo_byte, 4310);
        assert_eq!(params.max_tx_size, 16384);
        assert_eq!(params.max_value_size, 5000);
        assert_eq!(params.price_mem, Some((577, 10000)));
        assert_eq!(params.price_step, Some((721, 10000000)));
    }
}
