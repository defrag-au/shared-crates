//! `StatTiles` — a row of equal stat tiles: a figure over its caption, with an
//! optional ADA mark, sharing one baseline and one width.
//!
//! The companion to `egui-widgets`' `StatStrip`, sized for a phone: four tiles
//! across a 350-unit column, or any number that fits. A tile never wraps its
//! figure; a figure too long for its tile is the host's to shorten (a compact
//! number, not a full one).

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::marks::ada_mark;
use crate::painter::{Painter, draw_rounded_rect};
use crate::shape::stroke_rounded;

/// A tile's corner.
const TILE_R: f32 = 10.0;

/// The mark drawn before a tile's figure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatMark {
    Ada,
}

/// One tile: a caption, a figure, and what the figure is measured in.
#[derive(Clone, Copy, Debug)]
pub struct StatTile<'a> {
    pub label: &'a str,
    pub value: &'a str,
    pub mark: Option<StatMark>,
}

/// What the strip draws.
#[derive(Clone, Copy, Debug)]
pub struct StatTilesVm<'a> {
    pub tiles: &'a [StatTile<'a>],
}

/// Draw `vm.tiles` across `rect`, equal widths and one gap. `rect.h` is the
/// tile height; the caller owns where the strip sits.
pub fn stat_tiles(p: &Painter, vm: &StatTilesVm, rect: Rect) {
    if vm.tiles.is_empty() {
        return;
    }
    let gap = 6.0;
    let w = (rect.w - gap * (vm.tiles.len() - 1) as f32) / vm.tiles.len() as f32;
    for (i, tile) in vm.tiles.iter().enumerate() {
        let r = Rect::new(rect.x + i as f32 * (w + gap), rect.y, w, rect.h);
        draw_rounded_rect(r.x, r.y, r.w, r.h, TILE_R, p.theme.color.bg_secondary);
        // The hairline every surface wears, following the tile's own corners
        // and inset half a line so it keeps its whole width. (A square outline
        // round a rounded fill left its corners floating off the tile.)
        stroke_rounded(
            Rect::new(r.x + 0.5, r.y + 0.5, r.w - 1.0, r.h - 1.0),
            TILE_R - 0.5,
            1.0,
            p.theme.color.border,
        );
        // A figure over its caption, on named midlines; the ADA mark is
        // centred on the figure's.
        let value_mid = r.y + r.h * 0.36;
        let label_mid = r.y + r.h * 0.73;
        let mut x = r.x + 8.0;
        if tile.mark == Some(StatMark::Ada) {
            // An icon, not text: sized with the tile rather than the ramp.
            let mark = (r.h * 0.23).clamp(8.0, 12.0);
            ada_mark(
                vec2(x + mark * 0.45, value_mid),
                mark,
                p.theme.color.warning,
            );
            x += mark + 2.0;
        }
        p.text_mid(
            tile.value,
            x,
            value_mid,
            p.size(TextSize::Lg),
            p.theme.color.text_primary,
        );
        p.text_mid(
            tile.label,
            r.x + 8.0,
            label_mid,
            p.size(TextSize::Xs),
            p.theme.color.text_muted,
        );
    }
}
