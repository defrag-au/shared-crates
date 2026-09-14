//! The Splash royalty-pool swap leg.
//!
//! Transaction shape (`LUMPPAD_INTEGRATION.md` §7.5): the validator finds its
//! continuing output BY NFT, so the output's index is free; the value must
//! carry the same number of distinct tokens; the LP supply must not move
//! (`dlq == 0`); and the new datum bumps only the input side's treasury and
//! royalty counters.

use pallas_txbuilder::ExUnits;

use super::datum::RoyaltyPoolDatum;
use super::pool_address;
use super::quote::{PoolReserves, SplashDirection, price_impact_bps, quote_swap};
use crate::builder::fluent::TxBuilder;
use crate::builder::script::{RedeemerSource, ScriptInput, constr};
use crate::error::TxBuildError;
use crate::route::leg::{
    FeeLine, LegQuote, LegState, OutputSpec, RouteAsset, RouteError, RouteLeg,
};

/// `Swap = Constr 2 []` — the action inside the pool redeemer.
const ACTION_SWAP: u32 = 2;
/// The pool redeemer is `Constr 0 [action, selfIx]`.
const REDEEMER_CONSTRUCTOR: u32 = 0;

/// Observed 662K mem / 228M steps; seeded above that and replaced by the
/// evaluator.
const SEED_SWAP: ExUnits = ExUnits {
    mem: 730_000,
    steps: 250_000_000,
};

/// A direct spend of one Splash royalty pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplashSwapLeg {
    pub direction: SplashDirection,
}

impl SplashSwapLeg {
    pub fn new(direction: SplashDirection) -> Self {
        Self { direction }
    }

    /// The pool's holdings of `poolX` and `poolY`, read off the UTxO.
    ///
    /// The datum names the assets; the UTxO says how much of each is actually
    /// there. Using the datum's counters as reserves is the classic error —
    /// they are what is OWED out of the balance, not the balance.
    fn reserves(
        &self,
        state: &LegState,
        datum: &RoyaltyPoolDatum,
    ) -> Result<PoolReserves, RouteError> {
        Ok(PoolReserves {
            x: side_quantity(state, &datum.pool_x()?)?,
            y: side_quantity(state, &datum.pool_y()?)?,
        })
    }

    fn assets(&self, datum: &RoyaltyPoolDatum) -> Result<(RouteAsset, RouteAsset), RouteError> {
        let x = to_route_asset(&datum.pool_x()?)?;
        let y = to_route_asset(&datum.pool_y()?)?;
        Ok(match self.direction {
            SplashDirection::XToY => (x, y),
            SplashDirection::YToX => (y, x),
        })
    }
}

fn to_route_asset(asset: &super::datum::DatumAssetRef) -> Result<RouteAsset, RouteError> {
    if asset.is_ada() {
        return Ok(RouteAsset::Ada);
    }
    cardano_assets::AssetId::new(asset.policy_hex(), asset.name_hex())
        .map(RouteAsset::Token)
        .map_err(|e| RouteError::Datum(format!("pool asset: {e}")))
}

/// How much of one datum-named asset the pool UTxO holds.
fn side_quantity(state: &LegState, asset: &super::datum::DatumAssetRef) -> Result<u64, RouteError> {
    if asset.is_ada() {
        return Ok(state.contract_utxo.lovelace);
    }
    let policy = asset.policy_hex();
    let name = asset.name_hex();
    state
        .contract_utxo
        .assets
        .iter()
        .find(|a| a.asset_id.policy_id == policy && a.asset_id.asset_name_hex == name)
        .map(|a| a.quantity)
        .ok_or_else(|| RouteError::PoolValueMismatch {
            asset: format!("{policy}.{name}"),
        })
}

impl RouteLeg for SplashSwapLeg {
    fn asset_in(&self, state: &LegState) -> Result<RouteAsset, RouteError> {
        Ok(self
            .assets(&RoyaltyPoolDatum::from_cbor(&state.datum_cbor)?)?
            .0)
    }

    fn asset_out(&self, state: &LegState) -> Result<RouteAsset, RouteError> {
        Ok(self
            .assets(&RoyaltyPoolDatum::from_cbor(&state.datum_cbor)?)?
            .1)
    }

