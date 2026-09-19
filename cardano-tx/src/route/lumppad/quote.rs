//! LumpPad's exact arithmetic.
//!
//! Spec: `docs/design/LUMPPAD_INTEGRATION.md` §5, verified against all 31
//! trades in its Appendix A with margin 0. All integer, floor division. If a
//! formula here stops reproducing a tape row exactly, the formula is wrong —
//! the row is what the validator accepted.

use super::datum::PoolDatum;
use crate::route::leg::RouteError;

/// The virtual LUMP reserve every pool is launched with. Baked into both the
/// validator and every mint policy.
pub const VIRTUAL_RESERVE: u64 = 10_000_000;
/// The swap factor: 0.3 % of the curve leg stays in the pool.
pub const SWAP_FACTOR_NUM: u64 = 9_970;
pub const SWAP_FACTOR_DEN: u64 = 10_000;
/// Basis-point denominator for the two bucket fees.
pub const BPS_DEN: u64 = 10_000;
/// LumpPad splits the platform bucket in half on Claim; half is burned.
pub const PLATFORM_SPLIT_DEN: u64 = 2;

/// A buy: LUMP in, tokens out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuyQuote {
    /// LUMP that lands on the curve (`R_new − R`).
    pub net_curve_in: u64,
    /// `net_curve_in` after the 0.3 % swap factor — the number the constant
    /// product is evaluated at.
    pub effective_in: u64,
    /// Tokens to the buyer.
    pub tokens_out: u64,
    /// LUMP the buyer must add to the pool UTxO: curve leg plus both buckets.
    pub gross_in: u64,
    pub platform_fee: u64,
    pub creator_fee: u64,
    /// LUMP the solver could not fit under the user's gross budget, which
    /// stays with the user (0–2).
    pub remainder: u64,
    pub after: PoolDatum,
}

/// A sell: tokens in, LUMP out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SellQuote {
    pub tokens_in: u64,
    /// `tokens_in` after the 0.3 % swap factor.
    pub effective_in: u64,
    /// LUMP leaving the curve, before the buckets are charged.
    pub gross_out: u64,
    /// LUMP the seller receives. This is what the Sell redeemer's `min_out`
    /// refers to.
    pub net_out: u64,
    pub platform_fee: u64,
    pub creator_fee: u64,
    pub after: PoolDatum,
}

/// A claim: both buckets emptied to three payouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimQuote {
    /// `A/2` to the burn address.
    pub to_burn: u64,
    /// `A − A/2` to the treasury.
    pub to_treasury: u64,
    /// `B` to `creator_cred`, when non-zero.
    pub to_creator: u64,
    pub after: PoolDatum,
}

/// Spot price in LUMP per token: `(V + R) / T`, as a rational so callers keep
/// full precision. The pool datum IS the price oracle — one UTxO, no indexer.
pub fn spot_price(state: &PoolDatum) -> Result<(u64, u64), RouteError> {
    let numerator = VIRTUAL_RESERVE
        .checked_add(state.reserve)
        .ok_or(RouteError::Overflow)?;
    Ok((numerator, state.tokens))
}

/// Solve the largest curve amount `n` a user-typed GROSS budget `g` supports.
///
/// LumpPad's own solver is the closed form, floored — NOT "the largest `n`
/// that fits". Those differ: for `g = 1,000,000` on a fresh pool the closed
/// form gives `n = 975,369` (gross 999,998, remainder 2) while the largest
/// fitting `n` is 975,371 (gross exactly 1,000,000). Every SWOLE and AGENT buy
/// on chain shows 999,998 for a typed 1,000,000, so the closed form is the
/// specification and stepping UP would diverge from the venue.
///
/// The step-down loop below can never fire for the current fee schedule — the
/// closed form provably under-shoots — but it is kept as a guard so a future
/// fee change degrades to "quote slightly less" rather than "build an invalid
/// transaction".
pub fn solve_net_from_gross(state: &PoolDatum, gross: u64) -> Result<(u64, u64), RouteError> {
    if gross <= state.flat_fee {
        return Err(RouteError::FlatFeeExceedsInput {
            amount: gross,
            flat_fee: state.flat_fee,
        });
    }
    let budget = gross - state.flat_fee;
    // n ≈ budget * BPS_DEN / (BPS_DEN + platform_bps + creator_bps)
    let divisor = BPS_DEN
        .checked_add(state.platform_fee_bps)
        .and_then(|v| v.checked_add(state.creator_fee_bps))
        .ok_or(RouteError::Overflow)?;
    let mut n = (budget as u128 * BPS_DEN as u128 / divisor as u128) as u64;

    while n > 0 && gross_for_net(state, n)? > gross {
        n -= 1;
    }
    if n == 0 {
        return Err(RouteError::FlatFeeExceedsInput {
            amount: gross,
            flat_fee: state.flat_fee,
        });
    }
    let remainder = gross - gross_for_net(state, n)?;
    Ok((n, remainder))
}

