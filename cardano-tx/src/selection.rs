//! UTxO selection algorithms
//!
//! Pure functions for selecting UTxOs to fund transactions. All functions operate
//! on [`UtxoApi`](cardano_assets::UtxoApi) and [`TxBuildParams`](crate::params::TxBuildParams).

use cardano_assets::UtxoApi;

use crate::error::TxBuildError;
use crate::helpers::utxo_query::is_pure_ada_utxo;
use crate::params::TxBuildParams;

/// Select a UTxO with sufficient funds for the transaction.
///
/// Returns the first UTxO whose lovelace >= `amount + estimated_fee`.
pub fn select_utxo_for_amount(
    utxos: &[UtxoApi],
    amount: u64,
    estimated_fee: u64,
) -> Result<&UtxoApi, TxBuildError> {
    let required = amount + estimated_fee;

    utxos
        .iter()
        .find(|utxo| utxo.lovelace >= required)
        .ok_or(TxBuildError::InsufficientFunds {
            needed: required,
            available: utxos.iter().map(|u| u.lovelace).sum(),
        })
}

/// Select a UTxO with sufficient funds, preferring pure ADA UTxOs.
///
/// Useful for minting operations where preserving existing native assets
/// in change outputs adds complexity. Falls back to any sufficient UTxO.
pub fn select_utxo_for_amount_prefer_pure_ada<'a>(
    utxos: &'a [UtxoApi],
    amount: u64,
    estimated_fee: u64,
    params: &TxBuildParams,
) -> Result<&'a UtxoApi, TxBuildError> {
    // Min change UTxO: (160 + 68) * coins_per_utxo_byte
    let min_change_utxo = 228 * params.coins_per_utxo_byte;
    let required = amount + estimated_fee + min_change_utxo;

    let has_sufficient = |utxo: &&UtxoApi| utxo.lovelace >= required;

    // First try: pure ADA UTxO with sufficient funds
    if let Some(utxo) = utxos
        .iter()
        .find(|u| is_pure_ada_utxo(u) && has_sufficient(u))
    {
        return Ok(utxo);
    }

    // Fallback: any UTxO with sufficient funds
    utxos
        .iter()
        .find(|u| has_sufficient(u))
        .ok_or(TxBuildError::InsufficientFunds {
            needed: required,
            available: utxos.iter().map(|u| u.lovelace).sum(),
        })
}

/// Select all UTxOs with extractable ADA for a send-max transaction.
///
/// Returns UTxOs whose lovelace exceeds the minimum required to hold their
/// native assets, meaning there's surplus ADA that can be extracted.
pub fn select_all_utxos_for_max<'a>(
    utxos: &'a [UtxoApi],
    params: &TxBuildParams,
) -> Result<Vec<&'a UtxoApi>, TxBuildError> {
    let usable: Vec<&UtxoApi> = utxos
        .iter()
        .filter(|utxo| {
            let asset_ids: Vec<_> = utxo.assets.iter().map(|a| a.asset_id.clone()).collect();
            let min_required = crate::calculate_min_ada_with_params(
                &maestro::ProtocolParameters {
                    min_fee_coefficient: params.min_fee_coefficient,
                    min_fee_constant: maestro::AdaLovelace {
                        ada: maestro::AdaAmount {
                            lovelace: params.min_fee_constant,
                        },
                    },
                    min_utxo_deposit_coefficient: params.coins_per_utxo_byte,
                },
                &asset_ids,
                &crate::OutputParams { datum_size: None },
            );
            utxo.lovelace > min_required
        })
        .collect();

    if usable.is_empty() {
        return Err(TxBuildError::NoSuitableUtxo);
    }

    Ok(usable)
}

/// Select the best collateral UTxO from wallet UTxOs.
///
/// Plutus script transactions require a collateral input (pure ADA, no native
/// assets). Prefers the smallest pure-ADA UTxO with at least 5 ADA. Falls back
/// to the largest pure-ADA UTxO if none meet the 5 ADA threshold.
pub fn select_collateral(utxos: &[UtxoApi]) -> Option<&UtxoApi> {
    const MIN_COLLATERAL: u64 = 5_000_000; // 5 ADA

    // Prefer smallest pure-ADA UTxO >= 5 ADA (least wasteful)
    let mut candidates: Vec<&UtxoApi> = utxos
        .iter()
        .filter(|u| is_pure_ada_utxo(u) && u.lovelace >= MIN_COLLATERAL)
        .collect();
    candidates.sort_by_key(|u| u.lovelace);

    if let Some(best) = candidates.first() {
        return Some(best);
    }

    // Fallback: largest pure-ADA UTxO (even if < 5 ADA)
    utxos
        .iter()
        .filter(|u| is_pure_ada_utxo(u))
        .max_by_key(|u| u.lovelace)
}

