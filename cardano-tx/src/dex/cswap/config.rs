//! CSWAP concentrated liquidity pool — configuration and constants
//!
//! Parameters for building swap orders against CSWAP's concentrated
//! liquidity pools. Reverse-engineered from on-chain transactions.

/// CSWAP order script payment credential (28 bytes hex).
/// This is the payment script hash of the order escrow address.
pub const ORDER_SCRIPT_HASH: &str = "da5b47aed3955c9132ee087796fa3b58a1ba6173fa31a7bc29e56d4e";

/// CSWAP order script staking credential (28 bytes hex, key-based).
pub const ORDER_STAKE_KEY_HASH: &str = "ec39fae09e0835b546eac323f7d1c46d7b7f64fe42f359aae7912b13";

/// Default execution parameter for swap orders.
/// CSWAP UI uses 10 for standard market swaps.
pub const DEFAULT_EXECUTION_PARAM: u64 = 10;

/// Fee tier constant — always 15 across all observed orders.
pub const DEFAULT_FEE_TIER: u64 = 15;

/// Estimated batcher fee (lovelace). The batcher takes whatever ADA remains
/// after the pool swap and user output. Observed actual fee ~0.75 ADA.
pub const BATCHER_FEE_ESTIMATE: u64 = 750_000;

/// Minimum UTxO ADA returned to the user with their tokens (lovelace).
pub const MIN_UTXO_RETURN: u64 = 2_000_000;

// --- Cancel order constants ---

/// Cancel redeemer: Constructor 0 {} encoded as CBOR.
pub const CANCEL_REDEEMER_HEX: &str = "d87980";

/// Script reference UTxO containing the CSWAP order validator (PlutusV3).
/// Used as a reference input so we don't need to include the full script.
pub const CANCEL_SCRIPT_REF_TX: &str =
    "e8a645e941ba725b720b40bfdd903b4e78673364751860eb050ce15fa23a47af";
pub const CANCEL_SCRIPT_REF_INDEX: u64 = 0;

/// Execution units budget for cancel (observed from on-chain cancel TX).
/// We over-provision slightly for safety.
pub const CANCEL_EX_UNITS_MEM: u64 = 300_000;
pub const CANCEL_EX_UNITS_STEPS: u64 = 100_000_000;

/// Estimated script execution fee for cancel (lovelace).
/// Derived from observed fee ~0.30 ADA minus size fee component.
pub const CANCEL_SCRIPT_FEE: u64 = 150_000;
