//! `LifeStrip` — a token's whole life on one axis: a fuzzy price band, how
//! busy each stretch was, the window being replayed as a brush, and the
//! playhead.
//!
//! The companion to [`price_chart`](crate::price_chart): the chart draws the
//! window sharp, the strip places that window in the whole life. A finger on
//! the strip moves the window anywhere; a finger on the chart scrubs inside it.
//!
//! The fuzzy band is a **log scale** (a token's life spans orders of
//! magnitude) and each bucket draws its low-to-high with its close blurred over
//! it. A bucket no reading fell in is drawn fainter, so "nothing was looked at"
//! never reads as "the price did not move".

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::market_ink::MarketInk;
use crate::marks::pill_label;
use crate::painter::{Painter, draw_rounded_rect};
use crate::theme::with_alpha;

/// One stretch of a life: its low, high and close, and how busy it was.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LifeBucket {
    /// The stretch's start, unix seconds.
    pub unix: u64,
    /// How long the stretch is, unix seconds.
    pub bucket_secs: u64,
    pub lo: f64,
    pub hi: f64,
    pub close: f64,
    /// Transactions in the stretch, where they were counted.
    pub txs: u64,
    /// Whether any reading fell in it: a carried price is drawn fainter.
    pub seen: bool,
}

/// What the strip draws.
#[derive(Clone, Copy, Debug)]
pub struct LifeStripVm<'a> {
    pub buckets: &'a [LifeBucket],
    /// The whole life's axis.
    pub t0: u64,
    pub t1: u64,
    /// The window being replayed: its start and now.
    pub brush: (u64, u64),
    /// The playhead's moment, unix seconds.
    pub head: u64,
    /// What the chart above draws, where that is less than the window: a
    /// bracket so the chart's few weeks sit visibly in the whole life.
    pub view: Option<(u64, u64)>,
    /// Where the loaded history starts.
    pub loaded_from: u64,
    /// Whether the start of history is known (a rewind at the tape's start).
    pub at_start: bool,
    /// A rewind waiting on older pages: the brush pulses.
    pub pending: bool,
    /// A finger on the strip: the moment letting go will start the window.
    pub drag: Option<u64>,
    /// The drag's label, preformatted.
    pub drag_label: Option<&'a str>,
    /// The axis's first date, under it.
    pub first_label: Option<&'a str>,
    /// The axis's right end, under it.
    pub now_label: Option<&'a str>,
    pub ink: MarketInk,
}

/// Draw the strip in `rect`.
pub fn life_strip(p: &Painter, vm: &LifeStripVm, rect: Rect) {
    draw_rounded_rect(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        6.0,
        with_alpha(p.theme.color.bg_primary, 0.6),
    );
    let span = (vm.t0, vm.t1.max(vm.t0 + 1));
    let x_of = |t: u64| {
        rect.x + rect.w * (t.saturating_sub(span.0) as f32 / (span.1 - span.0) as f32).min(1.0)
    };
    price_band(
        p,
        vm.buckets,
        rect,
        span.0,
        span.1,
        p.theme.color.text_secondary,
        true,
    );

    // What is loaded, underlined; where history starts, when that is known.
    let loaded = x_of(vm.loaded_from);
    if vm.at_start {
        draw_rectangle(
            loaded - 1.0,
            rect.y,
            2.0,
            rect.h,
            with_alpha(vm.ink.ada, 0.6),
        );
    }
    draw_line(
        loaded,
        rect.bottom() + 1.5,
        rect.right(),
        rect.bottom() + 1.5,
        2.0,
        with_alpha(vm.ink.accent, 0.55),
    );

    // The brush: the window, from where it starts to now.
    // Never thinner than a finger can see: a day on a two-year life is a
    // third of a point.
    let bx = x_of(vm.brush.0).min(rect.right() - 8.0);
    let pulse = if vm.pending {
        0.5 + 0.5 * (p.time() as f32 * 4.0).sin().abs()
    } else {
        1.0
    };
    draw_rectangle(
        bx,
        rect.y,
        rect.right() - bx,
        rect.h,
        with_alpha(vm.ink.accent, 0.13 * pulse),
    );
    draw_rectangle_lines(
        bx,
        rect.y,
        rect.right() - bx,
        rect.h,
        1.5,
        with_alpha(vm.ink.accent, 0.8 * pulse),
    );
    draw_rectangle(bx - 1.5, rect.y - 2.0, 3.0, rect.h + 4.0, vm.ink.accent);

    // The playhead: where the chart above has got to on the whole life, and
    // the window behind it, played, a shade brighter.
    if !vm.pending {
        let head = x_of(vm.head).clamp(bx, rect.right());
        draw_rectangle(
            bx,
            rect.y,
            head - bx,
            rect.h,
            with_alpha(vm.ink.accent, 0.14),
        );
        draw_line(
            head,
            rect.y - 3.0,
            head,
            rect.bottom() + 3.0,
            2.0,
            p.theme.color.text_primary,
        );
        draw_circle(head, rect.y - 3.0, 2.5, p.theme.color.text_primary);

        // What the chart above draws, where that is less than the window: a
        // bracket, so the chart's few weeks sit visibly in the whole life and
        // slide along it with the playhead.
        if let Some((t0, t1)) = vm.view {
            let (x0, x1) = (x_of(t0), x_of(t1).max(x_of(t0) + 3.0));
            let ink = with_alpha(p.theme.color.text_primary, 0.85);
            for y in [rect.y - 1.0, rect.bottom() + 1.0] {
                draw_line(x0, y, x1, y, 2.0, ink);
            }
            for x in [x0, x1] {
                draw_line(x, rect.y - 1.0, x, rect.y + 5.0, 2.0, ink);
                draw_line(x, rect.bottom() - 5.0, x, rect.bottom() + 1.0, 2.0, ink);
            }
        }
    }

    let s = p.size(TextSize::Xs);
    // A finger on the strip: where letting go will start the window.
    if let (Some(drag), Some(label)) = (vm.drag, vm.drag_label) {
        let x = x_of(drag).clamp(rect.x, rect.right());
        draw_line(x, rect.y - 3.0, x, rect.bottom() + 3.0, 2.0, vm.ink.crown);
        pill_label(p, label, x, rect.y - 16.0, rect, vm.ink.crown);
    }
    // Its own row under it: where the life starts, and now.
    let row = Rect::new(rect.x, rect.bottom() + 2.0, rect.w, 12.0);
    if let Some(first) = vm.first_label {
        p.text_in(first, row, s, p.theme.color.text_muted, crate::Align::Start);
    }
    if let Some(now) = vm.now_label {
        p.text_in(now, row, s, p.theme.color.text_muted, crate::Align::End);
    }
}

