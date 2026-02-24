//! Splash DEX configuration and fee calculation
//!
//! Operational parameters for the Splash V3 spot order contract, plus
//! price conversion helpers and deposit calculations.
//!
//! Reference: protocol-sdk spotOrder.ts, constants.ts

use serde::Deserialize;

use super::SplashError;

// ============================================================================
// Constants — Splash V3 spot order contract parameters
// ============================================================================

/// Default batcher public key hash (permitted executor)
pub const DEFAULT_BATCHER_KEY: &str = "5cb2c968e5d1c7197a6ce7615967310a375545d9bc65063a964335b2";

/// Minimum collateral ADA for orders (in lovelace)
pub const MINIMUM_COLLATERAL_ADA: u64 = 1_500_000;

/// Estimated min UTxO deposit for the batcher's receive output (lovelace).
/// When the batcher executes the order, it creates an output back to us with
/// the bought tokens. This output needs min ADA. The SDK calculates this
/// dynamically via `predictDepositAda`, but ~1.5M covers a single native token.
pub const RECEIVE_OUTPUT_DEPOSIT: u64 = 1_500_000;

/// SpotOrderV3 script hash (from spectrum.fi/settings.json → operations.spotOrderV3.settingsV2)
const SPOT_ORDER_V3_SCRIPT: &str = "464eeee89f05aff787d40045af2a40a83fd96c513197d32fbc54ff02";

/// Cost per execution step (lovelace)
/// Note: spectrum.fi/settings.json says 600K but the Splash UI uses 1M in practice,
/// and the batcher appears to skip orders with step cost below 1M.
const STEP_COST: u64 = 1_000_000;

/// Worst case step cost (lovelace)
const WORST_STEP_COST: u64 = 1_000_000;

/// Maximum execution steps for limit orders
const MAX_STEP_COUNT: u64 = 4;

/// Maximum execution steps for market orders
const MAX_STEP_COUNT_MARKET: u64 = 1;

/// Default executor fee (lovelace) — matches what the Splash UI uses
const DEFAULT_EXECUTOR_FEE: u64 = 1_000_000;

// ============================================================================
// Constants — Cancel / refund mechanism
// ============================================================================

/// Redeemer CBOR for cancelling a spot order (Constr 0, no fields)
pub const CANCEL_REDEEMER_HEX: &str = "d87980";

/// Reference UTxO containing the V3 spot order validator script (mainnet).
/// Using a reference input avoids including the full script in the TX.
pub const MAINNET_SCRIPT_REF_TX: &str =
    "b91eda29d145ab6c0bc0d6b7093cb24b131440b7b015033205476f39c690a51f";
pub const MAINNET_SCRIPT_REF_INDEX: u64 = 0;

/// Execution budget for the cancel/refund redeemer
pub const CANCEL_EX_UNITS_MEM: u64 = 2_000_000;
pub const CANCEL_EX_UNITS_STEPS: u64 = 50_000_000;

/// Estimated script execution fee (lovelace) — conservative estimate.
/// Based on: price_mem(0.0577) * 2M + price_step(0.0000721) * 50M ≈ 119K, rounded up.
pub const CANCEL_SCRIPT_FEE: u64 = 150_000;

