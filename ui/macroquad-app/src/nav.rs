//! `bottom_nav` and `nav_rail` — the app's top-level destinations, always in
//! reach: a bar along the bottom on a phone, a rail down the left on anything
//! wider. [`crate::shell_layout`] says which, and where.
//!
//! Each destination is a glyph over its label. The one the reader is on sits
//! in a filled pill and its label is in the primary ink; the rest are muted. A
//! destination that needs something first (a wallet) shows a padlock and still
//! takes its tap, so the app can say what it needs rather than ignore the
//! reader.

use macroquad::prelude::*;
use macroquad_widgets::{Align, Painter, fill_rounded, theme::with_alpha};
use ui_theme::TextSize;

use crate::glyph::{Glyph, draw_glyph};

/// One destination.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavItem<'a> {
    pub glyph: Glyph,
    pub label: &'a str,
    /// Needs something first: drawn with a padlock, still tappable.
    pub locked: bool,
}

impl<'a> NavItem<'a> {
    pub fn new(glyph: Glyph, label: &'a str) -> Self {
        Self {
            glyph,
            label,
            locked: false,
        }
    }

    pub fn locked(mut self, locked: bool) -> Self {
        self.locked = locked;
        self
    }
}

/// The bar along the bottom: `band` painted to the screen's edge, the items
/// spread evenly across `row`. Returns the index of a tapped item.
pub fn bottom_nav(
    p: &Painter,
    band: Rect,
    row: Rect,
    items: &[NavItem],
    selected: usize,
) -> Option<usize> {
    let c = &p.theme.color;
    draw_rectangle(band.x, band.y, band.w, band.h, c.bg_secondary);
    draw_rectangle(band.x, band.y, band.w, 1.0, c.border);
    if items.is_empty() {
        return None;
    }
    let w = row.w / items.len() as f32;
    let mut tapped = None;
    for (i, item) in items.iter().enumerate() {
        let cell = Rect::new(row.x + w * i as f32, row.y, w, row.h);
        if destination(p, cell, item, i == selected) {
            tapped = Some(i);
        }
    }
    tapped
}

/// The rail down the left, items stacked from its top. Returns the index of
/// a tapped item.
pub fn nav_rail(p: &Painter, rail: Rect, items: &[NavItem], selected: usize) -> Option<usize> {
    let c = &p.theme.color;
    draw_rectangle(rail.x, rail.y, rail.w, rail.h, c.bg_primary);
    draw_rectangle(rail.right() - 1.0, rail.y, 1.0, rail.h, c.border);
    let mut tapped = None;
    for (i, item) in items.iter().enumerate() {
        let cell = Rect::new(rail.x, rail.y + 12.0 + 72.0 * i as f32, rail.w, 64.0);
        if destination(p, cell, item, i == selected) {
            tapped = Some(i);
        }
    }
    tapped
}

/// One destination in `cell`: the pill and glyph in its upper part, the
/// label under it. Returns whether it was tapped.
fn destination(p: &Painter, cell: Rect, item: &NavItem, on: bool) -> bool {
    let c = &p.theme.color;
    let hit = p.interact(cell, true);
    let pill = Rect::new(cell.center().x - 28.0, cell.y + 6.0, 56.0, 30.0);
    if on {
        fill_rounded(pill, 15.0, with_alpha(c.accent, 0.22));
    } else if hit.pressed || hit.hover {
        let a = if hit.pressed { 0.14 } else { 0.07 };
        fill_rounded(pill, 15.0, with_alpha(c.text_primary, a));
    }
    let ink = if on { c.text_primary } else { c.text_muted };
    let glyph = Rect::new(pill.center().x - 11.0, pill.center().y - 11.0, 22.0, 22.0);
    draw_glyph(item.glyph, glyph, if on { c.accent } else { ink });
    if item.locked {
        let lock = Rect::new(glyph.right() - 2.0, glyph.y - 4.0, 13.0, 13.0);
        draw_circle(lock.center().x, lock.center().y, 8.0, c.bg_secondary);
        draw_glyph(Glyph::Lock, lock, c.text_muted);
    }
    let label = Rect::new(cell.x + 2.0, pill.bottom() + 2.0, cell.w - 4.0, 20.0);
    let size = p.size(TextSize::Sm);
    let text = p.fit(item.label, size, label.w);
    p.text_in(&text, label, size, ink, Align::Center);
    hit.clicked
}
