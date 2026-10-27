//! `HolderBoard` — holdings ranked, as a holders list shows them: rank, a chip
//! for what is not a person, the amount and the share, with the share as a bar
//! behind the row.
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

/// One holding. `amount` is in display units; the host has applied decimals.
#[derive(Clone, Copy, Debug)]
pub struct Holding<'a> {
    /// Stable across pages: the holder's key, or the pools'.
    pub key: &'a str,
    pub label: &'a str,
    pub kind: HoldingKind,
    pub amount: f64,
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
    /// The largest holding's amount, for the share bar's width.
    pub largest: f64,
    /// Every holding's amount together, for the share.
    pub total: f64,
    /// Whether the client holds every holder or only the largest.
    pub complete: bool,
    pub ink: MarketInk,
    /// Ink per kind, where the host's palette differs from the shared seven.
    /// A kind not listed uses [`holding_colour`].
    pub kinds: &'a [KindInk],
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
    for row in vm.rows {
        let y = rows_y + row.at * ROW_H;
        // Off the list either way: past its foot, or scrolled up out of it
        // and still easing away.
        if y + ROW_H > area.bottom() + 0.5 || y < rows_y - 0.5 {
            continue;
        }
        let rect = Rect::new(area.x, y, area.w, ROW_H - 2.0);
        let tone = holding_colour_with(vm.ink, &p.theme, row.holding.kind, vm.kinds);
        // The share as a bar behind the row, against the largest.
        let share = (row.holding.amount / largest) as f32;
        let w = (rect.w - 4.0) * share.clamp(0.0, 1.0);
        draw_rounded_rect(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            4.0,
            with_alpha(p.theme.color.bg_secondary, 0.75),
        );
        draw_rounded_rect(
            rect.x,
            rect.y,
            w.max(2.0),
            rect.h,
            4.0,
            with_alpha(tone, 0.12),
        );
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
        p.text_in(
            &format::compact(row.holding.amount),
            mid(col_amt - 80.0, 80.0),
            s,
            p.theme.color.text_primary,
            crate::Align::End,
        );
        p.text_in(
            &format::share(row.holding.amount / total),
            mid(col_pct - 50.0, 50.0),
            s,
            p.theme.color.accent,
            crate::Align::End,
        );
    }
    // Where the list is, when it runs past the screen, and whether the
    // client holds every holder or only the largest.
    if vm.count > vm.fits || !vm.complete {
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
