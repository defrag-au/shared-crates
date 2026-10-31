//! `Chip` — a small rounded toggle (a filter, a sort, a trait value), laid out
//! in rows that wrap rather than scroll sideways.
//!
//! Wrapping on purpose: a sideways strip inside a page that scrolls down would
//! fight it for every drag, and a phone's reader cannot see what is past its
//! edge. [`chip_layout`] places them without drawing, so a sheet can size
//! itself to its chips before it opens.

use macroquad::prelude::*;
use macroquad_widgets::{Align, Painter, fill_rounded, stroke_rounded, theme::with_alpha};
use ui_theme::TextSize;

use crate::glyph::{Glyph, draw_glyph};

/// A chip's height, in points: under a finger's 44 because chips sit in rows
/// with a gap that the touch target extends into.
pub const CHIP_H: f32 = 34.0;
const GAP: f32 = 8.0;
const PAD: f32 = 14.0;

/// One chip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chip<'a> {
    pub label: &'a str,
    pub selected: bool,
    /// A count after the label, muted: how many match.
    pub count: Option<&'a str>,
    pub glyph: Option<Glyph>,
}

impl<'a> Chip<'a> {
    pub fn new(label: &'a str, selected: bool) -> Self {
        Self {
            label,
            selected,
            count: None,
            glyph: None,
        }
    }

    pub fn count(mut self, count: &'a str) -> Self {
        self.count = Some(count);
        self
    }

    pub fn glyph(mut self, glyph: Glyph) -> Self {
        self.glyph = Some(glyph);
        self
    }
}

/// A row of chips once drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChipRow {
    /// How tall the wrapped rows came out.
    pub height: f32,
    /// The chip tapped, by index.
    pub tapped: Option<usize>,
}

/// Where each chip goes, wrapping at `width` from `origin`, and the height
/// the rows take.
pub fn chip_layout(p: &Painter, origin: Vec2, width: f32, chips: &[Chip]) -> (Vec<Rect>, f32) {
    let size = p.size(TextSize::Base);
    let mut rects = Vec::with_capacity(chips.len());
    let (mut x, mut y) = (origin.x, origin.y);
    for chip in chips {
        // A point of slack: the label is measured again when it is fitted,
        // and a width equal to the text to the last bit of a float cuts it.
        let mut w = p.measure(chip.label, size).width.ceil() + 1.0 + PAD * 2.0;
        if let Some(n) = chip.count {
            w += p.measure_mono(n, p.size(TextSize::Sm)).width + 6.0;
        }
        if chip.glyph.is_some() || chip.selected {
            w += 20.0;
        }
        let w = w.min(width);
        if x > origin.x && x + w > origin.x + width {
            x = origin.x;
            y += CHIP_H + GAP;
        }
        rects.push(Rect::new(x, y, w, CHIP_H));
        x += w + GAP;
    }
    let height = if chips.is_empty() {
        0.0
    } else {
        y + CHIP_H - origin.y
    };
    (rects, height)
}

/// Draw `chips` wrapping across `width` from `origin`.
pub fn chip_row(p: &Painter, origin: Vec2, width: f32, chips: &[Chip]) -> ChipRow {
    let c = &p.theme.color;
    let (rects, height) = chip_layout(p, origin, width, chips);
    let size = p.size(TextSize::Base);
    let small = p.size(TextSize::Sm);
    let mut tapped = None;
    for (i, (chip, r)) in chips.iter().zip(&rects).enumerate() {
        let hit = p.interact(*r, true);
        let radius = r.h * 0.5;
        if chip.selected {
            fill_rounded(*r, radius, with_alpha(c.accent, 0.20));
            stroke_rounded(*r, radius, 1.0, with_alpha(c.accent, 0.7));
        } else {
            let fill = if hit.pressed {
                c.bg_highlight
            } else {
                c.bg_secondary
            };
            fill_rounded(*r, radius, fill);
            stroke_rounded(*r, radius, 1.0, c.border);
        }
        let mut x = r.x + PAD;
        let ink = if chip.selected {
            c.text_primary
        } else {
            c.text_secondary
        };
        let lead = if chip.selected {
            Some(Glyph::Check)
        } else {
            chip.glyph
        };
        if let Some(g) = lead {
            let gr = Rect::new(x - 4.0, r.center().y - 8.0, 16.0, 16.0);
            draw_glyph(g, gr, if chip.selected { c.accent } else { ink });
            x += 16.0;
        }
        let count_w = chip
            .count
            .map_or(0.0, |n| p.measure_mono(n, small).width + 6.0);
        let label_box = Rect::new(x, r.y, (r.right() - PAD - count_w - x).max(0.0), r.h);
        let label = p.fit(chip.label, size, label_box.w);
        p.text_in(&label, label_box, size, ink, Align::Start);
        if let Some(n) = chip.count {
            let count_box = Rect::new(r.right() - PAD - count_w + 6.0, r.y, count_w, r.h);
            p.mono_in(n, count_box, small, c.text_muted, Align::Start);
        }
        // The gap between rows is part of each chip's target.
        let target = Rect::new(r.x, r.y - GAP * 0.5, r.w, r.h + GAP);
        if p.tapped(target) {
            tapped = Some(i);
        }
    }
    ChipRow { height, tapped }
}
