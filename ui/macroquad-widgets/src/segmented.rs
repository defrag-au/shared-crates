//! `Segmented` molecule — a row of mutually exclusive options in one track, the picked one filled.
//!
//! Stateless: the host owns which is picked and re-projects it; the widget
//! reports a tap on another. Each segment is a [`Button`]: the picked one
//! filled in the accent, the rest ghosts over a shared rounded track, so the
//! row reads as one control rather than loose buttons.
//!
//! ## Two shapes
//!
//! [`segmented`] is the track: one background, inset buttons, and it answers a
//! tap itself. [`segmented_pills`] is the other convention the flow surfaces
//! use — separate chips with a gap and no track, for a view switch or a span
//! picker sitting over a plot, where a filled control would read as heavy.
//! [`pill_rects`] is the geometry both need, so a host that hit-tests its own
//! chips cannot disagree with the ones drawn.

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::button::{Button, ButtonVariant};
use crate::painter::{Painter, draw_rounded_rect};
use crate::theme::with_alpha;

/// Draw `options` side by side, equal widths, filling `rect`, with
/// `selected` picked. Returns the index tapped this frame, if it's not the
/// one already picked. `enabled = false` greys them all.
pub fn segmented(
    p: &Painter,
    rect: Rect,
    options: &[&str],
    selected: usize,
    enabled: bool,
) -> Option<usize> {
    if options.is_empty() {
        return None;
    }
    let radius = (rect.h * 0.22).min(10.0);
    draw_rounded_rect(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        radius,
        p.theme.color.bg_highlight,
    );
    // The hairline every surface wears, inset half a line so it keeps its
    // whole width.
    crate::shape::stroke_rounded(
        Rect::new(rect.x + 0.5, rect.y + 0.5, rect.w - 1.0, rect.h - 1.0),
        (radius - 0.5).max(0.0),
        1.0,
        p.theme.color.border,
    );
    let inset = 3.0;
    let w = (rect.w - inset * 2.0) / options.len() as f32;
    let mut tapped = None;
    for (i, label) in options.iter().enumerate() {
        let r = Rect::new(
            rect.x + inset + w * i as f32,
            rect.y + inset,
            w,
            rect.h - inset * 2.0,
        );
        let variant = if i == selected {
            ButtonVariant::Filled
        } else {
            ButtonVariant::Ghost
        };
        if Button::new(label)
            .variant(variant)
            .text_size(TextSize::Md)
            .enabled(enabled)
            .show(p, r)
            && i != selected
        {
            tapped = Some(i);
        }
    }
    tapped
}

/// The chips [`segmented_pills`] draws in `rect`: equal widths, one gap.
///
/// `rect` is the row, not a chip. Exposed so a host that wires its own taps
/// hit-tests exactly the rectangles that were drawn.
pub fn pill_rects(rect: Rect, count: usize) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    const GAP: f32 = 4.0;
    let w = (rect.w - GAP * (count - 1) as f32) / count as f32;
    (0..count)
        .map(|i| Rect::new(rect.x + i as f32 * (w + GAP), rect.y, w, rect.h))
        .collect()
}

/// Draw `options` as separate chips across `rect`, the picked one filled, and
/// return the chip rectangles.
///
/// The lighter sibling of [`segmented`]: no shared track, a tight radius, and
/// the accent at full strength on one chip rather than a filled button. The
/// tapped chip is the host's to find with [`pill_rects`] — a control this
/// small sits over a plot the host is already hit-testing.
pub fn segmented_pills(p: &Painter, rect: Rect, options: &[&str], selected: usize) -> Vec<Rect> {
    let chips = pill_rects(rect, options.len());
    let s = p.size(TextSize::Xs) * 0.9;
    for (i, (label, chip)) in options.iter().zip(&chips).enumerate() {
        let on = i == selected;
        draw_rounded_rect(
            chip.x,
            chip.y,
            chip.w,
            chip.h,
            4.0,
            with_alpha(p.theme.color.accent, if on { 0.8 } else { 0.14 }),
        );
        p.text_in(
            label,
            *chip,
            s,
            if on {
                p.theme.color.bg_primary
            } else {
                p.theme.color.text_secondary
            },
            crate::Align::Center,
        );
    }
    chips
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chips_fill_the_row_and_do_not_overlap() {
        let row = Rect::new(10.0, 20.0, 5.0 * 34.0 + 4.0 * 4.0, 16.0);
        let chips = pill_rects(row, 5);
        assert_eq!(chips.len(), 5);
        for chip in &chips {
            assert!((chip.w - 34.0).abs() < 1e-3, "{chip:?}");
            assert!((chip.h - row.h).abs() < 1e-3);
        }
        assert!((chips[0].x - row.x).abs() < 1e-3);
        assert!((chips[4].right() - row.right()).abs() < 1e-3);
        for pair in chips.windows(2) {
            assert!(pair[1].x > pair[0].right(), "{pair:?}");
        }
    }

    #[test]
    fn no_options_is_no_chips() {
        assert!(pill_rects(Rect::new(0.0, 0.0, 100.0, 16.0), 0).is_empty());
    }
}