/// The gross LUMP a curve amount `n` costs: the curve leg plus both buckets.
fn gross_for_net(state: &PoolDatum, n: u64) -> Result<u64, RouteError> {
    let platform = bucket_fee(n, state.platform_fee_bps)?
        .checked_add(state.flat_fee)
        .ok_or(RouteError::Overflow)?;
    let creator = bucket_fee(n, state.creator_fee_bps)?;
    n.checked_add(platform)
        .and_then(|v| v.checked_add(creator))
        .ok_or(RouteError::Overflow)
}

/// `amount * bps / 10_000`, floored.
fn bucket_fee(amount: u64, bps: u64) -> Result<u64, RouteError> {
    Ok((amount as u128 * bps as u128 / BPS_DEN as u128) as u64)
}

/// `amount * 9970 / 10000`, floored.
fn apply_swap_factor(amount: u64) -> u64 {
    (amount as u128 * SWAP_FACTOR_NUM as u128 / SWAP_FACTOR_DEN as u128) as u64
}

/// Quote a buy from the NET curve amount — the form the golden tape records
/// (`n = R_after − R_before`).
///
/// The 0.3 % is NOT moved to a bucket: `R_new = R + n` in full, so the pool's
/// `k` grows by the fee. A naive single-virtual fit therefore "drifts".
pub fn quote_buy_from_net(state: &PoolDatum, net_curve_in: u64) -> Result<BuyQuote, RouteError> {
    if net_curve_in == 0 {
        return Err(RouteError::ZeroOutput);
    }
    let effective_in = apply_swap_factor(net_curve_in);
    let denominator = (VIRTUAL_RESERVE as u128)
        .checked_add(state.reserve as u128)
        .and_then(|v| v.checked_add(effective_in as u128))
        .ok_or(RouteError::Overflow)?;
    if denominator == 0 {
        return Err(RouteError::Overflow);
    }
    let tokens_out = (state.tokens as u128 * effective_in as u128 / denominator) as u64;
    if tokens_out == 0 {
        return Err(RouteError::ZeroOutput);
    }
    if tokens_out > state.tokens {
        return Err(RouteError::InsufficientPoolReserve {
            needed: tokens_out,
            available: state.tokens,
        });
    }

    let platform_fee = bucket_fee(net_curve_in, state.platform_fee_bps)?
        .checked_add(state.flat_fee)
        .ok_or(RouteError::Overflow)?;
    let creator_fee = bucket_fee(net_curve_in, state.creator_fee_bps)?;
    let gross_in = gross_for_net(state, net_curve_in)?;

    let after = PoolDatum {
        reserve: state
            .reserve
            .checked_add(net_curve_in)
            .ok_or(RouteError::Overflow)?,
        tokens: state.tokens - tokens_out,
        platform_bucket: state
            .platform_bucket
            .checked_add(platform_fee)
            .ok_or(RouteError::Overflow)?,
        creator_bucket: state
            .creator_bucket
            .checked_add(creator_fee)
            .ok_or(RouteError::Overflow)?,
        ..state.clone()
    };

    Ok(BuyQuote {
        net_curve_in,
        effective_in,
        tokens_out,
        gross_in,
        platform_fee,
        creator_fee,
        remainder: 0,
        after,
    })
}

