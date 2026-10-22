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
//!
//! ## What a fill pays
//!
//! [`buy_output`] and [`sell_output`] are the batcher's own arithmetic, not an
//! estimate of it. Checked against every fill in the twelve most recent
//! batches on the $ALIENS pool (2026-10-22): six buys and a sell, each to the
//! unit. Three things make it exact, and each was wrong before:
//!
//! - the pool's ADA reserve is its lovelace less
//!   [`POOL_RESERVED_LOVELACE`](super::config::POOL_RESERVED_LOVELACE);
//! - the batcher's [`DEFAULT_FEE_TIER`](super::config::DEFAULT_FEE_TIER) is
//!   taken off the ADA side before the curve (a buy) or after it (a sell);
//! - the pool fee scales the input inside the product rather than being
//!   floored onto it first.

use pallas_primitives::Fragment;
use pallas_primitives::alonzo::{BigInt, PlutusData};

use super::CswapError;
use super::config::{DEFAULT_FEE_TIER, POOL_RESERVED_LOVELACE};

/// Basis points in one.
const BPS: u128 = 10_000;

/// Decoded fields from a CSWAP pool datum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CswapPoolDatum {
    pub total_lp_tokens: u64,
    pub pool_fee_bps: u64,
    /// The pair. Empty policy and name are ADA.
    pub quote_policy: Vec<u8>,
    pub quote_name: Vec<u8>,
    pub base_policy: Vec<u8>,
    pub base_name: Vec<u8>,
    /// The pool's LP token policy — also the policy of its pool NFT, `c`.
    pub lp_policy: Vec<u8>,
    pub lp_name: Vec<u8>,
}

impl CswapPoolDatum {
    /// The non-ADA side of an ADA pool, `(policy, name)` bytes. `None` for a
    /// token/token pool, which an ADA buy cannot use.
    pub fn token_side(&self) -> Option<(&[u8], &[u8])> {
        let quote_is_ada = self.quote_policy.is_empty() && self.quote_name.is_empty();
        let base_is_ada = self.base_policy.is_empty() && self.base_name.is_empty();
        match (quote_is_ada, base_is_ada) {
            (true, false) => Some((&self.base_policy, &self.base_name)),
            (false, true) => Some((&self.quote_policy, &self.quote_name)),
            _ => None,
        }
    }
}

/// Decode a CSWAP pool datum from CBOR hex bytes.
///
/// The CBOR hex comes from Maestro's inline datum field on the pool UTxO.
pub fn decode_pool_datum_cbor(cbor_hex: &str) -> Result<CswapPoolDatum, CswapError> {
    let bytes = hex::decode(cbor_hex)
        .map_err(|e| CswapError::InvalidInput(format!("invalid datum hex: {e}")))?;
    decode_pool_datum(&bytes)
}

