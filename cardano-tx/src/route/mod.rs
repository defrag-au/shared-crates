//! Composed routes — multi-venue value routing inside ONE transaction.
//!
//! A Cardano transaction is one balance sheet. If every script we spend only
//! constrains its OWN continuing output and its named payouts, the value one
//! leg releases can be consumed by the next leg's script output in the same
//! transaction.
//!
//! A [`Route`] is an ordered list of legs; each leg is a direct spend of one
//! contract UTxO with a pure quote function and a builder step. Because every
//! leg names the exact UTxO it spends, a composed route has **no slippage**:
//! it settles at the quoted numbers, or it fails at phase 1 because a pool
//! UTxO moved — at zero cost — and is rebuilt from fresh state.
//!
//! Layering: nothing here knows about Koios, workers or egui. It takes
//! resolved [`LegState`]s in and returns quotes and a [`TxBuilder`] out. IO
//! (fetching pool UTxOs, evaluation, submit) belongs to the caller.
//!
//! Spec: `docs/design/COMPOSED_ROUTES.md`.

pub mod leg;
pub mod lumppad;
#[cfg(test)]
mod pilot;
pub mod splash_pool;

use pallas_addresses::Address;
use pallas_txbuilder::ExUnits;
use std::collections::BTreeMap;

use crate::builder::TxDeps;
use crate::builder::fluent::TxBuilder;
use crate::error::TxBuildError;
use crate::evaluate::TxEvaluator;

pub use leg::{FeeLine, LegQuote, LegState, OutputSpec, RouteAsset, RouteError, RouteLeg};
pub use lumppad::{LumpPadBuyLeg, LumpPadSellLeg};
pub use splash_pool::{SplashDirection, SplashSwapLeg};

/// Parse a registry address, turning a malformed one into a route error
/// rather than a panic.
pub(crate) fn parse_address(bech32: &str) -> Result<Address, RouteError> {
    Address::from_bech32(bech32).map_err(|e| {
        RouteError::Registry(format!("registry address {bech32} does not decode: {e}"))
    })
}

/// One hop of a route.
///
/// An enum, not a trait object: the cart action this becomes has to
/// serialise, and the set of venues we can spend directly is closed by design
/// — a leg that needs a batcher is not a leg.
#[derive(Debug, Clone)]
pub enum Leg {
    /// Direct royalty-pool spend, either direction.
    SplashSwap(SplashSwapLeg),
    /// LUMP → the pool's token.
    LumpPadBuy(LumpPadBuyLeg),
    /// The pool's token → LUMP.
    LumpPadSell(LumpPadSellLeg),
}

impl Leg {
    fn as_route_leg(&self) -> &dyn RouteLeg {
        match self {
            Leg::SplashSwap(l) => l,
            Leg::LumpPadBuy(l) => l,
            Leg::LumpPadSell(l) => l,
        }
    }
}

impl RouteLeg for Leg {
    fn asset_in(&self, state: &LegState) -> Result<RouteAsset, RouteError> {
        self.as_route_leg().asset_in(state)
    }
    fn asset_out(&self, state: &LegState) -> Result<RouteAsset, RouteError> {
        self.as_route_leg().asset_out(state)
    }
    fn quote(&self, state: &LegState, amount_in: u64) -> Result<LegQuote, RouteError> {
        self.as_route_leg().quote(state, amount_in)
    }
    fn apply(
        &self,
        builder: TxBuilder,
        state: &LegState,
        quote: &LegQuote,
    ) -> Result<TxBuilder, TxBuildError> {
        self.as_route_leg().apply(builder, state, quote)
    }
}

