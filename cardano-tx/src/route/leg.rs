//! Leg vocabulary — what every hop in a route has in common.
//!
//! A leg is a direct spend of ONE contract UTxO with a pure quote function
//! and a builder step. It knows nothing about how it was funded or what comes
//! after it; the [`Route`](super::Route) chains them and adds change once.

use cardano_assets::{AssetId, UtxoApi};
use pallas_addresses::Address;
use pallas_txbuilder::ExUnits;

use crate::builder::fluent::TxBuilder;
use crate::builder::script::ScriptSource;
use crate::error::TxBuildError;

/// What a leg consumes or produces.
///
/// `AssetId` deliberately cannot represent ADA — "ADA is a special case with
/// no policy ID or asset name and is not represented by this type"
/// (`cardano_assets::AssetId`). The pilot route's first leg takes ADA in, so
/// the route's asset type has to name lovelace explicitly rather than encode
/// it as an empty-policy `AssetId` that every consumer would have to
/// special-case by string comparison.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RouteAsset {
    /// Lovelace.
    Ada,
    /// A native asset.
    Token(AssetId),
}

impl RouteAsset {
    /// A short label for review rows and widget columns.
    pub fn label(&self) -> String {
        match self {
            RouteAsset::Ada => "ADA".to_string(),
            RouteAsset::Token(id) => id.asset_name(),
        }
    }
}

impl std::fmt::Display for RouteAsset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteAsset::Ada => write!(f, "ADA"),
            RouteAsset::Token(id) => write!(f, "{}", id.dot_delimited()),
        }
    }
}

/// Everything a leg needs from the chain, resolved by the caller (IO).
///
/// `contract_utxo` rather than `pool_utxo`: the abstraction has to admit an
/// offer escrow and a marketplace listing, not only an AMM pool.
#[derive(Debug, Clone)]
pub struct LegState {
    /// The exact UTxO this leg will spend. Naming it is what removes slippage:
    /// if it has moved, the transaction fails at phase 1, for free.
    pub contract_utxo: UtxoApi,
    /// Its inline datum, as raw CBOR. Kept as bytes so a leg that must
    /// reproduce untouched sub-structures byte-for-byte can.
    pub datum_cbor: Vec<u8>,
    /// The validator, with its language READ OFF the reference UTxO.
    pub script: ScriptSource,
    /// The reference script's size in bytes, read off the reference UTxO —
    /// Conway charges a fee per referenced byte, and guessing it under-funds
    /// the transaction.
    pub ref_script_size: u32,
}

/// One named fee bucket. Enumerated, never summarised: the widget shows the
/// same rows LumpPad's own UI shows so a user can cross-check the quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeLine {
    pub label: &'static str,
    pub asset: RouteAsset,
    pub amount: u64,
}

/// An output a leg requires the transaction to contain.
#[derive(Debug, Clone)]
pub struct OutputSpec {
    pub address: Address,
    pub lovelace: u64,
    /// `(policy_hex, asset_name_hex, quantity)`.
    pub assets: Vec<(String, String, u64)>,
    pub inline_datum: Option<Vec<u8>>,
}

impl OutputSpec {
    /// Materialise as a pallas output.
    pub fn to_output(&self) -> Result<pallas_txbuilder::Output, TxBuildError> {
        let output = crate::helpers::output::create_ada_output(self.address.clone(), self.lovelace);
        let assets: Vec<(&str, &str, u64)> = self
            .assets
            .iter()
            .map(|(p, n, q)| (p.as_str(), n.as_str(), *q))
            .collect();
        let output = crate::helpers::output::add_assets_to_output(output, &assets)?;
        Ok(match &self.inline_datum {
            Some(datum) => output.set_inline_datum(datum.clone()),
            None => output,
        })
    }
}

/// The result of quoting one leg. Every number here is EXACT — what the
/// validator will accept, not a target with a tolerance.
#[derive(Debug, Clone)]
pub struct LegQuote {
    pub asset_in: RouteAsset,
    pub amount_in: u64,
    pub asset_out: RouteAsset,
    pub amount_out: u64,
    pub fees: Vec<FeeLine>,
    pub price_impact_bps: u32,
    /// The contract's continuing output — address, value and new inline datum.
    pub continuing_output: OutputSpec,
    /// Round-1 estimate; replaced by evaluation.
    pub seed_ex_units: ExUnits,
    /// Input the leg could not consume, and which therefore stays with the
    /// user. LumpPad's gross solver leaves 0–2 LUMP behind.
    pub remainder_in: u64,
}

impl LegQuote {
    /// What this leg actually consumed — `amount_in` less anything it could
    /// not take. The next leg is fed `amount_out`, but the route's own "you
    /// pay" line needs this.
    pub fn consumed_in(&self) -> u64 {
        self.amount_in.saturating_sub(self.remainder_in)
    }
}

/// One hop of a route.
pub trait RouteLeg {
    fn asset_in(&self, state: &LegState) -> Result<RouteAsset, RouteError>;
    fn asset_out(&self, state: &LegState) -> Result<RouteAsset, RouteError>;

    /// Pure. `amount_in` is what the previous leg produced (or the user's
    /// input).
    fn quote(&self, state: &LegState, amount_in: u64) -> Result<LegQuote, RouteError>;

    /// Stage this leg on the builder: script input, continuing output, and the
    /// reference input its script lives in.
    ///
    /// MUST NOT add user change or collateral — the route does that once, for
    /// the whole transaction.
    fn apply(
        &self,
        builder: TxBuilder,
        state: &LegState,
        quote: &LegQuote,
    ) -> Result<TxBuilder, TxBuildError>;
}

/// Why a route could not be quoted or built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    #[error("route has no legs")]
    Empty,
    #[error(
        "leg {index} takes {expected} but the previous leg produces {actual} — \
         a route's legs must chain"
    )]
    AssetMismatch {
        index: usize,
        expected: RouteAsset,
        actual: RouteAsset,
    },
    #[error("route has {legs} legs but {states} chain states were supplied")]
    StateCountMismatch { legs: usize, states: usize },
    #[error("could not decode the contract datum: {0}")]
    Datum(String),
    #[error("the pool UTxO does not hold {asset}, which its datum claims")]
    PoolValueMismatch { asset: String },
    #[error(
        "input of {amount} does not cover the venue's flat fee of {flat_fee} — \
         this trade cannot be priced"
    )]
    FlatFeeExceedsInput { amount: u64, flat_fee: u64 },
    #[error("not sellable at this size: the fees exceed what the curve returns")]
    NotSellable,
    #[error("a leg produced zero output for a non-zero input")]
    ZeroOutput,
    #[error("the pool would have to release {needed} but holds only {available}")]
    InsufficientPoolReserve { needed: u64, available: u64 },
    #[error("nothing to claim: both fee buckets are empty")]
    NothingToClaim,
    #[error("arithmetic overflow while quoting")]
    Overflow,
    #[error("{0}")]
    Registry(String),
    #[error("{0}")]
    Build(String),
}

impl From<TxBuildError> for RouteError {
    fn from(e: TxBuildError) -> Self {
        RouteError::Build(e.to_string())
    }
}
