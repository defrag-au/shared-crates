//! LumpPad route legs — Buy and Sell — and the standalone Claim builder.
//!
//! Transaction shape, verified on 31 mainnet trades
//! (`LUMPPAD_INTEGRATION.md` §6.1): the pool's continuing output is index 0 in
//! EVERY observed transaction, it is the only pool output, and the amounts are
//! exactly what the formulas give — never "with a margin left".

use pallas_addresses::Address;
use pallas_txbuilder::ExUnits;

use super::datum::PoolDatum;
use super::quote::{price_impact_bps, quote_buy, quote_sell, spot_price};
use super::{lump_asset, pool_address, token_asset};
use crate::builder::fluent::TxBuilder;
use crate::builder::script::{RedeemerSource, constr, encode_plutus_data, int};
use crate::error::TxBuildError;
use crate::route::leg::{
    FeeLine, LegQuote, LegState, OutputPlacement, OutputSpec, RouteAsset, RouteError, RouteLeg,
    SharesTransaction,
};

/// Redeemer constructor indices. The validator's UPLC dispatch compares
/// against exactly 0, 1 and 2 — there is no fourth path.
const REDEEMER_BUY: u32 = 0;
const REDEEMER_SELL: u32 = 1;
const REDEEMER_CLAIM: u32 = 2;

/// Seed budgets, from the ranges observed on chain plus headroom. Replaced by
/// the evaluator's figures on the second build round.
const SEED_BUY: ExUnits = ExUnits {
    mem: 470_000,
    steps: 160_000_000,
};
const SEED_SELL: ExUnits = ExUnits {
    mem: 480_000,
    steps: 160_000_000,
};
const SEED_CLAIM: ExUnits = ExUnits {
    mem: 650_000,
    steps: 220_000_000,
};

/// Buy: LUMP in, the pool's token out.
#[derive(Debug, Clone)]
pub struct LumpPadBuyLeg;

/// Sell: the pool's token in, LUMP out.
#[derive(Debug, Clone)]
pub struct LumpPadSellLeg;

/// The pool's continuing output for a given after-state.
///
/// `3 ADA + 1 state NFT + T tokens + (R + A + B) LUMP`, with the new datum
/// inline. Zero quantities are omitted — the ledger has no representation for
/// a zero-quantity asset in an output.
fn continuing_output(after: &PoolDatum) -> Result<OutputSpec, RouteError> {
    let deployment = address_registry::dex::LUMPPAD;
    let mut assets = vec![(
        after.token_policy_hex(),
        deployment.state_nft_name_hex.to_string(),
        1,
    )];
    if after.tokens > 0 {
        assets.push((
            after.token_policy_hex(),
            after.token_name_hex(),
            after.tokens,
        ));
    }
    let lump = after.lump_in_pool()?;
    if lump > 0 {
        assets.push((
            deployment.lump.policy_id.to_string(),
            deployment.lump.asset_name_hex.to_string(),
            lump,
        ));
    }
    Ok(OutputSpec {
        address: pool_address()?,
        lovelace: deployment.pool_min_lovelace,
        assets,
        inline_datum: Some(after.to_cbor()?),
    })
}

/// The fee rows both directions share, in the order LumpPad's own UI lists
/// them so a user can cross-check our quote against theirs.
fn fee_lines(platform_fee: u64, creator_fee: u64, swap_kept: u64) -> Vec<FeeLine> {
    let lump = RouteAsset::Token(lump_asset());
    vec![
        FeeLine {
            label: "LumpPad swap (stays in pool)",
            asset: lump.clone(),
            amount: swap_kept,
        },
        FeeLine {
            label: "LumpPad platform (half burns)",
            asset: lump.clone(),
            amount: platform_fee,
        },
        FeeLine {
            label: "LumpPad creator",
            asset: lump,
            amount: creator_fee,
        },
    ]
}

impl RouteLeg for LumpPadBuyLeg {
    fn asset_in(&self, _state: &LegState) -> Result<RouteAsset, RouteError> {
        Ok(RouteAsset::Token(lump_asset()))
    }

    fn asset_out(&self, state: &LegState) -> Result<RouteAsset, RouteError> {
        Ok(RouteAsset::Token(token_asset(&PoolDatum::from_cbor(
            &state.datum_cbor,
        )?)?))
    }

