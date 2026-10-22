//! CSWAP constant product AMM — configuration and constants
//!
//! Parameters for building swap orders against CSWAP's constant product
//! AMM pools. Reverse-engineered from on-chain transactions and pool datums.

/// CSWAP pool script address (mainnet).
/// All CSWAP constant product pools live as UTxOs at this single script address.
/// Each pool UTxO contains a pool NFT (amount=1) plus the traded token reserves.
/// To find a specific token's pool, filter UTxOs by asset at this address.
pub const POOL_SCRIPT_ADDRESS: &str = "addr1z8ke0c9p89rjfwmuh98jpt8ky74uy5mffjft3zlcld9h7ml3lmln3mwk0y3zsh3gs3dzqlwa9rjzrxawkwm4udw9axhs6fuu6e";

/// CSWAP order script payment credential (28 bytes hex).
/// This is the payment script hash of the order escrow address.
pub const ORDER_SCRIPT_HASH: &str = "da5b47aed3955c9132ee087796fa3b58a1ba6173fa31a7bc29e56d4e";

/// CSWAP order script staking credential (28 bytes hex, key-based).
pub const ORDER_STAKE_KEY_HASH: &str = "ec39fae09e0835b546eac323f7d1c46d7b7f64fe42f359aae7912b13";

/// Default execution parameter for swap orders.
///
/// It is the order's SLIPPAGE TOLERANCE in basis points, as the CSWAP UI
/// writes it: every one of twelve orders sampled on the $ALIENS pool
/// (2026-10-22) had `min_receive` exactly this many basis points under what
/// the batch then delivered — `1000` at 10%, `500` at 5%, `100` at 1%. Pass
/// the order's own tolerance where it is known.
pub const DEFAULT_EXECUTION_PARAM: u64 = 10;

/// Fee tier — 15 on every order observed. It is a fee, not a label: the
/// batcher takes this many basis points off the ADA side of every fill (a
/// buy's ADA before it reaches the pool, a sell's ADA after it leaves).
pub const DEFAULT_FEE_TIER: u64 = 15;

/// Batcher fee (lovelace) beyond [`BATCHER_NETWORK_FEE_ESTIMATE`]. None.
pub const BATCHER_FEE: u64 = 0;

/// What an order carries for the batcher's fill transaction (lovelace).
///
/// Every order sampled on 2026-10-22 was `amount + 2 ADA + 0.69 ADA`, and the
/// batch kept the 0.69 (it pays its ~0.31 ADA fee from it and keeps the rest).
/// This was 0.88 ADA when the CSWAP UI provisioned that; it no longer does.
pub const BATCHER_NETWORK_FEE_ESTIMATE: u64 = 690_000;

/// Lovelace every CSWAP pool UTxO holds outside its curve.
///
/// The pool's ADA reserve is its UTxO's lovelace LESS this. Using the whole
/// balance prices every fill a few hundred units high; with it, the model
/// reproduces six buys and a sell on the $ALIENS pool to the unit.
pub const POOL_RESERVED_LOVELACE: u64 = 2_000_000;

/// The pool NFT's asset name: `c`, under each pool's own LP policy.
pub const POOL_NFT_NAME_HEX: &str = "63";

/// Estimated network fee for our order submission TX (lovelace).
/// Simple TX with inline datum, observed ~0.175 ADA.
pub const ORDER_NETWORK_FEE_ESTIMATE: u64 = 200_000;

/// Default pool fee in basis points (0.85% = 85 bps).
/// Sourced from on-chain pool datum field `poolFee`.
/// Used as fallback when datum decoding fails.
pub const DEFAULT_POOL_FEE_BPS: u64 = 85;

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
