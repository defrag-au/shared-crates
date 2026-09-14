//! Splash royalty-pool swap arithmetic.
//!
//! Read from `WhalePoolsDex/PContracts/PRoyaltyPool.hs` and checked against
//! every swap in [`super::golden`]. All integer, floor division.
//!
//! The validator states its rule as an INEQUALITY:
//!
//! ```text
//! dy * (rx0 * feeDen + dx * f)  <=  ry0 * dx * f
//! ```
//!
//! so it accepts a swap that takes LESS than the maximum. We always take the
//! maximum — there is no reason to tip the pool, and no slippage tolerance to
//! spend, because naming the pool UTxO already pins the state.

use super::datum::{CounterSide, RoyaltyPoolDatum};
use crate::route::leg::RouteError;

/// Which way value moves through the pool, against the DATUM's `poolX` /
/// `poolY` — never against a ticker, which is not a thing the validator knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplashDirection {
    /// Pay X, receive Y.
    XToY,
    /// Pay Y, receive X.
    YToX,
}

impl SplashDirection {
    /// The side a swap's counters are bumped on: the side the input arrives
    /// on. The opposite side's counters are left byte-identical.
    pub fn counter_side(self) -> CounterSide {
        match self {
            SplashDirection::XToY => CounterSide::X,
            SplashDirection::YToX => CounterSide::Y,
        }
    }

    pub fn reversed(self) -> Self {
        match self {
            SplashDirection::XToY => SplashDirection::YToX,
            SplashDirection::YToX => SplashDirection::XToY,
        }
    }
}

/// The pool's reserves as they sit in the UTxO, before the accrued treasury
/// and royalty are taken out of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolReserves {
    /// `poolX` quantity held by the UTxO (lovelace, when X is ADA).
    pub x: u64,
    /// `poolY` quantity held by the UTxO.
    pub y: u64,
}

/// A quoted swap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapQuote {
    pub direction: SplashDirection,
    pub amount_in: u64,
    /// The maximum the validator will release — see the module note.
    pub amount_out: u64,
    pub treasury_fee: u64,
    pub royalty_fee: u64,
    /// The pool's liquidity-provider fee: what the swap leaves behind beyond
    /// the two named counters. `feeNum` is quoted against `feeDen`, so the LP
    /// share is `feeDen − feeNum` of the input.
    pub lp_fee: u64,
    /// Effective reserves the constant product was evaluated at.
    pub effective_in_reserve: u64,
    pub effective_out_reserve: u64,
    pub after: RoyaltyPoolDatum,
}

/// Effective reserves: what the UTxO holds, less what is already owed to the
/// treasury and the royalty recipient. Those accrue INSIDE the pool UTxO, so
/// the raw balance is not the curve.
pub fn effective_reserves(
    datum: &RoyaltyPoolDatum,
    reserves: PoolReserves,
) -> Result<(u64, u64), RouteError> {
    let (treasury_x, royalty_x) = (datum.treasury_x()?, datum.royalty_x()?);
    let (treasury_y, royalty_y) = (datum.treasury_y()?, datum.royalty_y()?);
    let rx0 = reserves
        .x
        .checked_sub(treasury_x)
        .and_then(|v| v.checked_sub(royalty_x))
        .ok_or_else(|| {
            RouteError::Datum(
                "pool holds less X than its datum says is owed to treasury + royalty".to_string(),
            )
        })?;
    let ry0 = reserves
        .y
        .checked_sub(treasury_y)
        .and_then(|v| v.checked_sub(royalty_y))
        .ok_or_else(|| {
            RouteError::Datum(
                "pool holds less Y than its datum says is owed to treasury + royalty".to_string(),
            )
        })?;
    Ok((rx0, ry0))
}

/// Quote a swap of `amount_in` in `direction`.
pub fn quote_swap(
    datum: &RoyaltyPoolDatum,
    reserves: PoolReserves,
    direction: SplashDirection,
    amount_in: u64,
) -> Result<SwapQuote, RouteError> {
    if amount_in == 0 {
        return Err(RouteError::ZeroOutput);
    }
    let fee_den = address_registry::dex::SPLASH_ROYALTY_POOL.fee_den;
    let f = datum.swap_fee_num()?;
    let (rx0, ry0) = effective_reserves(datum, reserves)?;

    let (reserve_in, reserve_out) = match direction {
        SplashDirection::XToY => (rx0, ry0),
        SplashDirection::YToX => (ry0, rx0),
    };

    // out = reserve_out * in * f / (reserve_in * feeDen + in * f)
    let weighted_in = (amount_in as u128)
        .checked_mul(f as u128)
        .ok_or(RouteError::Overflow)?;
    let denominator = (reserve_in as u128)
        .checked_mul(fee_den as u128)
        .and_then(|v| v.checked_add(weighted_in))
        .ok_or(RouteError::Overflow)?;
    if denominator == 0 {
        return Err(RouteError::Overflow);
    }
    let numerator = (reserve_out as u128)
        .checked_mul(weighted_in)
        .ok_or(RouteError::Overflow)?;
    let amount_out = u64::try_from(numerator / denominator).map_err(|_| RouteError::Overflow)?;

    if amount_out == 0 {
        return Err(RouteError::ZeroOutput);
    }
    if amount_out > reserve_out {
        return Err(RouteError::InsufficientPoolReserve {
            needed: amount_out,
            available: reserve_out,
        });
    }

    let treasury_fee = fee_share(amount_in, datum.treasury_fee()?, fee_den)?;
    let royalty_fee = fee_share(amount_in, datum.royalty_fee()?, fee_den)?;
    let lp_fee = fee_share(amount_in, fee_den.saturating_sub(datum.fee_num()?), fee_den)?;

    let after = datum.with_counters_bumped(direction.counter_side(), treasury_fee, royalty_fee)?;

    Ok(SwapQuote {
        direction,
        amount_in,
        amount_out,
        treasury_fee,
        royalty_fee,
        lp_fee,
        effective_in_reserve: reserve_in,
        effective_out_reserve: reserve_out,
        after,
    })
}

/// `amount * rate / feeDen`, floored — the validator's own expression, and
/// the exact deltas the counters take.
fn fee_share(amount: u64, rate: u64, fee_den: u64) -> Result<u64, RouteError> {
    if fee_den == 0 {
        return Err(RouteError::Overflow);
    }
    u64::try_from(amount as u128 * rate as u128 / fee_den as u128).map_err(|_| RouteError::Overflow)
}

/// Price impact in basis points against the pool's marginal price.
pub fn price_impact_bps(reserve_in: u64, reserve_out: u64, amount_in: u64, amount_out: u64) -> u32 {
    if reserve_in == 0 || amount_in == 0 || amount_out == 0 {
        return 0;
    }
    // At the marginal price, `amount_in` would return
    // `amount_in * reserve_out / reserve_in`.
    let ideal = amount_in as u128 * reserve_out as u128 / reserve_in as u128;
    if ideal <= amount_out as u128 {
        return 0;
    }
    u32::try_from((ideal - amount_out as u128) * 10_000 / ideal).unwrap_or(u32::MAX)
}