    /// `amount_in` is GROSS LUMP — what the previous leg handed us, or what
    /// the user typed. The solver splits it into the curve leg and the two
    /// buckets and hands back any 0–2 LUMP it could not place.
    fn quote(&self, state: &LegState, amount_in: u64) -> Result<LegQuote, RouteError> {
        let before = PoolDatum::from_cbor(&state.datum_cbor)?;
        let q = quote_buy(&before, amount_in)?;
        let (spot_num, spot_den) = spot_price(&before)?;

        Ok(LegQuote {
            asset_in: RouteAsset::Token(lump_asset()),
            amount_in,
            asset_out: RouteAsset::Token(token_asset(&before)?),
            amount_out: q.tokens_out,
            fees: fee_lines(
                q.platform_fee,
                q.creator_fee,
                q.net_curve_in - q.effective_in,
            ),
            price_impact_bps: price_impact_bps(spot_num, spot_den, q.net_curve_in, q.tokens_out),
            continuing_output: continuing_output(&q.after)?,
            seed_ex_units: SEED_BUY,
            remainder_in: q.remainder,
        })
    }

    fn output_placement(&self) -> OutputPlacement {
        OutputPlacement::First
    }

    /// Proven on mainnet — see [`SharesTransaction::No`].
    fn shares_transaction(&self) -> SharesTransaction {
        SharesTransaction::No
    }

    fn stage_input(
        &self,
        builder: TxBuilder,
        state: &LegState,
        quote: &LegQuote,
    ) -> Result<TxBuilder, TxBuildError> {
        stage(builder, state, quote, REDEEMER_BUY, quote.amount_out)
    }
}

impl RouteLeg for LumpPadSellLeg {
    fn asset_in(&self, state: &LegState) -> Result<RouteAsset, RouteError> {
        Ok(RouteAsset::Token(token_asset(&PoolDatum::from_cbor(
            &state.datum_cbor,
        )?)?))
    }

    fn asset_out(&self, _state: &LegState) -> Result<RouteAsset, RouteError> {
        Ok(RouteAsset::Token(lump_asset()))
    }

    fn quote(&self, state: &LegState, amount_in: u64) -> Result<LegQuote, RouteError> {
        let before = PoolDatum::from_cbor(&state.datum_cbor)?;
        let q = quote_sell(&before, amount_in)?;
        let (spot_num, spot_den) = spot_price(&before)?;

        Ok(LegQuote {
            asset_in: RouteAsset::Token(token_asset(&before)?),
            amount_in,
            asset_out: RouteAsset::Token(lump_asset()),
            amount_out: q.net_out,
            fees: fee_lines(
                q.platform_fee,
                q.creator_fee,
                // The swap factor is taken off the TOKENS going in, so the
                // LUMP it keeps in the pool is the difference the curve would
                // otherwise have released.
                q.tokens_in - q.effective_in,
            ),
            // Sell trades tokens for LUMP, so the "ideal" comparison runs the
            // other way: at spot, `tokens_in` tokens are worth
            // `tokens_in * spot_num / spot_den` LUMP.
            price_impact_bps: price_impact_bps(spot_den, spot_num, q.tokens_in, q.net_out),
            continuing_output: continuing_output(&q.after)?,
            seed_ex_units: SEED_SELL,
            remainder_in: 0,
        })
    }

    fn output_placement(&self) -> OutputPlacement {
        OutputPlacement::First
    }

    /// Proven on mainnet — see [`SharesTransaction::No`].
    fn shares_transaction(&self) -> SharesTransaction {
        SharesTransaction::No
    }

    fn stage_input(
        &self,
        builder: TxBuilder,
        state: &LegState,
        quote: &LegQuote,
    ) -> Result<TxBuilder, TxBuildError> {
        stage(builder, state, quote, REDEEMER_SELL, quote.amount_out)
    }
}

/// Stage the script spend. The pool's continuing output is placed by the
/// route — see [`OutputPlacement`].
///
/// `min_out` is the EXACT quote. LumpPad's own UI sets 99 % of it; a slippage
/// tolerance buys nothing here because naming the pool UTxO already pins the
/// state — if the pool moved, the transaction fails at phase 1, for free.
fn stage(
    builder: TxBuilder,
    state: &LegState,
    quote: &LegQuote,
    redeemer_constructor: u32,
    min_out: u64,
) -> Result<TxBuilder, TxBuildError> {
    let redeemer = encode_plutus_data(&constr(
        redeemer_constructor,
        vec![int(i64::try_from(min_out).map_err(|_| {
            TxBuildError::BuildFailed(format!("{min_out} exceeds a Plutus Int"))
        })?)],
    ))?;

    builder.spend_script_utxo(
        &state.contract_utxo,
        crate::builder::script::ScriptInput {
            script: state.script.clone(),
            // Inline on the UTxO — no datum witness.
            datum_cbor: None,
            redeemer: RedeemerSource::Fixed(redeemer),
            ex_units: quote.seed_ex_units.clone(),
        },
    )
}