/// PlutusV2 cost model (mainnet, epoch 528) — 175 values.
/// Required for computing `script_data_hash` in Plutus transactions.
pub const PLUTUS_V2_COST_MODEL: [i64; 175] = [
    100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32, 201305, 8356, 4, 16000, 100,
    16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 100, 100, 16000, 100, 94375, 32,
    132994, 32, 61462, 4, 72010, 178, 0, 1, 22151, 32, 91189, 769, 4, 2, 85848, 228465, 122, 0, 1,
    1, 1000, 42921, 4, 2, 24548, 29498, 38, 1, 898148, 27279, 1, 51775, 558, 1, 39184, 1000, 60594,
    1, 141895, 32, 83150, 32, 15299, 32, 76049, 1, 13169, 4, 22100, 10, 28999, 74, 1, 28999, 74, 1,
    43285, 552, 1, 44749, 541, 1, 33852, 32, 68246, 32, 72362, 32, 7243, 32, 7391, 32, 11546, 32,
    85848, 228465, 122, 0, 1, 1, 90434, 519, 0, 1, 74433, 32, 85848, 228465, 122, 0, 1, 1, 85848,
    228465, 122, 0, 1, 1, 955506, 213312, 0, 2, 270652, 22588, 4, 1457325, 64566, 4, 20467, 1, 4,
    0, 141992, 32, 100788, 420, 1, 1, 81663, 32, 59498, 32, 20142, 32, 24588, 32, 20744, 32, 25933,
    32, 24623, 32, 43053543, 10, 53384111, 14333, 10, 43574283, 26308, 10,
];

/// Mainnet executor fee API
pub const MAINNET_FEE_URL: &str =
    "https://analytics.splash.trade/platform-api/v1/fees-api/distribution/by/pair";

/// Mainnet order book API
pub const ORDER_BOOK_URL: &str =
    "https://analytics.splash.trade/platform-api/v1/trading-view/order-book";

/// Resolved configuration for building a spot order
#[derive(Debug, Clone)]
pub struct SpotOrderConfig {
    /// Script hash of the spot order validator (28 bytes hex)
    pub script_hash: String,
    /// Cost per execution step (lovelace)
    pub step_cost: u64,
    /// Worst case step cost (lovelace)
    pub worst_step_cost: u64,
    /// Default executor fee (lovelace)
    pub default_executor_fee: u64,
    /// Maximum execution steps for limit orders
    pub max_steps: u64,
    /// Maximum execution steps for market orders
    pub max_steps_market: u64,
}

// ============================================================================
// Fee API types (used by fetch module)
// ============================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct FeeDistribution {
    pub(crate) steps: Vec<FeeStep>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FeeStep {
    #[serde(rename = "lowerBound")]
    pub(crate) lower_bound: u64,
    #[serde(rename = "upperBound")]
    pub(crate) upper_bound: u64,
    pub(crate) fee: u64,
}

// ============================================================================
// Public API
// ============================================================================

impl SpotOrderConfig {
    /// Default V3 spot order config.
    ///
    /// Values sourced from spectrum.fi/settings.json → operations.spotOrderV3.settingsV2.
    /// Hardcoded since the spectrum.fi site is sunsetting.
    pub fn default_v3() -> Self {
        Self {
            script_hash: SPOT_ORDER_V3_SCRIPT.to_string(),
            step_cost: STEP_COST,
            worst_step_cost: WORST_STEP_COST,
            default_executor_fee: DEFAULT_EXECUTOR_FEE,
            max_steps: MAX_STEP_COUNT,
            max_steps_market: MAX_STEP_COUNT_MARKET,
        }
    }
}

// ============================================================================
// Order book / pricing types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct RawOrderBook {
    pub spot: String,
    #[serde(default)]
    pub bids: Vec<RawOrderBookItem>,
    #[serde(default)]
    pub asks: Vec<RawOrderBookItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawOrderBookItem {
    pub price: String,
    #[serde(rename = "avgPrice")]
    pub avg_price: String,
    #[serde(rename = "accumulatedLiquidity")]
    pub accumulated_liquidity: String,
}

/// Spot price quote resolved to a rational number
#[derive(Debug, Clone)]
pub struct OrderBookQuote {
    /// Raw spot price string from the API
    pub spot_price: String,
    /// Rational numerator
    pub numerator: u64,
    /// Rational denominator
    pub denominator: u64,
    /// Raw order book for inspection
    pub bids: Vec<RawOrderBookItem>,
    /// Raw order book for inspection
    pub asks: Vec<RawOrderBookItem>,
}

// ============================================================================
// Asset identifier conversion
// ============================================================================

