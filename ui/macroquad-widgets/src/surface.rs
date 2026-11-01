//! `surface` — the raised panel every card and section of a page sits on: the
//! secondary ground, one corner radius, and the theme's hairline drawn just
//! inside the edge.
//!
//! The border is inset by half a line ([`edge`]), so it keeps its whole width
//! on every side: a line centred on the edge loses its outer half wherever a
//! clip runs along that edge, and a clipped list's rows came out with thinner
//! sides than tops. Moved here from the staking site, where it was written
//! for that reason, once the holder app drew the same panel.

use macroquad::prelude::*;

use crate::Painter;
use crate::shape::{fill_rounded, stroke_rounded};

/// Every surface shares one corner.
pub const SURFACE_R: f32 = 12.0;

/// A 1-point line just inside `r`'s edge, in `color`.
pub fn edge(r: Rect, radius: f32, color: Color) {
    stroke_rounded(
        Rect::new(r.x + 0.5, r.y + 0.5, r.w - 1.0, r.h - 1.0),
        (radius - 0.5).max(0.0),
        1.0,
        color,
    );
}

/// A panel or card in `r`: the raised ground and the hairline border, the
/// same border the wallet button and every modal wear.
pub fn surface(p: &Painter, r: Rect) {
    let c = p.theme.color;
    fill_rounded(r, SURFACE_R, c.bg_secondary);
    edge(r, SURFACE_R, c.border);
}