/// Quote a buy from a user-typed GROSS LUMP amount — the UI's natural input.
pub fn quote_buy(state: &PoolDatum, gross_lump: u64) -> Result<BuyQuote, RouteError> {
    let (net, remainder) = solve_net_from_gross(state, gross_lump)?;
    let mut quote = quote_buy_from_net(state, net)?;
    quote.remainder = remainder;
    Ok(quote)
}

/// Quote a sell of `tokens_in` tokens.
///
/// The buckets are charged on the GROSS leaving the curve, playing the same
/// role `n` plays in a buy. A sell whose gross cannot cover the flat fee has a
/// non-positive net — LumpPad's UI calls that "Not sellable at this size", and
/// so do we, rather than returning a negative.
pub fn quote_sell(state: &PoolDatum, tokens_in: u64) -> Result<SellQuote, RouteError> {
    if tokens_in == 0 {
        return Err(RouteError::ZeroOutput);
    }
    let effective_in = apply_swap_factor(tokens_in);
    let denominator = (state.tokens as u128)
        .checked_add(effective_in as u128)
        .ok_or(RouteError::Overflow)?;
    if denominator == 0 {
        return Err(RouteError::Overflow);
    }
    let numerator = (VIRTUAL_RESERVE as u128)
        .checked_add(state.reserve as u128)
        .ok_or(RouteError::Overflow)?
        .checked_mul(effective_in as u128)
        .ok_or(RouteError::Overflow)?;
    let gross_out = (numerator / denominator) as u64;
    if gross_out > state.reserve {
        return Err(RouteError::InsufficientPoolReserve {
            needed: gross_out,
            available: state.reserve,
        });
    }

    let platform_fee = bucket_fee(gross_out, state.platform_fee_bps)?
        .checked_add(state.flat_fee)
        .ok_or(RouteError::Overflow)?;
    let creator_fee = bucket_fee(gross_out, state.creator_fee_bps)?;
    let charged = platform_fee
        .checked_add(creator_fee)
        .ok_or(RouteError::Overflow)?;
    if gross_out <= charged {
        return Err(RouteError::NotSellable);
    }
    let net_out = gross_out - charged;

    let after = PoolDatum {
        reserve: state.reserve - gross_out,
        tokens: state
            .tokens
            .checked_add(tokens_in)
            .ok_or(RouteError::Overflow)?,
        platform_bucket: state
            .platform_bucket
            .checked_add(platform_fee)
            .ok_or(RouteError::Overflow)?,
        creator_bucket: state
            .creator_bucket
            .checked_add(creator_fee)
            .ok_or(RouteError::Overflow)?,
        ..state.clone()
    };

    Ok(SellQuote {
        tokens_in,
        effective_in,
        gross_out,
        net_out,
        platform_fee,
        creator_fee,
        after,
    })
}

/// Split both fee buckets for a Claim. Permissionless; `R` and `T` unchanged.
pub fn claim_split(state: &PoolDatum) -> Result<ClaimQuote, RouteError> {
    if state.platform_bucket == 0 && state.creator_bucket == 0 {
        return Err(RouteError::NothingToClaim);
    }
    let to_burn = state.platform_bucket / PLATFORM_SPLIT_DEN;
    let to_treasury = state.platform_bucket - to_burn;
    Ok(ClaimQuote {
        to_burn,
        to_treasury,
        to_creator: state.creator_bucket,
        after: PoolDatum {
            platform_bucket: 0,
            creator_bucket: 0,
            ..state.clone()
        },
    })
}

/// Price impact in basis points: how far the average price a trade pays sits
/// above (buy) or below (sell) the spot price it started from.
pub fn price_impact_bps(spot_num: u64, spot_den: u64, amount_in: u64, amount_out: u64) -> u32 {
    if amount_out == 0 || spot_den == 0 || amount_in == 0 {
        return 0;
    }
    // spot = spot_num/spot_den LUMP per token. At spot, amount_in LUMP would
    // buy amount_in * spot_den / spot_num tokens.
    let ideal_out = amount_in as u128 * spot_den as u128 / spot_num as u128;
    if ideal_out <= amount_out as u128 {
        return 0;
    }
    (((ideal_out - amount_out as u128) * 10_000) / ideal_out) as u32
}