/// Convert our internal asset identifier to the Splash API format.
///
/// Splash uses `"."` for ADA and `"policy_id.asset_name_hex"` for native tokens.
/// Our system uses `"lovelace"` for ADA.
pub fn to_splash_asset(asset: &str) -> &str {
    if asset == "lovelace" {
        "."
    } else {
        asset
    }
}

// ============================================================================
// Market order price estimation from order book depth
// ============================================================================

/// Select the estimated fill price for a market order by walking the order book.
///
/// For **buying tokens** (input=ADA): walks the asks to find the depth tier whose
/// `accumulatedLiquidity` (in tokens) covers the estimated order size, then uses
/// that tier's `avg_price` (volume-weighted average across tiers up to that depth).
///
/// For **selling tokens** (input=token): walks the bids similarly.
///
/// Returns `(numerator, denominator)` in **lovelace per token** (same orientation
/// as the spot price). The caller is responsible for inverting if needed.
///
/// Falls back to the spot price if the order book is empty or the order exceeds
/// all available liquidity.
pub fn select_estimated_price(
    quote: &OrderBookQuote,
    input_is_ada: bool,
    input_amount: u64,
) -> Result<(u64, u64), SplashError> {
    let tiers = if input_is_ada {
        &quote.asks
    } else {
        &quote.bids
    };

    if tiers.is_empty() {
        tracing::warn!(
            "Order book has no {} tiers, falling back to spot price",
            if input_is_ada { "ask" } else { "bid" }
        );
        return Ok((quote.numerator, quote.denominator));
    }

    // Estimate how many tokens our order needs.
    // For buying: tokens = input_lovelace / price (lovelace per token)
    // For selling: tokens = input_amount directly (input is already tokens)
    let estimated_tokens = if input_is_ada {
        // Use the first ask price for a rough estimate
        let first_price = parse_order_book_price(&tiers[0].price)?;
        let (price_num, price_den) = first_price;
        // tokens = input_lovelace * price_den / price_num
        // (price is lovelace/token, so dividing lovelace by price gives tokens)
        if price_num == 0 {
            return Ok((quote.numerator, quote.denominator));
        }
        (input_amount as u128 * price_den as u128 / price_num as u128) as u64
    } else {
        input_amount
    };

    // Walk the tiers to find the one that covers our order
    for tier in tiers {
        let accumulated: u64 = tier.accumulated_liquidity.parse().unwrap_or(0);

        if accumulated >= estimated_tokens {
            // This tier covers our order — use its avg_price
            let (num, den) = decimal_to_rational(&tier.avg_price)?;
            tracing::info!(
                "Order book depth: {estimated_tokens} tokens fits within tier at accumulated {accumulated}, avg_price {num}/{den} ({:.4} lovelace/token)",
                num as f64 / den as f64
            );
            return Ok((num, den));
        }
    }

    // Order exceeds all tiers — use the deepest tier's avg_price
    if let Some(last) = tiers.last() {
        let (num, den) = decimal_to_rational(&last.avg_price)?;
        tracing::warn!(
            "Order exceeds order book depth, using deepest tier avg_price {num}/{den} ({:.4} lovelace/token)",
            num as f64 / den as f64
        );
        return Ok((num, den));
    }

    Ok((quote.numerator, quote.denominator))
}

/// Parse a price string from the order book into a rational (num, den).
fn parse_order_book_price(price_str: &str) -> Result<(u64, u64), SplashError> {
    decimal_to_rational(price_str)
}

// ============================================================================
// Price conversion helpers
// ============================================================================

