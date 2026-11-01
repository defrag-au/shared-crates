//! `HolderBoard` — holdings ranked, as a holders list shows them: rank, a chip
//! for what is not a person, the amount and the share, with the share as a bar
//! behind the row.
//!
//! The bar is the party's POSITION — free + locked + what a pool adds beside the
//! balance ([`Holding::position`]) — as a share of the table's total position.
//! That is what the ranking is, and a share of the raw balance would contradict
//! it: a wallet whose whole involvement is liquidity would be ranked high and
//! read `0.0%`. A locked part is a length inside the bar, in the Lock ink; the
//! pool side is a second, thinner length along the row's foot in the pool's ink.
//!
//! ⚠️ A pool's reserve is counted here TWICE — once at the pool's own row, which
//! holds it, and once at each provider whose share of it is attributed — so the
//! total position sums above the supply. That is deliberate: the column answers
//! "how much of this token does this party command", and the reserve is
//! commanded by whoever funded it. [`HolderBoardVm::total`] is that position
//! total, not the supply.
//!
//! The foot band is scaled to the largest pool position in the whole table
//! ([`HolderBoardVm::largest_pool`]), so the biggest provider spans its row and a
//! party with no balance at all still carries a band. A row with no units says
//! `—` rather than `0` for its amount and share: it holds none of the supply,
//! and `0.0%` beside a real pool position reads as a broken figure rather than
//! as an absence.
//!
//! A row is `at` a position the host eased, so a rank change slides rather than
//! jumps; the host keeps the scroll and the easing. The footer says where the
//! list is, and whether the client holds every holder or only the largest.

use macroquad::prelude::*;
use ui_theme::{TextSize, mix};

use crate::format;
use crate::market_ink::MarketInk;
use crate::painter::{Painter, draw_rounded_rect};
use crate::theme::with_alpha;

/// What a holding is. Only a person is ranked or tiered; the rest are shown for
/// what they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HoldingKind {
    /// A wallet, as the nautical tier it reaches (0 shrimp … 6 kraken).
    Person(u8),
    /// Tokens no holder has: a pool's, or a curve's side.
    Pool,
    Burn,
    Lock,
    Staking,
    Venue,
    Contract,
}

/// What of a holding is locked, and what sits beside it in a pool.
///
/// Only a host with TIP facts and a replay at the tip passes one: the split
/// describes now, and a rewound balance is a different moment. A holding with
/// neither a lock nor an LP share has no split at all.
///
/// ⚠️ `vested` is INSIDE [`Holding::amount`] and `lp` is BESIDE it — the same
/// distinction the archive draws, and getting them the wrong way round either
/// double-counts the position or wraps `free` past zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoldingSplit {
    /// Locked units, in display units. `free = amount − vested`.
    pub vested: f64,
    /// Units of an ADA pool's reserve this holder's LP share accounts for, in
    /// display units. Not part of `amount`: the reserve is the pool's row.
    pub lp: f64,
}

/// One holding. `amount` is in display units; the host has applied decimals.
#[derive(Clone, Copy, Debug)]
pub struct Holding<'a> {
    /// Stable across pages: the holder's key, or the pools'.
    pub key: &'a str,
    pub label: &'a str,
    pub kind: HoldingKind,
    pub amount: f64,
    /// What of `amount` is locked and what a pool adds, where the host knows.
    pub split: Option<HoldingSplit>,
}

impl Holding<'_> {
    /// Units free to move: the amount less anything locked.
    pub fn free(&self) -> f64 {
        match self.split {
            Some(s) => (self.amount - s.vested).max(0.0),
            None => self.amount,
        }
    }

    /// Units this holder's LP share is worth, beside its own balance.
    pub fn lp(&self) -> f64 {
        self.split.map_or(0.0, |s| s.lp)
    }

    /// What the party commands: `free + locked + lp`.
    ///
    /// The three parts added up once, which is what the ranking, the bar and the
    /// map's area are all of. `locked` is inside `amount` and `lp` is beside it,
    /// so this is `amount + lp`. Use it rather than `amount` anywhere a reader
    /// is comparing parties, or a provider's pool side silently disappears.
    pub fn position(&self) -> f64 {
        self.amount + self.lp()
    }
}

