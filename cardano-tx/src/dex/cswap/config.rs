//! CSWAP concentrated liquidity pool — configuration and constants
//!
//! Parameters for building swap orders against CSWAP's concentrated
//! liquidity pools. Reverse-engineered from on-chain transactions.

/// CSWAP order script payment credential (28 bytes hex).
/// This is the payment script hash of the order escrow address.
pub const ORDER_SCRIPT_HASH: &str = "da5b47aed3955c9132ee087796fa3b58a1ba6173fa31a7bc29e56d4e";

/// CSWAP order script staking credential (28 bytes hex, key-based).
pub const ORDER_STAKE_KEY_HASH: &str = "ec39fae09e0835b546eac323f7d1c46d7b7f64fe42f359aae7912b13";

/// Default execution parameter for buy orders (ADA -> Token).
/// Observed values: 5, 1000, 8000. Using 1000 as the safe default for buys.
pub const DEFAULT_EXECUTION_PARAM: u64 = 1000;

/// Fee tier constant — always 15 across all observed orders.
pub const DEFAULT_FEE_TIER: u64 = 15;

/// Estimated batcher fee (lovelace). The batcher takes whatever ADA remains
/// after the pool swap and user output. Observed range: 0.5–0.8 ADA.
/// We provision 0.7 ADA to ensure the batcher has incentive to fill.
pub const BATCHER_FEE_ESTIMATE: u64 = 700_000;

/// Minimum UTxO ADA returned to the user with their tokens (lovelace).
pub const MIN_UTXO_RETURN: u64 = 2_000_000;
