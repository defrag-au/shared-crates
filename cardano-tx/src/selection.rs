//! UTxO selection algorithms
//!
//! Pure functions for selecting UTxOs to fund transactions. All functions operate
//! on [`UtxoApi`](cardano_assets::UtxoApi) and [`TxBuildParams`](crate::params::TxBuildParams).

use cardano_assets::UtxoApi;

use crate::error::TxBuildError;
use crate::helpers::utxo_query::is_pure_ada_utxo;
use crate::params::TxBuildParams;

/// Tuning knobs for single-UTxO selection. Defaults match the policy every
/// fee-paying call site wants:
///
/// - **Prefer pure-ADA** UTxOs so the change output stays simple (no native
///   asset bookkeeping).
/// - **Smallest sufficient** candidate — minimises ADA locked in the TX and
///   reduces the chance of picking large consolidated UTxOs that carry hidden
///   datums.
/// - **Fall back to asset-bearing** UTxOs when no pure-ADA candidate is large
///   enough, so a wallet that only holds asset UTxOs still works.
///
/// The few call sites that need stricter behaviour (e.g. dedicated collateral
/// selection) can use [`Self::pure_ada_only`] or override individual flags
/// via the builder methods.
#[derive(Debug, Clone, Copy)]
pub struct UtxoSelectionConfig<'a> {
    params: &'a TxBuildParams,
    prefer_pure_ada: bool,
    allow_asset_fallback: bool,
}

impl<'a> UtxoSelectionConfig<'a> {
    /// Sensible defaults: prefer pure-ADA, smallest sufficient, fall back to
    /// asset-bearing UTxOs when no pure-ADA candidate fits.
    pub fn new(params: &'a TxBuildParams) -> Self {
        Self {
            params,
            prefer_pure_ada: true,
            allow_asset_fallback: true,
        }
    }

    /// Strict pure-ADA selection — no fallback to asset-bearing UTxOs.
    /// Errors with [`TxBuildError::NoPureAdaUtxoLargeEnough`] when no pure-ADA
    /// candidate covers the requested amount.
    pub fn pure_ada_only(params: &'a TxBuildParams) -> Self {
        Self {
            params,
            prefer_pure_ada: true,
            allow_asset_fallback: false,
        }
    }

    /// Override pure-ADA preference. With `false`, asset and pure-ADA UTxOs
    /// compete purely on size; the smallest sufficient one wins.
    pub fn prefer_pure_ada(mut self, v: bool) -> Self {
        self.prefer_pure_ada = v;
        self
    }

    /// Override the asset-bearing fallback. With `false`, callers get a hard
    /// error when no pure-ADA UTxO is large enough.
    pub fn allow_asset_fallback(mut self, v: bool) -> Self {
        self.allow_asset_fallback = v;
        self
    }
}

