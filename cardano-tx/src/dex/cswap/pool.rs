//! CSWAP pool datum decoding and constant product AMM math.
//!
//! Pool datums are inline on pool UTxOs. Format (from WingRiders datum registry):
//! Constructor 0, 8 fields:
//!   [0] totalLpTokens: BigInt
//!   [1] poolFee: BigInt (basis points, e.g. 85)
//!   [2] quoteAssetPolicy: Bytes
//!   [3] quoteAssetName: Bytes
//!   [4] baseAssetPolicy: Bytes
//!   [5] baseAssetName: Bytes
//!   [6] lpTokenPolicy: Bytes
//!   [7] lpTokenName: Bytes

use pallas_primitives::alonzo::{BigInt, PlutusData};
use pallas_primitives::Fragment;

use super::CswapError;

/// Decoded fields from a CSWAP pool datum.
#[derive(Debug, Clone)]
pub struct CswapPoolDatum {
    pub total_lp_tokens: u64,
    pub pool_fee_bps: u64,
}

/// Decode a CSWAP pool datum from CBOR hex bytes.
///
/// The CBOR hex comes from Maestro's inline datum field on the pool UTxO.
pub fn decode_pool_datum_cbor(cbor_hex: &str) -> Result<CswapPoolDatum, CswapError> {
    let bytes = hex::decode(cbor_hex)
        .map_err(|e| CswapError::InvalidInput(format!("invalid datum hex: {e}")))?;

    let plutus_data = PlutusData::decode_fragment(&bytes)
        .map_err(|e| CswapError::CborEncoding(format!("failed to decode pool datum CBOR: {e}")))?;

    match &plutus_data {
        PlutusData::Constr(constr) if constr.tag == 121 && constr.fields.len() == 8 => {
            let total_lp_tokens = extract_bigint(&constr.fields[0])?;
            let pool_fee_bps = extract_bigint(&constr.fields[1])?;
            Ok(CswapPoolDatum {
                total_lp_tokens,
                pool_fee_bps,
            })
        }
        PlutusData::Constr(constr) => Err(CswapError::InvalidInput(format!(
            "expected Constr 0 with 8 fields, got tag={} fields={}",
            constr.tag,
            constr.fields.len()
        ))),
        _ => Err(CswapError::InvalidInput(
            "expected Constr, got other PlutusData variant".to_string(),
        )),
    }
}

/// Constant product AMM: calculate output amount for a given input.
///
/// Formula: `dy = y * dx_after_fee / (x + dx_after_fee)`
/// where `dx_after_fee = dx * (10000 - pool_fee_bps) / 10000`
///
/// Uses u128 intermediates to avoid overflow on large reserves.
pub fn constant_product_swap(
    input_amount: u64,
    input_reserves: u64,
    output_reserves: u64,
    pool_fee_bps: u64,
) -> u64 {
    let dx = input_amount as u128;
    let x = input_reserves as u128;
    let y = output_reserves as u128;
    let fee_factor = (10_000 - pool_fee_bps) as u128;

    if x == 0 || y == 0 {
        return 0;
    }

    let dx_after_fee = dx * fee_factor / 10_000;
    let numerator = y * dx_after_fee;
    let denominator = x + dx_after_fee;

    (numerator / denominator) as u64
}

/// Extract a u64 from a PlutusData BigInt.
fn extract_bigint(data: &PlutusData) -> Result<u64, CswapError> {
    match data {
        PlutusData::BigInt(BigInt::Int(int)) => {
            let val: i128 = (*int).into();
            if val >= 0 && val <= u64::MAX as i128 {
                Ok(val as u64)
            } else {
                Err(CswapError::InvalidInput(format!(
                    "bigint out of u64 range: {val}"
                )))
            }
        }
        _ => Err(CswapError::InvalidInput(format!(
            "expected BigInt, got {data:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_product_swap_basic() {
        // 10 ADA into a pool with 1000 ADA / 10B tokens, 85 bps fee
        let tokens = constant_product_swap(10_000_000, 1_000_000_000, 10_000_000_000, 85);
        // dx' = 10M * 9915/10000 = 9_915_000
        // dy = 10B * 9_915_000 / (1B + 9_915_000) = ~98,176,579
        assert!(tokens > 98_000_000 && tokens < 99_000_000, "got {tokens}");
    }

    #[test]
    fn test_constant_product_swap_aliens() {
        // Real Aliens pool: ~89,897 ADA reserves, ~822,991,085 token reserves
        // 10 ADA buy with 85 bps fee
        let ada_reserves = 89_897_000_000u64; // ~89,897 ADA in lovelace
        let token_reserves = 822_991_085u64;
        let tokens = constant_product_swap(10_000_000, ada_reserves, token_reserves, 85);
        // CSWAP UI shows ~91,282 for 10 ADA
        // Our formula should give ~91,300-91,400 (within 0.2%)
        assert!(
            tokens > 90_000 && tokens < 93_000,
            "expected ~91,300, got {tokens}"
        );
    }

    #[test]
    fn test_constant_product_swap_zero_input() {
        assert_eq!(constant_product_swap(0, 1_000_000, 1_000_000, 85), 0);
    }

    #[test]
    fn test_constant_product_swap_zero_reserves() {
        assert_eq!(constant_product_swap(1_000_000, 0, 1_000_000, 85), 0);
    }

    #[test]
    fn test_decode_pool_datum() {
        // Real Aliens pool datum from Maestro (live on-chain)
        // Constructor 0, 8 fields:
        //   [0] totalLpTokens=638734409, [1] poolFee=85,
        //   [2] quotePolicy="", [3] quoteName="",
        //   [4] basePolicy="16657df3...", [5] baseName="416c69656e73",
        //   [6] lpPolicy="090da86f...", [7] lpName="432d4c503a..."
        let cbor_hex = "d8799f1a2612504918554040581c16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb746416c69656e73581c090da86f84b83c4612a73012fb186ea26d8b607e610a3bb29cd8cc3a52432d4c503a20414441207820416c69656e73ff";

        let datum = decode_pool_datum_cbor(cbor_hex).unwrap();
        assert_eq!(datum.total_lp_tokens, 638_734_409);
        assert_eq!(datum.pool_fee_bps, 85);
    }
}
