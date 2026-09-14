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

use cardano_assets::UtxoApi;
use cardano_assets::utxo::UtxoTag;
use pallas_addresses::Address;
use pallas_txbuilder::ExUnits;
use std::collections::BTreeMap;

use crate::builder::TxDeps;
use crate::builder::fluent::TxBuilder;
use crate::error::TxBuildError;
use crate::evaluate::TxEvaluator;

pub use leg::{
    FeeLine, LegQuote, LegState, OutputPlacement, OutputSpec, RouteAsset, RouteError, RouteLeg,
    SharesTransaction,
};
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
    fn output_placement(&self) -> OutputPlacement {
        self.as_route_leg().output_placement()
    }
    fn shares_transaction(&self) -> SharesTransaction {
        self.as_route_leg().shares_transaction()
    }
    fn stage_input(
        &self,
        builder: TxBuilder,
        state: &LegState,
        quote: &LegQuote,
    ) -> Result<TxBuilder, TxBuildError> {
        self.as_route_leg().stage_input(builder, state, quote)
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

    /// The sub-quote covering one segment, as if that span were a route of
    /// its own.
    ///
    /// The NUMBERS do not change when a route is split — each leg was quoted
    /// against its own pool state and nothing about a transaction boundary
    /// touches that. Only the boundary moves, which is what makes a chained
    /// plan quote identically to the atomic one it replaces.
    pub fn segment(&self, segment: &Segment) -> RouteQuote {
        let legs: Vec<LegQuote> = self.legs[segment.start..segment.end].to_vec();
        let first = legs.first().expect("a segment has at least one leg");
        let last = legs.last().expect("a segment has at least one leg");
        RouteQuote {
            asset_in: first.asset_in.clone(),
            amount_in: first.consumed_in(),
            asset_out: last.asset_out.clone(),
            amount_out: last.amount_out,
            fee_lines: legs.iter().flat_map(|l| l.fees.clone()).collect(),
            ex_units_total: legs
                .iter()
                .fold(ExUnits { mem: 0, steps: 0 }, |acc, l| ExUnits {
                    mem: acc.mem + l.seed_ex_units.mem,
                    steps: acc.steps + l.seed_ex_units.steps,
                }),
            legs,
        }
    }

    /// Everything the route hands back to the user, gathered into ONE output
    /// sized at its own min-UTxO: what the route produced, any leg remainders,
    /// and whatever the `funding` inputs carried beyond what the route spent.
    ///
    /// `funding` is the UTxOs staged to supply the route's INPUT asset — empty
    /// for an ADA-in route, where the builder's own coin selection covers it.
    /// Their leftovers have to be named here or the transaction does not
    /// conserve value: a UTxO is spent WHOLE, so a wallet UTxO holding 187,816
    /// LUMP put into a 30,000-LUMP trade must return 157,816 somewhere.
    ///
    /// `None` when nothing but ADA comes back, which the builder's change
    /// output already carries.
    pub fn user_output(
        &self,
        owner: &Address,
        params: &crate::params::TxBuildParams,
        funding: &[UtxoApi],
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

        // Everything the funding inputs brought in — a UTxO is spent whole,
        // including assets the route has no interest in.
        for utxo in funding {
            for asset in &utxo.assets {
                credit(&RouteAsset::Token(asset.asset_id.clone()), asset.quantity);
            }
        }
        credit(&self.asset_out, self.amount_out);
        for leg in &self.legs {
            credit(&leg.asset_in, leg.remainder_in);
        }

        // …less what the first leg actually takes off the user. Remainders are
        // credited above, so debit the amount OFFERED, not the amount
        // consumed, or the unplaceable part is counted twice.
        if let (RouteAsset::Token(id), Some(first)) = (&self.asset_in, self.legs.first())
            && let Some(held) = tokens.get_mut(id)
        {
            *held = held.saturating_sub(first.amount_in);
            if *held == 0 {
                tokens.remove(id);
            }
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

/// How many transactions a route needs, and why.
///
/// Not a preference — a consequence of what the venues will accept. A route
/// whose legs all compose settles atomically; one containing a leg that
/// refuses company cannot, at any price.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutePlan {
    /// Every leg in one transaction. Settles or fails as a unit; there is no
    /// state in which the user holds an intermediate asset.
    Atomic,
    /// Several transactions, each spending the previous one's hand-off
    /// output. Built together and signed together, but submitted in order.
    ///
    /// The trade-off is explicit: the user CAN end up holding an intermediate
    /// asset if a later transaction does not land. What they cannot get is a
    /// worse price than quoted — every leg still names the exact UTxO it
    /// spends, so a moved pool fails at phase 1 for free rather than filling
    /// badly.
    Chained { segments: usize },
}

/// One transaction's worth of a route: a span of legs that can share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// Index of the first leg, into [`Route::legs`].
    pub start: usize,
    /// One past the last leg.
    pub end: usize,
}

impl Segment {
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start >= self.end
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

    /// Split the route into the transactions its venues will actually accept.
    ///
    /// A leg that declares [`SharesTransaction::No`] gets a transaction to
    /// itself; runs of legs that compose are grouped. Pure, and decided by
    /// what the legs say rather than by who they are — a venue that later
    /// proves it tolerates company only has to change its own declaration.
    pub fn segments(&self) -> Vec<Segment> {
        let mut segments: Vec<Segment> = Vec::new();
        for (index, leg) in self.legs.iter().enumerate() {
            let solo = leg.shares_transaction() == SharesTransaction::No;
            match segments.last_mut() {
                // Extend the open segment, unless either it or this leg
                // insists on being alone.
                Some(open)
                    if !solo
                        && self.legs[open.start..open.end]
                            .iter()
                            .all(|l| l.shares_transaction() == SharesTransaction::Yes) =>
                {
                    open.end = index + 1;
                }
                _ => segments.push(Segment {
                    start: index,
                    end: index + 1,
                }),
            }
        }
        segments
    }

    /// What this route's legs force: one transaction, or several.
    pub fn plan(&self) -> RoutePlan {
        let segments = self.segments();
        if segments.len() <= 1 {
            RoutePlan::Atomic
        } else {
            RoutePlan::Chained {
                segments: segments.len(),
            }
        }
    }

    /// The sub-route covering one segment.
    pub fn sub_route(&self, segment: &Segment) -> Route {
        Route::new(self.legs[segment.start..segment.end].to_vec())
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

        // The route's INPUT asset has to arrive as an input. `TxBuilder`'s own
        // coin selection deliberately takes only asset-free UTxOs — it is
        // selecting for FEES — so a token-in route gets nothing from it, and
        // the transaction would balance in lovelace while creating tokens from
        // nowhere. That evaluates perfectly (the evaluator runs scripts; it
        // does not check value conservation) and is rejected at submit with
        // `ValueNotConservedUTxO`.
        let funding = select_input_utxos(&deps.utxos, quote)
            .map_err(|e| TxBuildError::BuildFailed(e.to_string()))?;

        let mut builder = TxBuilder::new(deps);
        for utxo in &funding {
            builder = builder.input(utxo)?;
        }
        for ((leg, state), leg_quote) in self.legs.iter().zip(states).zip(&quote.legs) {
            builder = leg.stage_input(builder, state, leg_quote)?;
        }

        // Continuing outputs, ORDERED BY PLACEMENT and not by leg order.
        //
        // A validator that locates its output at a fixed index has to get
        // that index; one that finds its own output by an NFT does not care.
        // In the pilot that means LumpPad's pool output goes first and
        // Splash's follows, which is the reverse of the leg order — and the
        // reverse of what §8.1's sketch drew.
        let mut placed: Vec<(OutputPlacement, &OutputSpec)> = self
            .legs
            .iter()
            .zip(&quote.legs)
            .map(|(leg, leg_quote)| (leg.output_placement(), &leg_quote.continuing_output))
            .collect();
        if placed
            .iter()
            .filter(|(p, _)| *p == OutputPlacement::First)
            .count()
            > 1
        {
            return Err(TxBuildError::BuildFailed(
                "two legs both require the first output; they cannot share a \
                 transaction and the route must be split"
                    .to_string(),
            ));
        }
        // `First` sorts before `Anywhere`, and the sort is STABLE, so legs
        // that do not care keep their relative order.
        placed.sort_by_key(|(placement, _)| *placement);
        for (_, output) in placed {
            builder = builder.output(output.to_output()?);
        }

        if let Some(output) = quote.user_output(&owner, &params, &funding) {
            builder = builder.output(output.to_output()?);
        }

        Ok(builder.with_collateral(crate::builder::script::CollateralConfig::Auto))
    }
}

/// Wallet UTxOs to stage so the route's input asset is actually present.
///
/// Empty for an ADA-in route: lovelace is what the builder's coin selection
/// is for. For a token-in route (a LumpPad sell, or a buy paid in LUMP) this
/// picks the fewest UTxOs that cover the amount — biggest holding first —
/// because every one of them is spent WHOLE and everything else they carry
/// has to come back in the user's output. Fewer inputs, smaller output.
fn select_input_utxos(
    available: &[UtxoApi],
    quote: &RouteQuote,
) -> Result<Vec<UtxoApi>, RouteError> {
    let RouteAsset::Token(wanted) = &quote.asset_in else {
        return Ok(Vec::new());
    };
    let needed = quote.legs.first().map(|l| l.amount_in).unwrap_or(0);
    if needed == 0 {
        return Ok(Vec::new());
    }

    let held = |utxo: &UtxoApi| -> u64 {
        utxo.assets
            .iter()
            .find(|a| a.asset_id == *wanted)
            .map(|a| a.quantity)
            .unwrap_or(0)
    };

    let mut candidates: Vec<&UtxoApi> = available
        .iter()
        .filter(|u| {
            held(u) > 0
                // A datum-bearing, script-bearing or script-address UTxO is
                // not ours to spend with a key.
                && !u.tags.contains(&UtxoTag::HasDatum)
                && !u.tags.contains(&UtxoTag::HasScriptRef)
                && !u.tags.contains(&UtxoTag::ScriptAddress)
        })
        .collect();
    candidates.sort_by_key(|u| std::cmp::Reverse(held(u)));

    let mut chosen = Vec::new();
    let mut collected = 0u64;
    for utxo in candidates {
        if collected >= needed {
            break;
        }
        collected += held(utxo);
        chosen.push(utxo.clone());
    }

    if collected < needed {
        return Err(RouteError::InsufficientInputAsset {
            asset: quote.asset_in.clone(),
            needed,
            available: collected,
        });
    }
    Ok(chosen)
}

/// A route built into the transactions it actually needs.
pub struct BuiltRoute {
    pub plan: RoutePlan,
    /// In submission order. For a chained plan, transaction `i + 1` spends an
    /// output of transaction `i`, so the order is not a preference.
    pub transactions: Vec<crate::builder::UnsignedTx>,
    /// The whole route's quote — unchanged by any split.
    pub quote: RouteQuote,
    /// The hand-off each transaction leaves for the next, in the same order.
    /// The LAST entry is what the user is left holding if the next
    /// transaction never lands; empty for an atomic plan.
    pub handoffs: Vec<crate::evaluate::PendingUtxo>,
}

impl BuiltRoute {
    /// What the user holds if transaction `index` lands and the next does
    /// not. `None` when nothing is stranded — the atomic case, or the last
    /// transaction.
    pub fn stranded_after(&self, index: usize) -> Option<&crate::evaluate::PendingUtxo> {
        self.handoffs.get(index)
    }
}

/// Build a route as its venues will accept it — one transaction, or a chain.
///
/// Each transaction is evaluated against the real validators before the next
/// is built, and a chained one is evaluated with its parent's outputs supplied
/// as [`PendingUtxo`](crate::evaluate::PendingUtxo) so the evaluator can
/// resolve inputs that are not on chain yet.
///
/// The hand-off is an ORDINARY output at the user's own address. That is what
/// makes a partial failure safe: if the second transaction never lands, the
/// user simply holds the intermediate asset in their own wallet, spendable by
/// their own key, with no contract involved.
pub async fn build_route_plan<E>(
    route: &Route,
    deps: TxDeps,
    states: &[LegState],
    amount_in: u64,
    evaluator: &E,
) -> Result<BuiltRoute, RouteError>
where
    E: TxEvaluator + ?Sized,
{
    let quote = route.quote(states, amount_in)?;
    let segments = route.segments();
    let plan = route.plan();

    let owner = deps.from_address.clone();
    let owner_bech32 = owner
        .to_bech32()
        .map_err(|e| RouteError::Registry(format!("owner address: {e}")))?;

    let mut wallet = deps.utxos.clone();
    let mut transactions = Vec::with_capacity(segments.len());
    let mut handoffs: Vec<crate::evaluate::PendingUtxo> = Vec::new();
    // Every hand-off built so far, so a later transaction can be evaluated
    // against all of them — a three-segment route's third transaction may
    // still be spending the first's change.
    let mut pending: Vec<crate::evaluate::PendingUtxo> = Vec::new();

    for (index, segment) in segments.iter().enumerate() {
        let sub_route = route.sub_route(segment);
        let sub_quote = quote.segment(segment);
        let sub_states = &states[segment.start..segment.end];
        let is_last = index + 1 == segments.len();

        let mut sub_deps = deps.clone();
        sub_deps.utxos = wallet.clone();
        // Only this segment's validators are referenced by this transaction,
        // so only their bytes are charged for.
        sub_deps.params.ref_script_size = sub_states
            .iter()
            .map(|s| u64::from(s.ref_script_size))
            .sum();

        let builder = sub_route.apply(sub_deps, sub_states, &sub_quote)?;
        let unsigned = builder.build_evaluated_pending(evaluator, &pending).await?;

        // Read the hand-off back OFF the built body rather than predicting it:
        // the builder chooses the output's min-UTxO and coin selection decides
        // what else rides along, so anything computed in advance is a guess
        // that the next transaction would then fail to spend.
        if !is_last {
            let handoff = find_handoff(&unsigned, &owner, &owner_bech32, &sub_quote)?;
            // The next segment funds from it, and it is the only wallet UTxO
            // that is guaranteed to hold the intermediate asset.
            wallet = vec![pending_to_utxo(&handoff)];
            // …plus whatever confirmed ADA the wallet still has, for fees and
            // collateral, minus what this transaction already spent.
            let spent = spent_refs(&unsigned);
            wallet.extend(
                deps.utxos
                    .iter()
                    .filter(|u| !spent.contains(&(u.tx_hash.clone(), u.output_index)))
                    .cloned(),
            );
            pending.push(handoff.clone());
            handoffs.push(handoff);
        }

        transactions.push(unsigned);
    }

    Ok(BuiltRoute {
        plan,
        transactions,
        quote,
        handoffs,
    })
}

/// The output a transaction leaves for the next one: at the user's own
/// address, carrying the segment's output asset.
fn find_handoff(
    unsigned: &crate::builder::UnsignedTx,
    owner: &Address,
    owner_bech32: &str,
    sub_quote: &RouteQuote,
) -> Result<crate::evaluate::PendingUtxo, RouteError> {
    use pallas_txbuilder::BuildConway;

    // The hash is over the BODY, and signing only adds witnesses — so this is
    // the reference the next transaction will spend, known before anyone signs
    // anything. That is what makes chained building possible at all.
    let built = unsigned
        .staging
        .clone()
        .build_conway_raw()
        .map_err(|e| RouteError::Build(format!("serialise a chained transaction: {e}")))?;
    let tx_hash = hex::encode(built.tx_hash.0);

    let wanted = match &sub_quote.asset_out {
        RouteAsset::Ada => None,
        RouteAsset::Token(id) => Some((id.policy_id.clone(), id.asset_name_hex.clone())),
    };
    let owner_bytes = owner.to_vec();

    for (index, output) in unsigned.staging.outputs.iter().flatten().enumerate() {
        if output.address.to_vec() != owner_bytes {
            continue;
        }
        let assets: Vec<(String, String, u64)> = output
            .assets
            .iter()
            .flat_map(|bundle| bundle.iter())
            .flat_map(|(policy, names)| {
                names.iter().map(move |(name, quantity)| {
                    (hex::encode(policy.0), hex::encode(&name.0), *quantity)
                })
            })
            .collect();

        let carries = match &wanted {
            Some((policy, name)) => assets
                .iter()
                .any(|(p, n, q)| p == policy && n == name && *q >= sub_quote.amount_out),
            // An ADA hand-off is the change output — whichever of the user's
            // outputs carries no assets.
            None => assets.is_empty(),
        };
        if carries {
            return Ok(crate::evaluate::PendingUtxo {
                tx_hash,
                index: index as u32,
                address: owner_bech32.to_string(),
                lovelace: output.lovelace,
                assets,
            });
        }
    }

    Err(RouteError::Build(format!(
        "the transaction for this segment produced no output at the user's address \
         carrying {} {} — the next transaction would have nothing to spend",
        sub_quote.amount_out, sub_quote.asset_out
    )))
}

fn pending_to_utxo(pending: &crate::evaluate::PendingUtxo) -> UtxoApi {
    UtxoApi {
        tx_hash: pending.tx_hash.clone(),
        output_index: pending.index,
        lovelace: pending.lovelace,
        assets: pending
            .assets
            .iter()
            .map(
                |(policy, name, quantity)| cardano_assets::utxo::AssetQuantity {
                    asset_id: cardano_assets::AssetId::new_unchecked(policy.clone(), name.clone()),
                    quantity: *quantity,
                },
            )
            .collect(),
        tags: Vec::new(),
    }
}

/// Which wallet UTxOs a built transaction consumed.
fn spent_refs(unsigned: &crate::builder::UnsignedTx) -> std::collections::HashSet<(String, u32)> {
    unsigned
        .staging
        .inputs
        .iter()
        .flatten()
        .map(|i| (hex::encode(i.tx_hash.0), i.txo_index as u32))
        .collect()
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