/// Select a single UTxO to fund a transaction's `amount + estimated_fee`,
/// reserving min-change overhead so the TX has room for a change output.
///
/// Behaviour is governed by [`UtxoSelectionConfig`] — see its docs for the
/// default policy. The "best" candidate is the **smallest sufficient** UTxO.
///
/// # Errors
///
/// - [`TxBuildError::NoUtxoCandidates`] — wallet has no UTxOs at all.
/// - [`TxBuildError::NoPureAdaUtxoLargeEnough`] — pure-ADA-only mode and no
///   pure-ADA UTxO covers the threshold.
/// - [`TxBuildError::NoSingleUtxoLargeEnough`] — wallet has UTxOs but none
///   alone covers the threshold. Carries `largest` + `total` so callers can
///   distinguish "consolidate" from "top up."
pub fn select_utxo_for_amount<'a>(
    utxos: &'a [UtxoApi],
    amount: u64,
    estimated_fee: u64,
    config: &UtxoSelectionConfig,
) -> Result<&'a UtxoApi, TxBuildError> {
    if utxos.is_empty() {
        return Err(TxBuildError::NoUtxoCandidates);
    }

    // Min change UTxO overhead: (160 + 68) * coins_per_utxo_byte. Same
    // baseline whether the change ends up pure-ADA or asset-bearing — the
    // asset case may need a larger min-ADA at TX-build time but the
    // selection threshold itself stays consistent so callers see stable
    // behaviour.
    let min_change_utxo = 228 * config.params.coins_per_utxo_byte;
    let required = amount + estimated_fee + min_change_utxo;
    let has_sufficient = |u: &&UtxoApi| u.lovelace >= required;

    // Phase 1: pure-ADA candidates (when preference is on). Smallest
    // sufficient wins.
    if config.prefer_pure_ada {
        if let Some(utxo) = utxos
            .iter()
            .filter(|u| is_pure_ada_utxo(u) && has_sufficient(u))
            .min_by_key(|u| u.lovelace)
        {
            return Ok(utxo);
        }
    }

    // Phase 2: any UTxO. Used when:
    //   - prefer_pure_ada is off (treat all UTxOs equally), or
    //   - prefer_pure_ada is on but no pure-ADA candidate fit AND fallback is
    //     allowed.
    if !config.prefer_pure_ada || config.allow_asset_fallback {
        if let Some(utxo) = utxos
            .iter()
            .filter(|u| has_sufficient(u))
            .min_by_key(|u| u.lovelace)
        {
            return Ok(utxo);
        }
    }

    // No suitable UTxO. Build the most informative error we can.
    if config.prefer_pure_ada && !config.allow_asset_fallback {
        let pure_ada: Vec<&UtxoApi> = utxos.iter().filter(|u| is_pure_ada_utxo(u)).collect();
        let largest_pure_ada = pure_ada.iter().map(|u| u.lovelace).max().unwrap_or(0);
        let total_pure_ada = pure_ada.iter().map(|u| u.lovelace).sum();
        return Err(TxBuildError::NoPureAdaUtxoLargeEnough {
            needed: required,
            largest_pure_ada,
            total_pure_ada,
        });
    }

    let largest = utxos.iter().map(|u| u.lovelace).max().unwrap_or(0);
    let total = utxos.iter().map(|u| u.lovelace).sum();
    Err(TxBuildError::NoSingleUtxoLargeEnough {
        needed: required,
        largest,
        total,
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
                    script_execution_prices: None,
                    max_execution_units_per_transaction: None,
                    max_transaction_size: None,
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

/// Select multiple UTxOs to cover a required amount.
///
/// Uses a greedy algorithm: sorts UTxOs by lovelace descending, takes until
/// the accumulated lovelace meets the target. Prefers pure ADA UTxOs first
/// to avoid unnecessary native asset handling in change outputs.
///
/// Returns the selected UTxOs and total lovelace they contain.
pub fn select_utxos_for_amount(
    utxos: &[UtxoApi],
    amount: u64,
    estimated_fee: u64,
    params: &TxBuildParams,
) -> Result<Vec<UtxoApi>, TxBuildError> {
    let min_change_utxo = 228 * params.coins_per_utxo_byte;
    let required = amount + estimated_fee + min_change_utxo;

    // First try: single UTxO (cheapest TX)
    if let Some(utxo) = utxos
        .iter()
        .filter(|u| is_pure_ada_utxo(u) && u.lovelace >= required)
        .min_by_key(|u| u.lovelace)
    {
        return Ok(vec![utxo.clone()]);
    }

    if let Some(utxo) = utxos
        .iter()
        .filter(|u| u.lovelace >= required)
        .min_by_key(|u| u.lovelace)
    {
        return Ok(vec![utxo.clone()]);
    }

    // Multi-UTxO selection: accumulate until we have enough.
    // Sort: pure ADA first, then by lovelace descending within each group.
    let mut sorted: Vec<&UtxoApi> = utxos.iter().collect();
    sorted.sort_by(|a, b| {
        let a_pure = is_pure_ada_utxo(a);
        let b_pure = is_pure_ada_utxo(b);
        b_pure.cmp(&a_pure).then(b.lovelace.cmp(&a.lovelace))
    });

    let mut selected = Vec::new();
    let mut accumulated = 0u64;

    // Each additional input adds ~44 bytes to the TX = ~1936 lovelace extra fee
    let per_input_fee_overhead = 44 * params.min_fee_coefficient;

    for utxo in sorted {
        if accumulated >= required {
            break;
        }
        selected.push(utxo.clone());
        accumulated += utxo.lovelace;
        // Account for the extra fee from additional inputs
        if selected.len() > 1 {
            // required grows slightly with each additional input
        }
    }

    // Check if we have enough including the multi-input fee overhead
    let total_fee_overhead = (selected.len().saturating_sub(1) as u64) * per_input_fee_overhead;
    let adjusted_required = required + total_fee_overhead;

    if accumulated < adjusted_required {
        return Err(TxBuildError::InsufficientFunds {
            needed: adjusted_required,
            available: accumulated,
        });
    }

    Ok(selected)
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
            price_mem: None,
            price_step: None,
            ..Default::default()
        }
    }

    fn make_utxo(lovelace: u64) -> UtxoApi {
        make_utxo_with_id("a".repeat(64), 0, lovelace, false)
    }

    fn make_utxo_with_asset(lovelace: u64) -> UtxoApi {
        make_utxo_with_id("b".repeat(64), 0, lovelace, true)
    }

    fn make_utxo_with_id(
        tx_hash: String,
        output_index: u32,
        lovelace: u64,
        with_asset: bool,
    ) -> UtxoApi {
        let assets = if with_asset {
            vec![AssetQuantity {
                asset_id: AssetId::new("a".repeat(56), "4e4654".to_string()).unwrap(),
                quantity: 1,
            }]
        } else {
            vec![]
        };
        UtxoApi {
            tx_hash,
            output_index,
            lovelace,
            assets,
            tags: vec![],
        }
    }

    // -----------------------------------------------------------------
    // select_utxo_for_amount + UtxoSelectionConfig — exhaustive coverage
    // -----------------------------------------------------------------

    #[test]
    fn empty_utxos_returns_no_utxo_candidates() {
        let params = test_params();
        let config = UtxoSelectionConfig::new(&params);
        let result = select_utxo_for_amount(&[], 0, 200_000, &config);
        assert!(matches!(result, Err(TxBuildError::NoUtxoCandidates)));
    }

    #[test]
    fn single_pure_ada_sufficient_returns_it() {
        let utxos = vec![make_utxo(5_000_000)];
        let params = test_params();
        let config = UtxoSelectionConfig::new(&params);
        let result = select_utxo_for_amount(&utxos, 2_000_000, 200_000, &config).unwrap();
        assert_eq!(result.lovelace, 5_000_000);
    }

    #[test]
    fn single_pure_ada_insufficient_returns_no_single_utxo_large_enough() {
        let utxos = vec![make_utxo(1_000_000)];
        let params = test_params();
        let config = UtxoSelectionConfig::new(&params);
        let result = select_utxo_for_amount(&utxos, 3_000_000, 200_000, &config);
        match result {
            Err(TxBuildError::NoSingleUtxoLargeEnough {
                needed,
                largest,
                total,
            }) => {
                assert!(needed > 3_000_000, "needed includes fee + min-change");
                assert_eq!(largest, 1_000_000);
                assert_eq!(total, 1_000_000);
            }
            other => panic!("expected NoSingleUtxoLargeEnough, got {other:?}"),
        }
    }

    #[test]
    fn picks_smallest_sufficient_pure_ada() {
        // Multiple pure-ADA UTxOs all sufficient; smallest-sufficient wins
        // (don't lock more ADA than necessary).
        let utxos = vec![
            make_utxo_with_id("a".repeat(64), 0, 100_000_000, false),
            make_utxo_with_id("a".repeat(64), 1, 5_000_000, false),
            make_utxo_with_id("a".repeat(64), 2, 50_000_000, false),
        ];
        let params = test_params();
        let config = UtxoSelectionConfig::new(&params);
        let result = select_utxo_for_amount(&utxos, 2_000_000, 200_000, &config).unwrap();
        assert_eq!(result.lovelace, 5_000_000);
    }

    #[test]
    fn pure_ada_wins_over_same_sized_asset_utxo() {
        // Default policy prefers pure-ADA when both sizes match.
        let utxos = vec![make_utxo_with_asset(10_000_000), make_utxo(10_000_000)];
        let params = test_params();
        let config = UtxoSelectionConfig::new(&params);
        let result = select_utxo_for_amount(&utxos, 2_000_000, 200_000, &config).unwrap();
        assert!(result.assets.is_empty(), "pure-ADA should win");
    }

    #[test]
    fn falls_back_to_asset_utxo_when_pure_ada_insufficient() {
        // Pure-ADA candidate exists but is too small; asset-bearing one fits.
        let utxos = vec![make_utxo_with_asset(10_000_000), make_utxo(100_000)];
        let params = test_params();
        let config = UtxoSelectionConfig::new(&params);
        let result = select_utxo_for_amount(&utxos, 2_000_000, 200_000, &config).unwrap();
        assert!(!result.assets.is_empty(), "should have fallen back");
        assert_eq!(result.lovelace, 10_000_000);
    }

    #[test]
    fn asset_fallback_disabled_errors_with_pure_ada_diagnostic() {
        // Strict pure-ADA mode: asset-bearing UTxO is plenty big, but caller
        // refuses to fall back. Error carries pure-ADA-specific stats.
        let utxos = vec![
            make_utxo_with_asset(50_000_000),
            make_utxo_with_id("a".repeat(64), 0, 1_000_000, false),
            make_utxo_with_id("a".repeat(64), 1, 500_000, false),
        ];
        let params = test_params();
        let config = UtxoSelectionConfig::new(&params).allow_asset_fallback(false);
        let result = select_utxo_for_amount(&utxos, 2_000_000, 200_000, &config);
        match result {
            Err(TxBuildError::NoPureAdaUtxoLargeEnough {
                needed,
                largest_pure_ada,
                total_pure_ada,
            }) => {
                assert!(needed > 2_000_000);
                assert_eq!(largest_pure_ada, 1_000_000);
                assert_eq!(total_pure_ada, 1_500_000); // 1M + 500K
            }
            other => panic!("expected NoPureAdaUtxoLargeEnough, got {other:?}"),
        }
    }

    #[test]
    fn pure_ada_only_constructor_with_no_pure_ada_errors() {
        let utxos = vec![make_utxo_with_asset(50_000_000)];
        let params = test_params();
        let config = UtxoSelectionConfig::pure_ada_only(&params);
        let result = select_utxo_for_amount(&utxos, 2_000_000, 200_000, &config);
        match result {
            Err(TxBuildError::NoPureAdaUtxoLargeEnough {
                largest_pure_ada,
                total_pure_ada,
                ..
            }) => {
                assert_eq!(largest_pure_ada, 0, "no pure-ADA at all");
                assert_eq!(total_pure_ada, 0);
            }
            other => panic!("expected NoPureAdaUtxoLargeEnough, got {other:?}"),
        }
    }

    #[test]
    fn pure_ada_only_constructor_with_pure_ada_picks_it() {
        let utxos = vec![make_utxo_with_asset(50_000_000), make_utxo(5_000_000)];
        let params = test_params();
        let config = UtxoSelectionConfig::pure_ada_only(&params);
        let result = select_utxo_for_amount(&utxos, 2_000_000, 200_000, &config).unwrap();
        assert!(result.assets.is_empty());
        assert_eq!(result.lovelace, 5_000_000);
    }

    #[test]
    fn all_asset_bearing_picks_smallest_sufficient_with_default_config() {
        // No pure-ADA at all; default config falls back to asset-bearing,
        // smallest sufficient wins.
        let utxos = vec![
            make_utxo_with_id("b".repeat(64), 0, 100_000_000, true),
            make_utxo_with_id("b".repeat(64), 1, 10_000_000, true),
            make_utxo_with_id("b".repeat(64), 2, 50_000_000, true),
        ];
        let params = test_params();
        let config = UtxoSelectionConfig::new(&params);
        let result = select_utxo_for_amount(&utxos, 2_000_000, 200_000, &config).unwrap();
        assert_eq!(result.lovelace, 10_000_000);
    }

    #[test]
    fn prefer_pure_ada_off_picks_smallest_sufficient_regardless_of_assets() {
        // Asset-bearing UTxO is smaller; with prefer_pure_ada=false, it wins
        // over a larger pure-ADA UTxO of the same wallet.
        let utxos = vec![make_utxo_with_asset(5_000_000), make_utxo(10_000_000)];
        let params = test_params();
        let config = UtxoSelectionConfig::new(&params).prefer_pure_ada(false);
        let result = select_utxo_for_amount(&utxos, 2_000_000, 200_000, &config).unwrap();
        assert_eq!(result.lovelace, 5_000_000);
        assert!(!result.assets.is_empty());
    }

    #[test]
    fn fragmented_wallet_error_distinguishes_largest_from_total() {
        // Many small UTxOs that combined would cover the amount, but no single
        // one does. The error must surface largest != total so the caller can
        // suggest "consolidate" instead of "top up."
        let utxos = vec![
            make_utxo_with_id("a".repeat(64), 0, 1_000_000, false),
            make_utxo_with_id("a".repeat(64), 1, 1_200_000, false),
            make_utxo_with_id("a".repeat(64), 2, 1_100_000, false),
            make_utxo_with_id("a".repeat(64), 3, 1_500_000, false),
        ];
        let params = test_params();
        let config = UtxoSelectionConfig::new(&params);
        // Total = 4.8 ADA, largest = 1.5 ADA, but we ask for 5 ADA — no
        // single UTxO suffices, but combined the wallet has near-enough.
        let result = select_utxo_for_amount(&utxos, 4_000_000, 200_000, &config);
        match result {
            Err(TxBuildError::NoSingleUtxoLargeEnough { largest, total, .. }) => {
                assert_eq!(largest, 1_500_000);
                assert_eq!(total, 4_800_000);
                assert_ne!(largest, total, "fragmentation must be visible");
            }
            other => panic!("expected NoSingleUtxoLargeEnough, got {other:?}"),
        }
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
