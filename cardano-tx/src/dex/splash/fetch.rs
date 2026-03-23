//! Splash DEX API fetch functions
//!
//! Dynamic executor fee and order book APIs from analytics.splash.trade.
//! Uses http-client for platform-agnostic HTTP (works on both native and WASM).

use http_client::HttpClient;

use super::config::{
    decimal_to_rational, derive_lp_fee_bps, to_splash_asset, FeeApiResponse, FeeDistribution,
    OrderBookQuote, RawOrderBook, MAINNET_FEE_URL, ORDER_BOOK_URL,
};
use super::SplashError;

/// Fetch dynamic executor fee for a specific trading pair and amount.
///
/// Falls back to the default executor fee if the API is unavailable.
///
/// # Arguments
/// * `input_asset` - Input asset identifier ("lovelace" for ADA, or "policy_id.asset_name")
/// * `output_asset` - Output asset identifier
/// * `amount` - Input amount (for finding the correct fee tier)
/// * `default_fee` - Fallback fee if API call fails
pub async fn fetch_executor_fee(
    input_asset: &str,
    output_asset: &str,
    amount: u64,
    default_fee: u64,
) -> u64 {
    match fetch_executor_fee_inner(input_asset, output_asset, amount).await {
        Ok(fee) => fee,
        Err(e) => {
            tracing::warn!("Failed to fetch executor fee, using default {default_fee}: {e}");
            default_fee
        }
    }
}

async fn fetch_executor_fee_inner(
    input_asset: &str,
    output_asset: &str,
    amount: u64,
) -> Result<u64, SplashError> {
    let dist = fetch_fee_distribution(input_asset, output_asset).await?;
    dist.steps
        .iter()
        .find(|s| amount >= s.lower_bound() && amount < s.upper_bound())
        .or(dist.steps.last())
        .map(|s| s.fee)
        .ok_or_else(|| SplashError::ConfigFetch("empty fee distribution".to_string()))
}

/// Fetch the full fee distribution table for a trading pair.
///
/// Returns all fee tiers — callers can look up specific amounts via
/// `FeeDistribution::fee_for_amount()`.
pub async fn fetch_fee_distribution(
    input_asset: &str,
    output_asset: &str,
) -> Result<FeeDistribution, SplashError> {
    let from = to_splash_asset(input_asset);
    let to = to_splash_asset(output_asset);
    let url = format!("{MAINNET_FEE_URL}?from={from}&to={to}");
    let client = HttpClient::new();
    let resp: FeeApiResponse = client
        .get(&url)
        .await
        .map_err(|e| SplashError::ConfigFetch(format!("fee fetch failed: {e}")))?;

    // Pick the right direction: ADA input → fromAdaSteps, token input → fromAssetSteps
    let steps = if input_asset == "lovelace" {
        resp.config.from_ada_steps
    } else {
        resp.config.from_asset_steps
    };

    Ok(FeeDistribution { steps })
}

/// Fetch order book and extract spot price for a trading pair.
///
/// Accepts our internal asset identifiers (`"lovelace"` or `"policy_id.asset_name_hex"`)
/// in any order. The Splash API always lists pairs as `base=token, quote=ADA`,
/// so this function normalizes the pair orientation automatically.
///
/// The returned spot price is always **lovelace per token**.
pub async fn fetch_spot_price(asset_a: &str, asset_b: &str) -> Result<OrderBookQuote, SplashError> {
    // Splash pairs are always base=token, quote=ADA (".")
    // Figure out which asset is the token and which is ADA
    let (token, _ada) = if asset_a == "lovelace" {
        (asset_b, asset_a)
    } else {
        (asset_a, asset_b)
    };

    let splash_token = to_splash_asset(token);
    let url = format!("{ORDER_BOOK_URL}?base={splash_token}&quote=.");

    let client = HttpClient::new();
    let book: RawOrderBook = client
        .get(&url)
        .await
        .map_err(|e| SplashError::ConfigFetch(format!("order book fetch failed: {e}")))?;

    let (numerator, denominator) = decimal_to_rational(&book.spot)?;

    let amm_base_reserves = book
        .amm_total_liquidity_base
        .as_deref()
        .and_then(|s| s.parse::<u64>().ok());
    let amm_quote_reserves = book
        .amm_total_liquidity_quote
        .as_deref()
        .and_then(|s| s.parse::<u64>().ok());

    let lp_fee_bps = derive_lp_fee_bps(amm_base_reserves, amm_quote_reserves, &book.asks);

    Ok(OrderBookQuote {
        spot_price: book.spot,
        numerator,
        denominator,
        bids: book.bids,
        asks: book.asks,
        amm_base_reserves,
        amm_quote_reserves,
        lp_fee_bps,
    })
}
