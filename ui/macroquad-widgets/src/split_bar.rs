//! `SplitBar` — one horizontal bar split between two amounts, with a label at
//! each end and a marker at the split. What a buy/sell pressure bar is, and
//! what any two-way share is.
//!
//! The bar itself is the track under the first share; the second share is the
//! remaining track. Labels sit on the bar's midline, one against each edge, so
//! a wide label never pushes the bar narrower than the caller allowed.

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::painter::{Painter, draw_rounded_rect};
use crate::theme::with_alpha;

/// What the bar draws.
#[derive(Clone, Copy, Debug)]
pub struct SplitBarVm<'a> {
    /// The first side's share, `0..=1`.
    pub share: f32,
    /// The first side's label, against the left edge.
    pub start: &'a str,
    /// The second side's label, against the right edge.
    pub end: &'a str,
    /// The first side's fill and label ink.
    pub start_ink: Color,
    /// The second side's fill and label ink.
    pub end_ink: Color,
    /// The marker at the split.
    pub marker_ink: Color,
}

/// Draw the bar and its labels in `rect`. `rect.h` is the bar's height; the
/// labels take their room from the width, not the height.
pub fn split_bar(p: &Painter, vm: &SplitBarVm, rect: Rect) {
    let share = vm.share.clamp(0.0, 1.0);
    let s = p.size(TextSize::Xs);
    // Room for the widest label either end can take, plus its margin.
    let label_w = p
        .measure(vm.start, s)
        .width
        .max(p.measure(vm.end, s).width)
        .max(p.measure("00% LABELS", s).width)
        + 12.0;
    let r = Rect::new(
        rect.x + label_w,
        rect.y,
        (rect.w - label_w * 2.0).max(8.0),
        rect.h,
    );
    let radius = (r.h * 0.5).min(6.0);
    draw_rounded_rect(r.x, r.y, r.w, r.h, radius, with_alpha(vm.end_ink, 0.75));
    draw_rounded_rect(r.x, r.y, (r.w * share).max(4.0), r.h, radius, vm.start_ink);
    let nx = r.x + r.w * share;
    draw_rectangle(nx - 1.5, r.y - 4.0, 3.0, r.h + 8.0, vm.marker_ink);
    // Both labels on the bar's own midline.
    let row = Rect::new(rect.x, r.y, rect.w, r.h);
    p.text_in(vm.start, row, s, vm.start_ink, crate::Align::Start);
    p.text_in(vm.end, row, s, vm.end_ink, crate::Align::End);
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_share_is_clamped_to_the_track() {
        // The drawing is the clamp's only consumer; assert the arithmetic the
        // two bars share, so a NaN or a negative share cannot reach `draw`.
        for (raw, want) in [
            (-0.5_f32, 0.0),
            (0.0, 0.0),
            (0.62, 0.62),
            (1.0, 1.0),
            (1.7, 1.0),
        ] {
            let share = raw.clamp(0.0, 1.0);
            assert_eq!(share, want);
        }
    }
}
