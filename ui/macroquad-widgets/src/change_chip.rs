//! `ChangeChip` — a price's change as an arrow and a signed percentage, with
//! the stretch it is measured over under it.
//!
//! The chip is right-aligned in its rect so it can sit opposite a price; the
//! "since" line is end-aligned across the whole rect. What the change is over
//! is the host's to decide (`in the last 24h`, `since launch`); the widget only
//! draws the number and the words.

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::format::percent;
use crate::market_ink::MarketInk;
use crate::marks::arrow;
use crate::painter::{Painter, draw_rounded_rect};
use crate::theme::with_alpha;

/// What the chip draws.
#[derive(Clone, Copy, Debug)]
pub struct ChangeChipVm<'a> {
    /// The change as a fraction: `0.03` is `+3.0%`.
    pub change: f64,
    /// What the change is over, in words.
    pub since: &'a str,
    pub ink: MarketInk,
}

/// Draw the chip in `rect`'s top-right and the "since" line under it. Returns
/// the chip's own rect, for a host that wants to hit-test it.
pub fn change_chip(p: &Painter, vm: &ChangeChipVm, rect: Rect) -> Rect {
    // The same threshold `format::percent` rounds to zero at.
    let up = vm.change > -0.000_5;
    let tone = if up { vm.ink.buy } else { vm.ink.sell };
    let label = percent(vm.change);
    let ls = p.size(TextSize::Lg);
    let lw = p.measure(&label, ls).width;
    let chip = Rect::new(
        (rect.right() - lw - 30.0).max(rect.x),
        rect.y,
        lw + 30.0,
        26.0,
    );
    draw_rounded_rect(chip.x, chip.y, chip.w, chip.h, 8.0, with_alpha(tone, 0.16));
    arrow(vec2(chip.x + 12.0, chip.center().y), 9.0, up, tone);
    let words = Rect::new(chip.x + 22.0, chip.y, chip.w - 28.0, chip.h);
    p.text_in(&label, words, ls, tone, crate::Align::Start);
    let under = Rect::new(rect.x, rect.y + 28.0, rect.w, 12.0);
    p.text_in(
        vm.since,
        under,
        p.size(TextSize::Xs),
        p.theme.color.text_muted,
        crate::Align::End,
    );
    chip
}