/// Decode a CSWAP pool datum from its CBOR bytes.
pub fn decode_pool_datum(bytes: &[u8]) -> Result<CswapPoolDatum, CswapError> {
    let plutus_data = PlutusData::decode_fragment(bytes)
        .map_err(|e| CswapError::CborEncoding(format!("failed to decode pool datum CBOR: {e}")))?;

    match &plutus_data {
        PlutusData::Constr(constr) if constr.tag == 121 && constr.fields.len() == 8 => {
            let f = &constr.fields;
            Ok(CswapPoolDatum {
                total_lp_tokens: extract_bigint(&f[0])?,
                pool_fee_bps: extract_bigint(&f[1])?,
                quote_policy: extract_bytes(&f[2])?,
                quote_name: extract_bytes(&f[3])?,
                base_policy: extract_bytes(&f[4])?,
                base_name: extract_bytes(&f[5])?,
                lp_policy: extract_bytes(&f[6])?,
                lp_name: extract_bytes(&f[7])?,
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

/// Tokens a buy of `ada_in` lovelace receives, exactly as the batcher fills it
/// against a pool holding `pool_lovelace` and `token_reserve`.
///
/// `ada_in` is the order's swap amount — its deposit less the 2 ADA that
/// returns and the batcher's provision. The batcher takes
/// [`DEFAULT_FEE_TIER`] basis points of it first; the pool's curve runs on the
/// rest.
pub fn buy_output(ada_in: u64, pool_lovelace: u64, token_reserve: u64, pool_fee_bps: u64) -> u64 {
    let into_pool = ada_in as u128 - platform_fee(ada_in as u128);
    curve(
        into_pool,
        pool_lovelace.saturating_sub(POOL_RESERVED_LOVELACE) as u128,
        token_reserve as u128,
        pool_fee_bps,
    )
}

/// Lovelace a sell of `token_in` hands the seller (on top of the 2 ADA their
/// order returns), exactly as the batcher fills it. The curve pays out, then
/// the batcher takes [`DEFAULT_FEE_TIER`] basis points of that.
pub fn sell_output(
    token_in: u64,
    pool_lovelace: u64,
    token_reserve: u64,
    pool_fee_bps: u64,
) -> u64 {
    let out = curve(
        token_in as u128,
        token_reserve as u128,
        pool_lovelace.saturating_sub(POOL_RESERVED_LOVELACE) as u128,
        pool_fee_bps,
    ) as u128;
    (out - platform_fee(out)) as u64
}

/// How much ADA an order can swap before the pool's CURVE alone moves the
/// price by `impact_bps`, with the pool's fee and the batcher's cut on top:
/// the pool's buy depth at that impact.
///
/// A net input `n` into ADA reserve `r` receives `n / (r + n)` less than the
/// spot price would give, so moving it by `p` takes `n = r·p / (1 − p)`; the
/// pool fee and the batcher's [`DEFAULT_FEE_TIER`] are grossed back on. Fees
/// are excluded from the impact on purpose — they are flat costs with their
/// own lines, and counting them would make every pool "move" on the first
/// lovelace.
pub fn buy_depth(pool_lovelace: u64, pool_fee_bps: u64, impact_bps: u32) -> u64 {
    let p = u128::from(impact_bps.min(9_999));
    let reserve = pool_lovelace.saturating_sub(POOL_RESERVED_LOVELACE) as u128;
    let keep_pool = BPS.saturating_sub(pool_fee_bps as u128);
    let keep_platform = BPS - DEFAULT_FEE_TIER as u128;
    if keep_pool == 0 {
        return 0;
    }
    let net = reserve * p / (BPS - p);
    let into_pool = net * BPS / keep_pool;
    u64::try_from(into_pool * BPS / keep_platform).unwrap_or(u64::MAX)
}

/// The batcher's cut of an ADA amount, ROUNDED UP: a 26,087,000 lovelace buy
/// reached the pool 39,131 short (39,130.5 exact), and a sell paying out
/// 93,491,920 handed over 140,238 less (140,237.88 exact).
fn platform_fee(lovelace: u128) -> u128 {
    (lovelace * DEFAULT_FEE_TIER as u128).div_ceil(BPS)
}

/// The constant-product payout for `dx` into reserves `x`→`y`, with the pool
/// fee scaling the input inside the product. Floors once, at the end.
fn curve(dx: u128, x: u128, y: u128, pool_fee_bps: u64) -> u64 {
    let keep = BPS.saturating_sub(pool_fee_bps as u128);
    let denominator = x * BPS + dx * keep;
    if denominator == 0 || y == 0 {
        return 0;
    }
    (y * dx * keep / denominator) as u64
}

/// Constant product AMM: calculate output amount for a given input.
///
/// Formula: `dy = y * dx_after_fee / (x + dx_after_fee)`
/// where `dx_after_fee = dx * (10000 - pool_fee_bps) / 10000`
///
/// An APPROXIMATION of a CSWAP fill, kept for the split optimiser's DEX-agnostic
/// pool model: it ignores the batcher's fee tier and the pool's reserved ADA,
/// and floors the fee early. To price a CSWAP order, use [`buy_output`].
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

/// Extract the bytes of a PlutusData byte string.
fn extract_bytes(data: &PlutusData) -> Result<Vec<u8>, CswapError> {
    match data {
        PlutusData::BoundedBytes(bytes) => Ok(bytes.to_vec()),
        _ => Err(CswapError::InvalidInput(format!(
            "expected Bytes, got {data:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live $ALIENS pool datum (2026-10-22), whose LP policy is also the
    /// pool NFT's.
    const ALIENS_DATUM: &str = "d8799f1a37d5043d18554040581c16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb746416c69656e73581c090da86f84b83c4612a73012fb186ea26d8b607e610a3bb29cd8cc3a52432d4c503a20414441207820416c69656e73ff";

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

    /// The pair and the LP policy are read, and ADA is the empty side.
    #[test]
    fn the_datum_names_the_pair_and_the_lp_policy() {
        let datum = decode_pool_datum_cbor(ALIENS_DATUM).unwrap();
        let (policy, name) = datum.token_side().expect("an ADA pool");
        assert_eq!(
            hex::encode(policy),
            "16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb7"
        );
        assert_eq!(name, b"Aliens");
        assert_eq!(
            hex::encode(&datum.lp_policy),
            "090da86f84b83c4612a73012fb186ea26d8b607e610a3bb29cd8cc3a"
        );
    }

    /// Six real buys on the $ALIENS pool (2026-10-22), each to the unit:
    /// `(pool lovelace before, tokens before, ADA swapped, tokens delivered)`.
    /// The swapped ADA is each order's deposit less 2.69 ADA.
    #[test]
    fn buys_match_real_fills_to_the_unit() {
        let fills = [
            (5_555_699_498, 164_791_018, 35_000_000, 1_021_783),
            (5_590_646_998, 163_769_235, 10_000_000, 289_599),
            (5_352_773_499, 171_118_298, 51_000_000, 1_599_602),
            (5_403_696_999, 169_518_696, 90_036_000, 2_751_923),
            (5_493_597_945, 166_766_773, 100_000_000, 2_953_193),
            (5_593_447_945, 163_813_580, 26_087_000, 753_163),
        ];
        for (lovelace, tokens, ada_in, delivered) in fills {
            assert_eq!(
                buy_output(ada_in, lovelace, tokens, 85),
                delivered,
                "{ada_in} lovelace into {lovelace}/{tokens}"
            );
        }
    }

    /// Swapping exactly the depth at `p` moves the curve by `p`: the batcher's
    /// own fill comes out `p` short of the spot price on what reaches the
    /// curve. A shallow pool's depth is small; a deep one's large.
    #[test]
    fn swapping_the_depth_moves_the_curve_by_that_much() {
        let (lovelace, tokens, fee) = (5_526_003_894u64, 165_843_819u64, 85u64);
        let reserve = (lovelace - POOL_RESERVED_LOVELACE) as u128;
        for p in [100u32, 200, 500] {
            let depth = buy_depth(lovelace, fee, p);
            let out = buy_output(depth, lovelace, tokens, fee) as u128;
            // What reached the curve, net of the batcher's cut and the fee.
            let into_pool = depth as u128 - platform_fee(depth as u128);
            let net = into_pool * (BPS - fee as u128) / BPS;
            let spot = net * tokens as u128 / reserve;
            let moved = (spot - out) * 10_000 / spot;
            assert!(moved.abs_diff(p as u128) <= 1, "{p} bps: moved {moved}");
        }
        // A pool a hundredth the size has a hundredth the depth.
        let small = buy_depth(POOL_RESERVED_LOVELACE + reserve as u64 / 100, fee, 100);
        assert!(small.abs_diff(buy_depth(lovelace, fee, 100) / 100) <= 2);
    }

    /// A real sell: 2,783,402 $ALIENS paid out 93,351,682 lovelace on top of
    /// the order's returned 2 ADA (`07c07d54…`, 2026-10-22).
    #[test]
    fn a_sell_matches_a_real_fill_to_the_unit() {
        assert_eq!(
            sell_output(2_783_402, 5_619_495_814, 163_060_417, 85),
            93_351_682
        );
    }
}