/// A quoted route: every leg's exact numbers, and the totals a user signs for.
#[derive(Debug, Clone)]
pub struct RouteQuote {
    pub legs: Vec<LegQuote>,
    pub asset_in: RouteAsset,
    /// What the user parts with — the FIRST leg's consumed input.
    pub amount_in: u64,
    pub asset_out: RouteAsset,
    /// What the user receives — the LAST leg's output.
    pub amount_out: u64,
    /// Every fee bucket, flattened in leg order.
    pub fee_lines: Vec<FeeLine>,
    /// Seeded before evaluation, real after it.
    pub ex_units_total: ExUnits,
}

impl RouteQuote {
    /// Anything a leg could not consume and which therefore comes back to the
    /// user, by asset. LumpPad's gross solver leaves 0–2 LUMP.
    pub fn remainders(&self) -> Vec<(RouteAsset, u64)> {
        self.legs
            .iter()
            .filter(|l| l.remainder_in > 0)
            .map(|l| (l.asset_in.clone(), l.remainder_in))
            .collect()
    }

    /// The worst single-leg price impact, which is the one worth showing.
    pub fn price_impact_bps(&self) -> u32 {
        self.legs
            .iter()
            .map(|l| l.price_impact_bps)
            .max()
            .unwrap_or(0)
    }

    /// Everything the route hands back to the user: the final output, plus any
    /// leg remainders, gathered into ONE output sized at its own min-UTxO.
    ///
    /// `None` when the route produces only ADA, which the builder's change
    /// output already carries.
    pub fn user_output(
        &self,
        owner: &Address,
        params: &crate::params::TxBuildParams,
    ) -> Option<OutputSpec> {
        let mut tokens: BTreeMap<cardano_assets::AssetId, u64> = BTreeMap::new();
        let mut lovelace_out = 0u64;

        let mut credit = |asset: &RouteAsset, amount: u64| {
            if amount == 0 {
                return;
            }
            match asset {
                RouteAsset::Ada => lovelace_out += amount,
                RouteAsset::Token(id) => *tokens.entry(id.clone()).or_default() += amount,
            }
        };

        credit(&self.asset_out, self.amount_out);
        for leg in &self.legs {
            credit(&leg.asset_in, leg.remainder_in);
        }

        if tokens.is_empty() {
            return None;
        }

        let assets: Vec<crate::utxo::AssetAmount> =
            tokens.iter().map(|(id, q)| (id.clone(), *q)).collect();
        // An asset-bearing output has to clear its OWN min-UTxO, which depends
        // on the quantities' CBOR width — not a flat per-asset estimate.
        let lovelace = params.min_utxo_for_assets(&assets).max(lovelace_out);

        Some(OutputSpec {
            address: owner.clone(),
            lovelace,
            assets: tokens
                .into_iter()
                .map(|(id, q)| (id.policy_id.clone(), id.asset_name_hex.clone(), q))
                .collect(),
            inline_datum: None,
        })
    }
}

/// An ordered list of legs, each spending one named contract UTxO.
#[derive(Debug, Clone)]
pub struct Route {
    pub legs: Vec<Leg>,
}

impl Route {
    pub fn new(legs: Vec<Leg>) -> Self {
        Self { legs }
    }

