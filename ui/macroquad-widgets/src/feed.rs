//! `Feed` — a stack of activity rows, newest on top: a chip for what happened,
//! who or what it was, the amount and the time, and the newest row sliding in.
//!
//! The visual half of a ledger feed. The host owns the deque and the beat
//! indices; this draws a slice of rows, newest first, with the arrival
//! animation, and a pill when the reader has scrolled back. A row is the same
//! whether it is a trade, a mint or a transfer — the chip and the tone carry
//! that, and the host supplies both.

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::marks::pill_label;
use crate::painter::{Line, Painter, draw_rounded_rect};
use crate::theme::with_alpha;

/// A feed row's height.
pub const FEED_ROW_H: f32 = 34.0;
/// The gap under a row.
const FEED_ROW_GAP: f32 = 3.0;
/// One row and its gap: what a scroll steps by.
pub const FEED_ROW_STEP: f32 = FEED_ROW_H + FEED_ROW_GAP;

/// One row, already projected by the host.
#[derive(Clone, Copy, Debug)]
pub struct FeedRow<'a> {
    /// The short chip: `BUY`, `SELL`, `MINT`, `LAUNCH`, `MOVE`.
    pub chip: &'a str,
    /// The chip's ink, and the row's left stripe.
    pub tone: Color,
    /// Who or what it was.
    pub title: &'a str,
    /// Whether `title` is a handle, which the feed draws in the accent.
    pub title_is_handle: bool,
    /// The line under the title.
    pub sub: &'a str,
    /// The amount, right-aligned; an empty string still takes its line.
    pub right: &'a str,
    pub right_tone: Color,
    /// The time, under the amount.
    pub time: &'a str,
}

/// What the feed draws.
#[derive(Clone, Copy, Debug)]
pub struct FeedVm<'a> {
    /// Newest first.
    pub rows: &'a [FeedRow<'a>],
    /// Seconds since the newest row landed: drives the step-down and slide-in.
    /// Ignored when `scrolled` is set.
    pub age: f32,
    /// The scrolled-back pill, when the reader has pulled the feed back.
    pub scrolled: Option<&'a str>,
}

/// Draw one row, faded by `fade`.
pub fn feed_row(p: &Painter, row: &FeedRow, rect: Rect, fade: f32) {
    draw_rounded_rect(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        8.0,
        with_alpha(p.theme.color.bg_secondary, 0.9 * fade),
    );
    draw_rounded_rect(rect.x, rect.y, 3.0, rect.h, 1.5, with_alpha(row.tone, fade));
    let cs = p.size(TextSize::Xs);
    // Room for the widest chip, LAUNCH, with a margin: at 52 it touched both
    // edges.
    let cw = p.measure("LAUNCH", cs).width + 14.0;
    let chip = Rect::new(rect.x + 9.0, rect.y + 8.0, cw, 18.0);
    draw_rounded_rect(
        chip.x,
        chip.y,
        chip.w,
        chip.h,
        5.0,
        with_alpha(row.tone, 0.18 * fade),
    );
    p.text_in(
        row.chip,
        chip,
        cs,
        with_alpha(row.tone, fade),
        crate::Align::Center,
    );

    // A title over its subtitle, and the amount over the time on the right:
    // each a block centred in the row, as one-line labels are.
    let x = chip.right() + 9.0;
    let right_w = 86.0;
    let room = rect.right() - right_w - x;
    let ts = p.size(TextSize::Base);
    let title = p.fit(row.title, ts, room);
    let tone = if row.title_is_handle {
        p.theme.color.accent
    } else {
        p.theme.color.text_primary
    };
    let sub = p.fit(row.sub, cs, room);
    let gap = 5.0;
    let left = Rect::new(x, rect.y, room, rect.h);
    p.stack_in(
        &[
            Line::new(&title, ts, with_alpha(tone, fade)),
            Line::new(&sub, cs, with_alpha(p.theme.color.text_muted, fade)),
        ],
        left,
        crate::Align::Start,
        gap,
    );
    let right = Rect::new(rect.right() - right_w - 9.0, rect.y, right_w, rect.h);
    let rs = p.size(TextSize::Md);
    // No amount still takes its line, so the time sits level with the
    // subtitle in every row.
    p.stack_in(
        &[
            Line::new(row.right, rs, with_alpha(row.right_tone, fade)),
            Line::new(row.time, cs, with_alpha(p.theme.color.text_muted, fade)),
        ],
        right,
        crate::Align::End,
        gap,
    );
}

/// Draw the feed in `area`, newest first. The rows that fit are
/// `area.h / FEED_ROW_STEP`. With `scrolled`, the rows are still (the reader
/// is looking back) and the pill says how far.
pub fn feed(p: &Painter, vm: &FeedVm, area: Rect) {
    let rows = ((area.h / FEED_ROW_STEP).floor().max(1.0)) as usize;
    if let Some(label) = vm.scrolled {
        for (i, row) in vm.rows.iter().take(rows).enumerate() {
            let r = Rect::new(
                area.x,
                area.y + i as f32 * FEED_ROW_STEP,
                area.w,
                FEED_ROW_H,
            );
            feed_row(p, row, r, 1.0);
        }
        pill_label(
            p,
            label,
            area.center().x,
            area.y - 15.0,
            area,
            p.theme.color.accent_yellow,
        );
        return;
    }
    // The rest step down a slot first, then the newest slides in from the
    // left into the room they left: together, the two overlapped. The feed
    // ends at the screen's foot, so the row pushed off is not drawn.
    let age = vm.age.max(0.0);
    let down = cubic_out((age / 0.15).min(1.0));
    let arrive = cubic_out(((age - 0.12) / 0.2).clamp(0.0, 1.0));
    for (i, row) in vm.rows.iter().take(rows).enumerate() {
        let (x, y, fade) = if i == 0 {
            (area.x - (1.0 - arrive) * 40.0, area.y, arrive)
        } else {
            (
                area.x,
                area.y + (i as f32 - 1.0 + down) * FEED_ROW_STEP,
                1.0,
            )
        };
        if fade <= 0.01 {
            continue;
        }
        feed_row(p, row, Rect::new(x, y, area.w, FEED_ROW_H), fade);
    }
}

/// `Ease::CubicOut`, without a gamekit dependency: fast start, no overshoot.
fn cubic_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arrival_settles_exactly() {
        assert_eq!(cubic_out(0.0), 0.0);
        assert_eq!(cubic_out(1.0), 1.0);
        assert!(cubic_out(0.5) > 0.5);
        assert_eq!(cubic_out(-1.0), 0.0);
        assert_eq!(cubic_out(2.0), 1.0);
    }

    #[test]
    fn one_step_is_a_row_and_its_gap() {
        assert!((FEED_ROW_STEP - (FEED_ROW_H + 3.0)).abs() < f32::EPSILON);
    }
}
