//! `Segmented` molecule — a row of mutually exclusive options in one track, the picked one filled.
//!
//! Stateless: the host owns which is picked and re-projects it; the widget
//! reports a tap on another. Each segment is a [`Button`]: the picked one
//! filled in the accent, the rest ghosts over a shared rounded track, so the
//! row reads as one control rather than loose buttons.

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::button::{Button, ButtonVariant};
use crate::painter::{Painter, draw_rounded_rect};

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