/// Convert a decimal string (e.g. "0.001234") to a rational numerator/denominator.
///
/// Handles integer strings ("1234"), decimal strings ("0.001234"), and
/// scientific notation is not supported.
///
/// Truncates to at most 12 decimal places to avoid u64 overflow with
/// high-precision API responses.
pub fn decimal_to_rational(s: &str) -> Result<(u64, u64), SplashError> {
    const MAX_DECIMALS: usize = 12;

    if s.is_empty() {
        return Err(SplashError::InvalidInput("empty price string".to_string()));
    }

    let parts: Vec<&str> = s.split('.').collect();
    match parts.len() {
        1 => {
            // Integer: "1234" → 1234/1
            let n: u64 = parts[0]
                .parse()
                .map_err(|e| SplashError::InvalidInput(format!("bad price integer: {e}")))?;
            Ok((n, 1))
        }
        2 => {
            // Decimal: "0.001234" → combine integer + fractional parts
            let integer_str = parts[0];
            // Truncate to MAX_DECIMALS to stay within u64 range
            let frac_str = if parts[1].len() > MAX_DECIMALS {
                &parts[1][..MAX_DECIMALS]
            } else {
                parts[1]
            };
            let decimal_places = frac_str.len() as u32;
            let denominator = 10u64.checked_pow(decimal_places).ok_or_else(|| {
                SplashError::InvalidInput(format!("too many decimal places: {decimal_places}"))
            })?;

            let integer_part: u64 = if integer_str.is_empty() {
                0
            } else {
                integer_str
                    .parse()
                    .map_err(|e| SplashError::InvalidInput(format!("bad price integer: {e}")))?
            };

            let frac_part: u64 = if frac_str.is_empty() {
                0
            } else {
                frac_str
                    .parse()
                    .map_err(|e| SplashError::InvalidInput(format!("bad price fraction: {e}")))?
            };

            let numerator = integer_part
                .checked_mul(denominator)
                .and_then(|v| v.checked_add(frac_part))
                .ok_or_else(|| SplashError::InvalidInput("price overflow".to_string()))?;

            // Simplify with GCD
            let g = gcd(numerator, denominator);
            Ok((numerator / g, denominator / g))
        }
        _ => Err(SplashError::InvalidInput(format!(
            "invalid price format: '{s}'"
        ))),
    }
}

/// Apply slippage to a price ratio. Reduces the expected output.
///
/// For a buy order (selling input for output), slippage means accepting fewer
/// output tokens. So we reduce the numerator (output per input).
///
/// `slippage_bps`: basis points, e.g. 300 = 3%, 500 = 5%
pub fn apply_slippage(numerator: u64, denominator: u64, slippage_bps: u32) -> (u64, u64) {
    // price_with_slippage = price * (10000 - slippage_bps) / 10000
    // To avoid losing precision, multiply numerator by (10000 - bps) and denominator by 10000
    let factor = 10_000u64.saturating_sub(slippage_bps as u64);
    let new_num = numerator.saturating_mul(factor);
    let new_den = denominator.saturating_mul(10_000);

    let g = gcd(new_num, new_den);
    (new_num / g, new_den / g)
}

/// Greatest common divisor (Euclidean algorithm)
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.max(1) // avoid division by zero
}

// ============================================================================
// Deposit calculation
// ============================================================================