    /// Pure. Validates that the legs chain (`leg[i].asset_out ==
    /// leg[i+1].asset_in`) and folds each leg's quote into the next.
    pub fn quote(&self, states: &[LegState], amount_in: u64) -> Result<RouteQuote, RouteError> {
        if self.legs.is_empty() {
            return Err(RouteError::Empty);
        }
        if states.len() != self.legs.len() {
            return Err(RouteError::StateCountMismatch {
                legs: self.legs.len(),
                states: states.len(),
            });
        }

        let mut quotes: Vec<LegQuote> = Vec::with_capacity(self.legs.len());
        let mut carried = amount_in;

        for (index, (leg, state)) in self.legs.iter().zip(states).enumerate() {
            if let Some(previous) = quotes.last() {
                let expected = leg.asset_in(state)?;
                if expected != previous.asset_out {
                    return Err(RouteError::AssetMismatch {
                        index,
                        expected,
                        actual: previous.asset_out.clone(),
                    });
                }
            }
            let quote = leg.quote(state, carried)?;
            carried = quote.amount_out;
            quotes.push(quote);
        }

        let first = &quotes[0];
        let last = quotes.last().expect("non-empty");
        let fee_lines = quotes.iter().flat_map(|q| q.fees.clone()).collect();
        let ex_units_total = quotes
            .iter()
            .fold(ExUnits { mem: 0, steps: 0 }, |acc, q| ExUnits {
                mem: acc.mem + q.seed_ex_units.mem,
                steps: acc.steps + q.seed_ex_units.steps,
            });

        Ok(RouteQuote {
            asset_in: first.asset_in.clone(),
            // What the user actually parts with, not what they offered: a
            // first leg that could not place all of its input hands the
            // remainder straight back.
            amount_in: first.consumed_in(),
            asset_out: last.asset_out.clone(),
            amount_out: last.amount_out,
            fee_lines,
            ex_units_total,
            legs: quotes,
        })
    }

    /// Stage every leg onto a builder, then the output that hands the user
    /// what the route produced.
    ///
    /// Continuing outputs are emitted in LEG ORDER and BEFORE anything else:
    /// LumpPad's pool output is at index 0 in every historical transaction
    /// (`LUMPPAD_INTEGRATION.md` §6.1 / §8.1 item 3), and until a transaction
    /// with the pool elsewhere has succeeded on chain we treat that as
    /// required. Splash finds its own output by NFT, so it does not care.
    ///
    /// Collateral and change are the ROUTE's job, once — not each leg's.
    ///
    /// The user's output is explicit rather than left to `TxBuilder`'s change,
    /// which is ADA-only. A route RELEASES native assets from a pool; if
    /// nothing names an output for them the transaction does not conserve
    /// value and the node rejects it with `ValueNotConservedUTxO`.
    pub fn apply(
        &self,
        deps: TxDeps,
        states: &[LegState],
        quote: &RouteQuote,
    ) -> Result<TxBuilder, TxBuildError> {
        let owner = deps.from_address.clone();
        let params = deps.params.clone();
        let mut builder = TxBuilder::new(deps);
        for ((leg, state), leg_quote) in self.legs.iter().zip(states).zip(&quote.legs) {
            builder = leg.apply(builder, state, leg_quote)?;
        }

        if let Some(output) = quote.user_output(&owner, &params) {
            builder = builder.output(output.to_output()?);
        }

        Ok(builder.with_collateral(crate::builder::script::CollateralConfig::Auto))
    }
}

/// Quote, build and EVALUATE a route in one call — the entry point a worker
/// uses.
///
/// Evaluation failures must reach the caller intact: the Ogmios body is the
/// only thing that says which redeemer failed and why.
pub async fn build_route_evaluated<E>(
    route: &Route,
    deps: TxDeps,
    states: &[LegState],
    amount_in: u64,
    evaluator: &E,
) -> Result<(crate::builder::UnsignedTx, RouteQuote), RouteError>
where
    E: TxEvaluator + ?Sized,
{
    let mut quote = route.quote(states, amount_in)?;
    let builder = route.apply(deps, states, &quote)?;
    let unsigned = builder.build_evaluated(evaluator).await?;
    quote.ex_units_total = evaluated_ex_units(&unsigned);
    Ok((unsigned, quote))
}

/// Sum the execution budget actually booked in a built transaction.
fn evaluated_ex_units(unsigned: &crate::builder::UnsignedTx) -> ExUnits {
    let Some(redeemers) = unsigned.staging.redeemers.as_ref() else {
        return ExUnits { mem: 0, steps: 0 };
    };
    redeemers
        .iter()
        .filter_map(|(_, (_, units))| units.clone())
        .fold(ExUnits { mem: 0, steps: 0 }, |acc, u| ExUnits {
            mem: acc.mem + u.mem,
            steps: acc.steps + u.steps,
        })
}