/// Estimate fee for a simple transaction (1 input, 2 outputs, ~300 bytes).
pub fn estimate_simple_fee(params: &TxBuildParams) -> u64 {
    let tx_size_estimate = 300u64;
    params.min_fee_coefficient * tx_size_estimate + params.min_fee_constant
}

#[cfg(test)]
mod tests {
    use super::*;
    use cardano_assets::{AssetId, AssetQuantity};

    fn test_params() -> TxBuildParams {
        TxBuildParams {
            min_fee_coefficient: 44,
            min_fee_constant: 155381,
            coins_per_utxo_byte: 4310,
            max_tx_size: 16384,
            max_value_size: 5000,
        }
    }

    fn make_utxo(lovelace: u64) -> UtxoApi {
        UtxoApi {
            tx_hash: "a".repeat(64),
            output_index: 0,
            lovelace,
            assets: vec![],
            tags: vec![],
        }
    }

    fn make_utxo_with_asset(lovelace: u64) -> UtxoApi {
        let policy = "a".repeat(56);
        UtxoApi {
            tx_hash: "b".repeat(64),
            output_index: 0,
            lovelace,
            assets: vec![AssetQuantity {
                asset_id: AssetId::new(policy, "4e4654".to_string()).unwrap(),
                quantity: 1,
            }],
            tags: vec![],
        }
    }

    #[test]
    fn test_select_utxo_for_amount() {
        let utxos = vec![make_utxo(1_000_000), make_utxo(5_000_000)];
        let result = select_utxo_for_amount(&utxos, 3_000_000, 200_000).unwrap();
        assert_eq!(result.lovelace, 5_000_000);
    }

    #[test]
    fn test_select_utxo_insufficient() {
        let utxos = vec![make_utxo(1_000_000)];
        assert!(select_utxo_for_amount(&utxos, 3_000_000, 200_000).is_err());
    }

    #[test]
    fn test_select_prefers_pure_ada() {
        let utxos = vec![make_utxo_with_asset(10_000_000), make_utxo(10_000_000)];
        let params = test_params();
        let result =
            select_utxo_for_amount_prefer_pure_ada(&utxos, 2_000_000, 200_000, &params).unwrap();
        // Should pick the pure ADA one (second in list)
        assert!(result.assets.is_empty());
    }

    #[test]
    fn test_select_falls_back_to_asset_utxo() {
        let utxos = vec![make_utxo_with_asset(10_000_000), make_utxo(100_000)];
        let params = test_params();
        let result =
            select_utxo_for_amount_prefer_pure_ada(&utxos, 2_000_000, 200_000, &params).unwrap();
        // Pure ADA one is too small, should fall back
        assert!(!result.assets.is_empty());
    }

    #[test]
    fn test_estimate_simple_fee() {
        let params = test_params();
        let fee = estimate_simple_fee(&params);
        // 44 * 300 + 155381 = 13200 + 155381 = 168581
        assert_eq!(fee, 168581);
    }

    #[test]
    fn test_select_all_for_max() {
        let utxos = vec![make_utxo(5_000_000), make_utxo(3_000_000)];
        let params = test_params();
        let result = select_all_utxos_for_max(&utxos, &params).unwrap();
        assert_eq!(result.len(), 2);
    }

    // --- select_collateral tests ---

    #[test]
    fn test_collateral_prefers_smallest_above_threshold() {
        let utxos = vec![
            make_utxo(10_000_000),
            make_utxo(5_000_000),
            make_utxo(50_000_000),
        ];
        let result = select_collateral(&utxos).unwrap();
        assert_eq!(result.lovelace, 5_000_000);
    }

    #[test]
    fn test_collateral_skips_asset_utxos() {
        let utxos = vec![make_utxo_with_asset(20_000_000), make_utxo(6_000_000)];
        let result = select_collateral(&utxos).unwrap();
        assert_eq!(result.lovelace, 6_000_000);
        assert!(result.assets.is_empty());
    }

    #[test]
    fn test_collateral_fallback_below_threshold() {
        // Only pure-ADA UTxOs below 5 ADA — picks the largest
        let utxos = vec![make_utxo(2_000_000), make_utxo(4_000_000)];
        let result = select_collateral(&utxos).unwrap();
        assert_eq!(result.lovelace, 4_000_000);
    }

    #[test]
    fn test_collateral_none_when_no_pure_ada() {
        let utxos = vec![
            make_utxo_with_asset(10_000_000),
            make_utxo_with_asset(20_000_000),
        ];
        assert!(select_collateral(&utxos).is_none());
    }

    #[test]
    fn test_collateral_empty_utxos() {
        let utxos: Vec<UtxoApi> = vec![];
        assert!(select_collateral(&utxos).is_none());
    }
}