/// Calculate the total ADA deposit required for a spot order.
///
/// This includes:
/// - Worst case step cost (covers execution even if price moves)
/// - Step cost * (max_steps - 1) for additional partial fills
/// - Executor fee
/// - Receive output deposit (min ADA for the batcher's token output back to us)
/// - Minimum collateral (only if the other deposits don't already cover it)
///
/// The `input_amount` is only included if the input asset is ADA.
/// Market orders use `max_steps_market` (1), limit orders use `max_steps` (4).
pub fn calculate_order_deposit(
    config: &SpotOrderConfig,
    executor_fee: u64,
    input_lovelace: u64,
    is_market: bool,
) -> u64 {
    let max_steps = if is_market {
        config.max_steps_market
    } else {
        config.max_steps
    };

    let step_deposit =
        config.worst_step_cost + config.step_cost.saturating_mul(max_steps.saturating_sub(1));

    let base_deposit = input_lovelace + step_deposit + executor_fee + RECEIVE_OUTPUT_DEPOSIT;

    // Only add collateral padding if the base deposit doesn't already cover it
    if base_deposit >= MINIMUM_COLLATERAL_ADA {
        base_deposit
    } else {
        base_deposit + (MINIMUM_COLLATERAL_ADA - base_deposit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decimal_to_rational_integer() {
        let (n, d) = decimal_to_rational("1234").unwrap();
        assert_eq!(n, 1234);
        assert_eq!(d, 1);
    }

    #[test]
    fn test_decimal_to_rational_simple() {
        let (n, d) = decimal_to_rational("0.5").unwrap();
        assert_eq!(n, 1);
        assert_eq!(d, 2);
    }

    #[test]
    fn test_decimal_to_rational_small() {
        let (n, d) = decimal_to_rational("0.001234").unwrap();
        // 1234 / 1000000, GCD(1234, 1000000) = 2 → 617/500000
        assert_eq!(n, 617);
        assert_eq!(d, 500000);
    }

    #[test]
    fn test_decimal_to_rational_with_integer_part() {
        let (n, d) = decimal_to_rational("3.14").unwrap();
        // 314 / 100, GCD(314, 100) = 2 → 157/50
        assert_eq!(n, 157);
        assert_eq!(d, 50);
    }

    #[test]
    fn test_decimal_to_rational_one() {
        let (n, d) = decimal_to_rational("1.0").unwrap();
        assert_eq!(n, 1);
        assert_eq!(d, 1);
    }

    #[test]
    fn test_decimal_to_rational_empty_fails() {
        assert!(decimal_to_rational("").is_err());
    }

    #[test]
    fn test_apply_slippage_5_percent() {
        // Price 1000/1, 5% slippage (500 bps)
        let (n, d) = apply_slippage(1000, 1, 500);
        // 1000 * 9500 / (1 * 10000) = 9500000/10000, GCD = 10000 → 950/1
        assert_eq!(n, 950);
        assert_eq!(d, 1);
    }

    #[test]
    fn test_apply_slippage_3_percent() {
        // Price 100/1, 3% slippage (300 bps)
        let (n, d) = apply_slippage(100, 1, 300);
        // 100 * 9700 / 10000 = 970000/10000 → 97/1
        assert_eq!(n, 97);
        assert_eq!(d, 1);
    }

    #[test]
    fn test_apply_slippage_preserves_ratio() {
        // Price 617/500000, 5% slippage
        let (n, d) = apply_slippage(617, 500000, 500);
        // 617 * 9500 = 5861500, 500000 * 10000 = 5000000000
        // GCD(5861500, 5000000000) → simplifies
        // Verify the ratio is approximately 0.95 * original
        let original = 617.0 / 500000.0;
        let slipped = n as f64 / d as f64;
        let ratio = slipped / original;
        assert!((ratio - 0.95).abs() < 0.0001);
    }

    #[test]
    fn test_apply_slippage_zero() {
        let (n, d) = apply_slippage(1000, 1, 0);
        assert_eq!(n, 1000);
        assert_eq!(d, 1);
    }

    #[test]
    fn test_gcd_basic() {
        assert_eq!(gcd(12, 8), 4);
        assert_eq!(gcd(100, 75), 25);
        assert_eq!(gcd(7, 3), 1);
        assert_eq!(gcd(0, 5), 5);
    }

    #[test]
    fn test_decimal_to_rational_high_precision() {
        // Real Splash API response: "64.51299721958981903537" (20 decimal places)
        // Should truncate to 12 and not overflow
        let (n, d) = decimal_to_rational("64.51299721958981903537").unwrap();
        // 64.512997219589 → 64512997219589 / 10^12
        assert!(n > 0);
        assert!(d > 0);
        // Verify ratio is approximately 64.51
        let ratio = n as f64 / d as f64;
        assert!((ratio - 64.513).abs() < 0.001);
    }

    #[test]
    fn test_to_splash_asset() {
        assert_eq!(to_splash_asset("lovelace"), ".");
        assert_eq!(to_splash_asset("abc123.def456"), "abc123.def456");
    }
}