/// A holding's colour: people by tier, brighter the bigger; each role its own.
pub fn holding_colour(ink: MarketInk, theme: &crate::Theme, kind: HoldingKind) -> Color {
    match kind {
        HoldingKind::Person(tier) => {
            let t = (tier as f32 / 6.0).clamp(0.0, 1.0);
            theme.series.ordinal.at(t)
        }
        HoldingKind::Pool => ink.pool,
        HoldingKind::Burn => ink.sell,
        HoldingKind::Lock => ink.ada,
        HoldingKind::Staking => theme.color.success,
        HoldingKind::Venue => ink.transfer,
        HoldingKind::Contract => theme.color.text_muted,
    }
}

/// One kind's ink where a host's palette differs from the shared seven.
///
/// The seven fields of [`MarketInk`] cover what a market view encodes
/// *semantically* — a buy, a sell, a pool. A host whose map and feed already
/// had a role palette of their own (a burned token's orange-red, say) names the
/// difference here rather than bending the seven, so both views of one token
/// keep agreeing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KindInk {
    pub kind: HoldingKind,
    pub ink: Color,
}

/// A holding's colour: `kinds` first, then [`holding_colour`].
pub fn holding_colour_with(
    ink: MarketInk,
    theme: &crate::Theme,
    kind: HoldingKind,
    kinds: &[KindInk],
) -> Color {
    kinds
        .iter()
        .find(|k| k.kind == kind)
        .map_or_else(|| holding_colour(ink, theme, kind), |k| k.ink)
}

/// A short chip for a holding that is not a person.
pub fn holding_chip(kind: HoldingKind) -> Option<&'static str> {
    match kind {
        HoldingKind::Person(_) => None,
        HoldingKind::Pool => Some("POOL"),
        HoldingKind::Burn => Some("BURN"),
        HoldingKind::Lock => Some("LOCK"),
        HoldingKind::Staking => Some("STAKE"),
        HoldingKind::Venue => Some("VENUE"),
        HoldingKind::Contract => Some("SCRIPT"),
    }
}

/// One row, placed by the host.
#[derive(Clone, Copy, Debug)]
pub struct BoardRow<'a> {
    pub holding: &'a Holding<'a>,
    /// 1-based rank in the whole list.
    pub rank: usize,
    /// Row position from the top of the visible list, eased by the host. `0`
    /// is the first visible row; a leaving row eases out below.
    pub at: f32,
    /// A trade's colour and strength, over the row.
    pub flash: Option<(Color, f32)>,
}

/// What the board draws.
#[derive(Clone, Copy, Debug)]
pub struct HolderBoardVm<'a> {
    pub rows: &'a [BoardRow<'a>],
    /// The rank of the first visible row, 0-based, for the footer.
    pub top: usize,
    /// How many rows fit, for the footer.
    pub fits: usize,
    /// Every holding the client has.
    pub count: usize,
    /// The largest POSITION in the table, for the share bar's width. Not the
    /// largest balance: a bar is what the party commands.
    pub largest: f64,
    /// The largest pool position in the table, for the foot band's length, so
    /// the biggest provider spans its row. `0.0` when no holding has one, and
    /// no band is drawn.
    pub largest_pool: f64,
    /// Every holding's POSITION together, for the share: above the supply
    /// wherever a pool's reserve is attributed to its providers as well as
    /// held by the pool.
    pub total: f64,
    /// Whether the client holds every holder or only the largest.
    pub complete: bool,
    pub ink: MarketInk,
    /// Ink per kind, where the host's palette differs from the shared seven.
    /// A kind not listed uses [`holding_colour`].
    pub kinds: &'a [KindInk],
    /// The holding the reader picked, by [`Holding::key`]. Its row gets the
    /// raised plate and an accent rule at the start — the cue that the detail
    /// shown beside the board belongs to THIS row rather than to the top one.
    pub selected: Option<&'a str>,
}

/// The board's row height.
const ROW_H: f32 = 22.0;