/// Where a Claim's creator share goes.
///
/// The datum stores `creator_cred` as 28 bytes and does NOT say whether they
/// are a key hash or a script hash, so an address cannot be derived from it
/// alone. Rather than guess — a wrong header byte pays a real address that is
/// not the creator's — this enum makes the caller state it, with the one case
/// we can determine ourselves resolved automatically.
#[derive(Debug, Clone)]
pub enum CreatorPayout {
    /// `creator_cred` IS the burn credential, so the creator's share merges
    /// into the burn output. Observed on AGENT #1, where the two burn-bound
    /// amounts came out as a single output.
    BurnsWithPlatform,
    /// The caller resolved the credential to an address.
    To(Address),
}

impl CreatorPayout {
    /// Resolve the only case the datum determines on its own.
    pub fn from_datum(datum: &PoolDatum) -> Option<Self> {
        (hex::encode(&datum.creator_cred) == address_registry::dex::LUMPPAD.burn_credential)
            .then_some(CreatorPayout::BurnsWithPlatform)
    }
}

/// Stage a permissionless Claim: empty both fee buckets into their payouts and
/// recreate the pool with `A = B = 0`, `R` and `T` untouched.
///
/// Not a [`RouteLeg`]. A claim consumes nothing from the user and produces
/// nothing to them — `amount_in` has no meaning for it, so making it a hop
/// would mean inventing one. It is a transaction shape of its own (and a cart
/// action), built onto the same `TxBuilder` as everything else.
///
/// Every observed claim used 1.5 ADA per payout, funded by the claimer.
/// Whether the validator demands exactly that or merely min-UTxO is
/// unverified (`LUMPPAD_INTEGRATION.md` §8.1 item 4), so we copy what worked.
pub fn stage_claim(
    builder: TxBuilder,
    state: &LegState,
    creator: Option<CreatorPayout>,
) -> Result<TxBuilder, RouteError> {
    let before = PoolDatum::from_cbor(&state.datum_cbor)?;
    let claim = super::quote::claim_split(&before)?;
    let deployment = address_registry::dex::LUMPPAD;

    let creator = creator
        .or_else(|| CreatorPayout::from_datum(&before))
        .ok_or_else(|| {
            RouteError::Registry(format!(
                "creator_cred {} is not the burn credential, and the datum does not say \
                 whether it is a key hash or a script hash — supply the creator's address",
                hex::encode(&before.creator_cred)
            ))
        })?;

    let redeemer = encode_plutus_data(&constr(REDEEMER_CLAIM, vec![]))?;
    let builder = builder.spend_script_utxo(
        &state.contract_utxo,
        crate::builder::script::ScriptInput {
            script: state.script.clone(),
            datum_cbor: None,
            redeemer: RedeemerSource::Fixed(redeemer),
            ex_units: SEED_CLAIM,
        },
    )?;

    // The pool's continuing output stays FIRST, as in every observed tx.
    let mut builder = builder.output(continuing_output(&claim.after)?.to_output()?);

    let (burn_amount, creator_output) = match &creator {
        CreatorPayout::BurnsWithPlatform => (claim.to_burn + claim.to_creator, None),
        CreatorPayout::To(address) => (claim.to_burn, Some(address.clone())),
    };

    let mut payouts: Vec<(Address, u64)> = vec![
        (
            crate::route::parse_address(address_registry::dex::LUMPPAD_BURN_ADDRESS)?,
            burn_amount,
        ),
        (
            crate::route::parse_address(address_registry::dex::LUMPPAD_TREASURY_ADDRESS)?,
            claim.to_treasury,
        ),
    ];
    if let Some(address) = creator_output {
        payouts.push((address, claim.to_creator));
    }

    for (address, amount) in payouts {
        if amount == 0 {
            continue;
        }
        builder = builder.output(
            OutputSpec {
                address,
                lovelace: deployment.claim_payout_lovelace,
                assets: vec![(
                    deployment.lump.policy_id.to_string(),
                    deployment.lump.asset_name_hex.to_string(),
                    amount,
                )],
                inline_datum: None,
            }
            .to_output()?,
        );
    }

    Ok(builder)
}
