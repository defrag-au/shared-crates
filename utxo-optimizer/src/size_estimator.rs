use cardano_assets::utxo::AssetQuantity;

use crate::FeeParams;

// ============================================================================
// Transaction size constants (matching unfrackit / Cardano CSL)
// ============================================================================

/// Transaction body overhead in bytes (CBOR map, hash, etc.).
const MIN_TXN: u64 = 150;
/// Witness set overhead in bytes (vkey witness ~102 bytes + CBOR wrapping).
const MIN_SIG: u64 = 120;
/// Fee field size in bytes.
const FEE_SIZE: u64 = 6;
/// Metadata overhead in bytes.
const META_SIZE: u64 = 16;
/// Size per transaction input in bytes.
const SIZE_PER_INPUT: u64 = 37;
/// Base size per transaction output in bytes (before token data).
const SIZE_PER_OUTPUT: u64 = 64;
/// Safety margin subtracted from max TX size for the bail threshold.
const BAIL_MARGIN: u64 = 2048;

// ============================================================================
// Min-UTxO estimation
// ============================================================================

/// Fixed overhead for the Babbage/Conway min-UTxO calculation (bytes).
const FIXED_OVERHEAD: u64 = 160;
/// Bytes per policy ID in a multi-asset output.
const BYTES_PER_POLICY: u64 = 28;
/// Bytes per asset entry in a multi-asset output.
const BYTES_PER_ASSET: u64 = 12;

/// Estimate the minimum lovelace required for an output.
///
/// Uses the Babbage/Conway formula:
/// `coins_per_utxo_byte * (160 + 28 * num_policies + 12 * num_assets)`
pub fn estimate_min_lovelace(
    coins_per_utxo_byte: u64,
    num_assets: usize,
    num_policies: usize,
) -> u64 {
    if num_assets == 0 {
        return coins_per_utxo_byte * FIXED_OVERHEAD;
    }
    let policy_bytes = num_policies as u64 * BYTES_PER_POLICY;
    let asset_bytes = num_assets as u64 * BYTES_PER_ASSET;
    coins_per_utxo_byte * (FIXED_OVERHEAD + policy_bytes + asset_bytes)
}

/// Estimate minimum lovelace for a set of assets.
pub fn estimate_min_lovelace_for_assets(coins_per_utxo_byte: u64, assets: &[AssetQuantity]) -> u64 {
    if assets.is_empty() {
        return coins_per_utxo_byte * FIXED_OVERHEAD;
    }
    let mut policies = Vec::new();
    for aq in assets {
        if !policies.contains(&aq.asset_id.policy_id.as_str()) {
            policies.push(aq.asset_id.policy_id.as_str());
        }
    }
    estimate_min_lovelace(coins_per_utxo_byte, assets.len(), policies.len())
}

// ============================================================================
// Transaction size estimation
// ============================================================================

/// Token data used for TX size estimation.
pub struct OutputTokenData<'a> {
    pub assets: &'a [AssetQuantity],
}

/// Input token data for TX size estimation (tokens carried by an input UTxO).
pub struct InputTokenData<'a> {
    pub assets: &'a [AssetQuantity],
}