/// Draw the column heads and the rows in `area`, with the footer where the
/// list runs past it.
pub fn holder_board(p: &Painter, vm: &HolderBoardVm, area: Rect) {
    let s = p.size(TextSize::Xs);
    let total = vm.total.max(1.0);
    let largest = vm.largest.max(1.0);
    // Columns: rank, holder, amount, share.
    let col_rank = area.x + 6.0;
    let col_name = area.x + 34.0;
    let col_pct = area.right() - 6.0;
    let col_amt = area.right() - 62.0;
    let heads = Rect::new(area.x, area.y, area.w, 14.0);
    for (label, r, align) in [
        (
            "#",
            Rect::new(col_rank, heads.y, 24.0, 14.0),
            crate::Align::Start,
        ),
        (
            "HOLDER",
            Rect::new(col_name, heads.y, 120.0, 14.0),
            crate::Align::Start,
        ),
        (
            "AMOUNT",
            Rect::new(col_amt - 80.0, heads.y, 80.0, 14.0),
            crate::Align::End,
        ),
        (
            "SHARE",
            Rect::new(col_pct - 50.0, heads.y, 50.0, 14.0),
            crate::Align::End,
        ),
    ] {
        p.text_in(label, r, s, p.theme.color.text_muted, align);
    }
    let rows_y = heads.bottom() + 2.0;
    // The footer is drawn in the last 12 points when the list runs past the
    // screen, so a row is not allowed into that band even if the host handed
    // it more than it said fits: the band and the range it prints must agree.
    let footer = vm.count > vm.fits || !vm.complete;
    let foot = if footer { 12.0 } else { 0.0 };
    for row in vm.rows {
        let y = rows_y + row.at * ROW_H;
        // Off the list either way: past its foot, or scrolled up out of it
        // and still easing away.
        if y + ROW_H > area.bottom() - foot + 0.5 || y < rows_y - 0.5 {
            continue;
        }
        let rect = Rect::new(area.x, y, area.w, ROW_H - 2.0);
        let tone = holding_colour_with(vm.ink, &p.theme, row.holding.kind, vm.kinds);
        let selected = vm.selected == Some(row.holding.key);
        // The share as a bar behind the row, against the largest POSITION.
        // Where the host knows the split, the locked part is drawn INSIDE that
        // length in the Lock ink: how much of a row cannot move is then a
        // length rather than another number to read.
        let position = row.holding.position();
        let share = (position / largest) as f32;
        let w = (rect.w - 4.0) * share.clamp(0.0, 1.0);
        draw_rounded_rect(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            4.0,
            with_alpha(
                match selected {
                    true => p.theme.color.bg_highlight,
                    false => p.theme.color.bg_secondary,
                },
                match selected {
                    true => 0.9,
                    false => 0.75,
                },
            ),
        );
        let locked = row
            .holding
            .split
            .map_or(0.0, |s| {
                s.vested / position.max(f64::MIN_POSITIVE)
            })
            .clamp(0.0, 1.0) as f32;
        let free_w = w * (1.0 - locked);
        if free_w > 0.5 {
            draw_rounded_rect(
                rect.x,
                rect.y,
                free_w.max(2.0),
                rect.h,
                4.0,
                with_alpha(tone, 0.12),
            );
        }
        if w - free_w > 0.5 {
            draw_rounded_rect(
                rect.x + free_w,
                rect.y,
                w - free_w,
                rect.h,
                4.0,
                with_alpha(vm.ink.ada, 0.3),
            );
        }
        // THE POOL SIDE, as its own measure along the row's foot. Not inside
        // the share bar: a pool position is BESIDE a balance, not part of it,
        // and a full-height length read as a selected row rather than as a
        // second number. Scaled to the largest POOL position rather than to the
        // largest balance, so the biggest provider's band is the full row and
        // the order of the pools is legible at a glance.
        let pool_w = match vm.largest_pool > 0.0 {
            true => ((row.holding.lp() / vm.largest_pool) as f32 * (rect.w - 4.0))
                .clamp(0.0, rect.w - 4.0),
            false => 0.0,
        };
        if pool_w > 1.0 {
            let band = (rect.h * 0.34).clamp(3.0, 5.0);
            draw_rounded_rect(
                rect.x,
                rect.y + rect.h - band,
                pool_w,
                band,
                2.0,
                with_alpha(vm.ink.pool, 0.55),
            );
        }
        if let Some((c, a)) = row.flash {
            draw_rounded_rect(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                4.0,
                with_alpha(c, 0.35 * a.clamp(0.0, 1.0)),
            );
        }
        // The selection's own mark, over the fills: the row the detail belongs
        // to, in the accent the rest of the screen uses for "this one".
        if selected {
            draw_rounded_rect(rect.x, rect.y, 2.0, rect.h, 1.0, p.theme.color.accent);
        }
        let mid = |x: f32, w: f32| Rect::new(x, rect.y, w, rect.h);
        p.text_in(
            &row.rank.to_string(),
            mid(col_rank, 26.0),
            s,
            p.theme.color.text_primary,
            crate::Align::Start,
        );
        let name_room = col_amt - 84.0 - col_name;
        let mut name_x = col_name;
        if let Some(chip) = holding_chip(row.holding.kind) {
            let cw = p.measure(chip, s).width + 8.0;
            let cr = Rect::new(name_x, rect.y + 3.0, cw, rect.h - 6.0);
            // The chip's own fill decides its ink: a light role colour (a
            // pool's accent) must not leave white text on a pale plate.
            let fill = mix(p.theme.color.bg_secondary, tone, 0.35);
            let chip_ink = ink_on(fill, p.theme.color.bg_primary, p.theme.color.text_primary);
            draw_rounded_rect(cr.x, cr.y, cr.w, cr.h, 3.0, fill);
            p.text_in(chip, cr, s, chip_ink, crate::Align::Center);
            name_x += cw + 5.0;
        }
        let name_w = (col_name + name_room - name_x).max(10.0);
        let ink = match row.holding.kind {
            HoldingKind::Person(_) => p.theme.color.text_primary,
            _ => p.theme.color.text_secondary,
        };
        p.text_in(
            &p.fit(row.holding.label, s, name_w),
            mid(name_x, name_w),
            s,
            ink,
            crate::Align::Start,
        );
        // The amount, with the units a pool adds beside it: `+` and the figure
        // in the pool's ink, so an LP position reads at a glance and is never
        // mistaken for part of the balance.
        //
        // A party with no units says `—`: it holds none of the supply, and a
        // `0` beside a real pool position reads as a figure that failed to
        // load rather than as an absence.
        let amount_box = Rect::new(col_amt - 80.0, rect.y, 80.0, rect.h);
        let lp = row.holding.lp();
        let holds = row.holding.amount > 0.0;
        let units = match holds {
            true => format::compact(row.holding.amount),
            false => "—".to_string(),
        };
        match lp > 0.0 {
            true => {
                let extra = format!("+{}", format::compact(lp));
                let ew = p.measure(&extra, s).width;
                p.text_in(
                    &extra,
                    Rect::new(amount_box.right() - ew, rect.y, ew, rect.h),
                    s,
                    vm.ink.pool,
                    crate::Align::End,
                );
                let room = (amount_box.w - ew - 5.0).max(10.0);
                p.text_in(
                    &units,
                    Rect::new(amount_box.x, rect.y, room, rect.h),
                    s,
                    p.theme.color.text_primary,
                    crate::Align::End,
                );
            }
            false => p.text_in(
                &units,
                amount_box,
                s,
                p.theme.color.text_primary,
                crate::Align::End,
            ),
        }
        let share = match holds {
            true => format::share(position / total),
            false => "—".to_string(),
        };
        p.text_in(
            &share,
            mid(col_pct - 50.0, 50.0),
            s,
            p.theme.color.accent,
            crate::Align::End,
        );
    }
    // Where the list is, when it runs past the screen, and whether the
    // client holds every holder or only the largest.
    if footer {
        let first = vm.top + 1;
        let last = (vm.top + vm.fits).min(vm.count);
        let range = match vm.count > vm.fits {
            true => format!("{first}–{last} OF {}", vm.count),
            false => format!("{} SHOWN", vm.count),
        };
        let note = match vm.complete {
            true => format!("{range} · DRAG OR SCROLL"),
            false => format!("{range} · THE LARGEST LOADED"),
        };
        let more = Rect::new(area.x, area.bottom() - 12.0, area.w, 12.0);
        p.text_in(
            &note,
            more,
            s,
            p.theme.color.text_muted,
            crate::Align::Center,
        );
    }
}

/// Dark ink on a light surface, light on a dark one.
fn ink_on(c: Color, dark: Color, light: Color) -> Color {
    match 0.299 * c.r + 0.587 * c.g + 0.114 * c.b > 0.6 {
        true => dark,
        false => light,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_people_are_unlabelled_and_ranked() {
        assert_eq!(holding_chip(HoldingKind::Person(3)), None);
        for kind in [
            HoldingKind::Pool,
            HoldingKind::Burn,
            HoldingKind::Lock,
            HoldingKind::Staking,
            HoldingKind::Venue,
            HoldingKind::Contract,
        ] {
            assert!(holding_chip(kind).is_some(), "{kind:?} needs a chip");
        }
    }
}