/// The fuzzy price over `(t0, t1)` in `rect`: each stretch's low-to-high as a
/// soft band, its close as a blurred line, on a log scale, with how busy it
/// was as faint bars beneath.
pub fn price_band(
    p: &Painter,
    buckets: &[LifeBucket],
    rect: Rect,
    t0: u64,
    t1: u64,
    tone: Color,
    busy: bool,
) {
    let shown = visible(buckets, t0, t1);
    if shown.is_empty() {
        return;
    }
    let span = (t1.saturating_sub(t0)).max(1) as f32;
    let x_of = |t: u64| rect.x + rect.w * (t.saturating_sub(t0) as f32 / span).min(1.0);
    let (lo, hi) = shown.iter().fold((f64::MAX, f64::MIN), |(lo, hi), b| {
        (lo.min(b.lo.max(1e-12)), hi.max(b.hi))
    });
    let (llo, lhi) = (lo.ln(), (hi.ln()).max(lo.ln() + 1e-6));
    let y_of = |price: f64| {
        let f = ((price.max(1e-12).ln() - llo) / (lhi - llo)) as f32;
        rect.bottom() - 2.0 - (rect.h - 5.0) * f.clamp(0.0, 1.0)
    };
    if busy {
        let most = shown.iter().map(|b| b.txs).max().unwrap_or(0).max(1) as f32;
        for b in &shown {
            if b.txs > 0 {
                let x0 = x_of(b.unix.max(t0));
                let w = (x_of(b.unix + b.bucket_secs) - x0).max(1.0);
                let h = rect.h * 0.4 * (b.txs as f32 / most).sqrt();
                draw_rectangle(
                    x0,
                    rect.bottom() - h,
                    w,
                    h,
                    with_alpha(p.theme.color.accent, 0.16),
                );
            }
        }
    }
    let mut line: Vec<Vec2> = Vec::new();
    for b in &shown {
        let x0 = x_of(b.unix.max(t0));
        let w = (x_of(b.unix + b.bucket_secs) - x0).max(1.0);
        let (top, bottom) = (y_of(b.hi), y_of(b.lo));
        let a = if b.seen { 0.24 } else { 0.08 };
        draw_rectangle(x0, top - 1.5, w, (bottom - top) + 3.0, with_alpha(tone, a));
        line.push(vec2(x0 + w * 0.5, y_of(b.close)));
    }
    for (width, a) in [(6.0, 0.07), (3.0, 0.16), (1.2, 0.75)] {
        for seg in line.windows(2) {
            draw_line(
                seg[0].x,
                seg[0].y,
                seg[1].x,
                seg[1].y,
                width,
                with_alpha(tone, a),
            );
        }
    }
}

/// The buckets that overlap `(t0, t1)`: a stretch starting before the axis
/// ends and ending after it starts.
fn visible(buckets: &[LifeBucket], t0: u64, t1: u64) -> Vec<&LifeBucket> {
    buckets
        .iter()
        .filter(|b| b.unix + b.bucket_secs >= t0 && b.unix <= t1)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bucket(unix: u64) -> LifeBucket {
        LifeBucket {
            unix,
            bucket_secs: 100,
            lo: 1.0,
            hi: 2.0,
            close: 1.5,
            txs: 1,
            seen: true,
        }
    }

    #[test]
    fn a_bucket_outside_the_span_is_not_drawn() {
        // The filter is the whole contract: a strip must not paint a stretch
        // the axis does not cover.
        let buckets = [bucket(0), bucket(1_000)];
        let shown = visible(&buckets, 50, 500);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].unix, 0);
    }

    #[test]
    fn a_stretch_overlapping_either_edge_is_kept() {
        let buckets = [bucket(0), bucket(400), bucket(900)];
        let shown = visible(&buckets, 50, 500);
        assert_eq!(
            shown.len(),
            2,
            "the one that ends after the start, and the one that starts before the end"
        );
    }
}