/// Estimate the size of a transaction given its inputs and outputs.
///
/// Returns estimated size in bytes.
pub fn estimate_tx_size(
    num_inputs: usize,
    outputs: &[OutputTokenData<'_>],
    input_tokens: &[InputTokenData<'_>],
) -> u64 {
    let mut size = MIN_TXN + MIN_SIG + FEE_SIZE + META_SIZE;

    // Inputs
    size += num_inputs as u64 * SIZE_PER_INPUT;

    // Outputs
    for output in outputs {
        size += estimate_output_size(output.assets);
    }

    // Input token data (tokens carried by inputs contribute to witness/redeemer size)
    for input in input_tokens {
        size += estimate_token_data_size(input.assets);
    }

    size
}

/// Estimate the size contribution of a single output.
fn estimate_output_size(assets: &[AssetQuantity]) -> u64 {
    let mut size = SIZE_PER_OUTPUT;

    let mut seen_policies: Vec<&str> = Vec::new();
    for aq in assets {
        let pid = aq.asset_id.policy_id.as_str();
        if !seen_policies.contains(&pid) {
            // Policy ID is 56 hex chars = 28 bytes
            size += 28;
            seen_policies.push(pid);
        }
        // Asset name: hex length / 2
        size += aq.asset_id.asset_name_hex.len() as u64 / 2;
        // Quantity: number of digits
        size += digit_count(aq.quantity);
    }

    size
}

/// Estimate the size contribution of token data in inputs.
fn estimate_token_data_size(assets: &[AssetQuantity]) -> u64 {
    let mut size = 0u64;

    let mut seen_policies: Vec<&str> = Vec::new();
    for aq in assets {
        let pid = aq.asset_id.policy_id.as_str();
        if !seen_policies.contains(&pid) {
            size += 28;
            seen_policies.push(pid);
        }
        size += aq.asset_id.asset_name_hex.len() as u64 / 2;
        size += digit_count(aq.quantity);
    }

    size
}

/// Number of decimal digits in a u64 value.
pub(crate) fn digit_count(n: u64) -> u64 {
    if n == 0 {
        return 1;
    }
    (n as f64).log10().floor() as u64 + 1
}

// ============================================================================
// Fee estimation
// ============================================================================

/// Estimate the transaction fee given its estimated size.
///
/// Formula: `(estimated_size + 636) * min_fee_coefficient + min_fee_constant`
pub fn estimate_fee(estimated_size: u64, params: &FeeParams) -> u64 {
    (estimated_size + 636) * params.min_fee_coefficient + params.min_fee_constant
}

/// The bail-out threshold: max TX size minus safety margin.
/// Stop adding inputs/outputs to a step when approaching this size.
pub fn bail_size(params: &FeeParams) -> u64 {
    params.max_tx_size as u64 - BAIL_MARGIN
}

#[cfg(test)]
mod tests {
    use super::*;
    use cardano_assets::AssetId;

    fn make_asset(policy: &str, name_hex: &str, quantity: u64) -> AssetQuantity {
        AssetQuantity {
            asset_id: AssetId::new_unchecked(policy.to_string(), name_hex.to_string()),
            quantity,
        }
    }

    #[test]
    fn test_min_lovelace_pure_ada() {
        // Pure ADA output: 4310 * 160 = 689,600
        assert_eq!(estimate_min_lovelace(4310, 0, 0), 689_600);
    }

    #[test]
    fn test_min_lovelace_single_nft() {
        // 1 policy, 1 asset: 4310 * (160 + 28 + 12) = 4310 * 200 = 862,000
        assert_eq!(estimate_min_lovelace(4310, 1, 1), 862_000);
    }

    #[test]
    fn test_min_lovelace_multi_policy() {
        // 3 policies, 10 assets: 4310 * (160 + 84 + 120) = 4310 * 364 = 1,568,840
        assert_eq!(estimate_min_lovelace(4310, 10, 3), 1_568_840);
    }

    #[test]
    fn test_min_lovelace_for_assets() {
        let assets = vec![
            make_asset(
                "aaaa00000000000000000000000000000000000000000000000000aa",
                "4e465431",
                1,
            ),
            make_asset(
                "aaaa00000000000000000000000000000000000000000000000000aa",
                "4e465432",
                1,
            ),
            make_asset(
                "bbbb00000000000000000000000000000000000000000000000000bb",
                "544f4b454e",
                100,
            ),
        ];
        // 2 policies, 3 assets: 4310 * (160 + 56 + 36) = 4310 * 252 = 1,086,120
        assert_eq!(estimate_min_lovelace_for_assets(4310, &assets), 1_086_120);
    }

    #[test]
    fn test_estimate_fee() {
        let params = FeeParams::default();
        // Size 1000: (1000 + 636) * 44 + 155381 = 1636 * 44 + 155381 = 71984 + 155381 = 227365
        assert_eq!(estimate_fee(1000, &params), 227_365);
    }

    #[test]
    fn test_bail_size() {
        let params = FeeParams::default();
        assert_eq!(bail_size(&params), 14_336);
    }

    #[test]
    fn test_digit_count() {
        assert_eq!(digit_count(0), 1);
        assert_eq!(digit_count(1), 1);
        assert_eq!(digit_count(9), 1);
        assert_eq!(digit_count(10), 2);
        assert_eq!(digit_count(999), 3);
        assert_eq!(digit_count(1_000_000), 7);
    }

    #[test]
    fn test_tx_size_empty() {
        // Minimal tx: just base overhead
        let size = estimate_tx_size(0, &[], &[]);
        assert_eq!(size, MIN_TXN + MIN_SIG + FEE_SIZE + META_SIZE);
    }

    #[test]
    fn test_tx_size_pure_ada() {
        // 2 inputs, 1 pure-ADA output
        let outputs = vec![OutputTokenData { assets: &[] }];
        let size = estimate_tx_size(2, &outputs, &[]);
        let expected =
            MIN_TXN + MIN_SIG + FEE_SIZE + META_SIZE + 2 * SIZE_PER_INPUT + SIZE_PER_OUTPUT;
        assert_eq!(size, expected);
    }

    #[test]
    fn test_tx_size_with_tokens() {
        let assets = vec![
            make_asset(
                "aaaa00000000000000000000000000000000000000000000000000aa",
                "4e465431",
                1,
            ),
            make_asset(
                "aaaa00000000000000000000000000000000000000000000000000aa",
                "4e465432",
                1,
            ),
        ];
        let outputs = vec![OutputTokenData { assets: &assets }];
        let size = estimate_tx_size(1, &outputs, &[]);

        // Verify it's reasonable (> base + input + output)
        let base = MIN_TXN + MIN_SIG + FEE_SIZE + META_SIZE + SIZE_PER_INPUT + SIZE_PER_OUTPUT;
        assert!(size > base);
        // And less than something absurd
        assert!(size < 2000);
    }
}