    fn quote(&self, state: &LegState, amount_in: u64) -> Result<LegQuote, RouteError> {
        let before = RoyaltyPoolDatum::from_cbor(&state.datum_cbor)?;
        let reserves = self.reserves(state, &before)?;
        let q = quote_swap(&before, reserves, self.direction, amount_in)?;
        let (asset_in, asset_out) = self.assets(&before)?;

        // The continuing output is the pool UTxO's value with the two swapped
        // quantities adjusted and EVERY other asset carried through — the
        // validator checks the token count is unchanged, and the LP supply
        // must not move.
        let (x_after, y_after) = match self.direction {
            SplashDirection::XToY => (
                reserves
                    .x
                    .checked_add(amount_in)
                    .ok_or(RouteError::Overflow)?,
                reserves
                    .y
                    .checked_sub(q.amount_out)
                    .ok_or(RouteError::Overflow)?,
            ),
            SplashDirection::YToX => (
                reserves
                    .x
                    .checked_sub(q.amount_out)
                    .ok_or(RouteError::Overflow)?,
                reserves
                    .y
                    .checked_add(amount_in)
                    .ok_or(RouteError::Overflow)?,
            ),
        };
        let continuing_output =
            build_continuing_output(state, &before, &q.after, x_after, y_after)?;

        Ok(LegQuote {
            asset_in: asset_in.clone(),
            amount_in,
            asset_out,
            amount_out: q.amount_out,
            fees: vec![
                FeeLine {
                    label: "Splash LP",
                    asset: asset_in.clone(),
                    amount: q.lp_fee,
                },
                FeeLine {
                    label: "Splash treasury",
                    asset: asset_in.clone(),
                    amount: q.treasury_fee,
                },
                FeeLine {
                    label: "Splash royalty",
                    asset: asset_in,
                    amount: q.royalty_fee,
                },
            ],
            price_impact_bps: price_impact_bps(
                q.effective_in_reserve,
                q.effective_out_reserve,
                amount_in,
                q.amount_out,
            ),
            continuing_output,
            seed_ex_units: SEED_SWAP,
            remainder_in: 0,
        })
    }

    fn apply(
        &self,
        builder: TxBuilder,
        state: &LegState,
        quote: &LegQuote,
    ) -> Result<TxBuilder, TxBuildError> {
        let builder = builder.spend_script_utxo(
            &state.contract_utxo,
            ScriptInput {
                script: state.script.clone(),
                datum_cbor: None,
                // `selfIx` is resolved at assembly from the LEDGER's sorted
                // input order, which is only final after coin selection. It is
                // NOT the order legs were staged in.
                redeemer: RedeemerSource::SelfIndexed {
                    constructor: REDEEMER_CONSTRUCTOR,
                    action: constr(ACTION_SWAP, vec![]),
                },
                ex_units: quote.seed_ex_units.clone(),
            },
        )?;
        Ok(builder.output(quote.continuing_output.to_output()?))
    }
}

/// The pool's continuing output: every asset it held, with the two swapped
/// quantities replaced, and the bumped datum inline.
fn build_continuing_output(
    state: &LegState,
    before: &RoyaltyPoolDatum,
    after: &RoyaltyPoolDatum,
    x_after: u64,
    y_after: u64,
) -> Result<OutputSpec, RouteError> {
    let x = before.pool_x()?;
    let y = before.pool_y()?;

    let mut lovelace = state.contract_utxo.lovelace;
    if x.is_ada() {
        lovelace = x_after;
    }
    if y.is_ada() {
        lovelace = y_after;
    }

    let mut assets: Vec<(String, String, u64)> = Vec::new();
    for held in &state.contract_utxo.assets {
        let policy = held.asset_id.policy_id.clone();
        let name = held.asset_id.asset_name_hex.clone();
        let quantity = if !x.is_ada() && policy == x.policy_hex() && name == x.name_hex() {
            x_after
        } else if !y.is_ada() && policy == y.policy_hex() && name == y.name_hex() {
            y_after
        } else {
            // The pool NFT and the LP token ride through untouched — the
            // validator requires `dlq == 0` and finds itself by NFT.
            held.quantity
        };
        if quantity == 0 {
            return Err(RouteError::PoolValueMismatch {
                asset: format!("{policy}.{name} would leave the pool entirely"),
            });
        }
        assets.push((policy, name, quantity));
    }

    Ok(OutputSpec {
        address: pool_address()?,
        lovelace,
        assets,
        inline_datum: Some(after.to_cbor()?),
    })
}
