//! `Icon` — small icons drawn with shapes (a sword, a shield, a heart, a coin), so a stat or an effect shows without an icon font.
//!
//! macroquad has no icon font, and a font of symbols is a second atlas to
//! warm and a bundle to ship. These are a few triangles, lines and circles
//! each, filling whatever square they are given, in one colour.
//!
//! Moved here from the fight spike's `ui.rs` once a second game (the NFT
//! cards in `macroquad-cnft`) needed the same set: a sword and a shield should
//! not be drawn two ways in one estate.

use macroquad::prelude::*;

use crate::shape::{fill_rounded, stroke_rounded};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Icon {
    /// Attack, damage.
    Sword,
    /// Defence, block.
    Shield,
    /// Rage, burn.
    Flame,
    /// Two stacked cards: draw.
    Cards,
    /// A plus: heal.
    Plus,
    /// A card with a bar through it: strip a card from the hand.
    Strip,
    /// A coin: loot, value.
    Coin,
    /// A lightning bolt: speed, energy.
    Bolt,
    /// A heart: health.
    Heart,
    /// A diamond: a rarity mark, or any stat without its own icon.
    Diamond,
}

impl Icon {
    pub const ALL: [Icon; 10] = [
        Icon::Sword,
        Icon::Shield,
        Icon::Flame,
        Icon::Cards,
        Icon::Plus,
        Icon::Strip,
        Icon::Coin,
        Icon::Bolt,
        Icon::Heart,
        Icon::Diamond,
    ];
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

/// Draw `icon` filling `r`, in `color`.
pub fn draw_icon(icon: Icon, r: Rect, color: Color) {
    let c = r.center();
    let s = r.w.min(r.h);
    match icon {
        Icon::Sword => {
            // Blade from lower-left to upper-right, a crossguard and a pommel.
            let a = vec2(r.x + r.w * 0.22, r.y + r.h * 0.82);
            let b = vec2(r.x + r.w * 0.85, r.y + r.h * 0.12);
            let w = (s * 0.16).max(1.5);
            draw_line(a.x, a.y, b.x, b.y, w, color);
            let g = a + (b - a) * 0.25;
            let n = vec2(-(b - a).y, (b - a).x).normalize() * r.w * 0.22;
            draw_line(g.x - n.x, g.y - n.y, g.x + n.x, g.y + n.y, w, color);
            draw_circle(a.x, a.y, w * 0.9, color);
        }
        Icon::Shield => {
            let top = r.h * 0.62;
            draw_rectangle(r.x + r.w * 0.12, r.y, r.w * 0.76, top, color);
            draw_triangle(
                vec2(r.x + r.w * 0.12, r.y + top),
                vec2(r.x + r.w * 0.88, r.y + top),
                vec2(c.x, r.y + r.h),
                color,
            );
        }
        Icon::Flame => {
            let rad = s * 0.32;
            let base = vec2(c.x, r.y + r.h - rad);
            draw_circle(base.x, base.y, rad, color);
            draw_triangle(
                vec2(base.x - rad, base.y),
                vec2(base.x + rad, base.y),
                vec2(c.x + rad * 0.3, r.y),
                color,
            );
        }
        Icon::Cards => {
            // Two upright cards, the back one offset up and right.
            let (w, h) = (r.w * 0.58, r.h * 0.78);
            let back = Rect::new(r.x + r.w * 0.36, r.y, w, h);
            let front = Rect::new(r.x + r.w * 0.06, r.y + r.h * 0.22, w, h);
            let rad = w * 0.16;
            fill_rounded(back, rad, with_alpha(color, 0.55));
            fill_rounded(front, rad, color);
        }
        Icon::Plus => {
            let t = s * 0.3;
            draw_rectangle(c.x - t / 2.0, r.y + r.h * 0.08, t, r.h * 0.84, color);
            draw_rectangle(r.x + r.w * 0.08, c.y - t / 2.0, r.w * 0.84, t, color);
        }
        Icon::Strip => {
            let card = Rect::new(r.x + r.w * 0.2, r.y + r.h * 0.06, r.w * 0.6, r.h * 0.88);
            stroke_rounded(card, card.w * 0.16, (r.w * 0.1).max(1.2), color);
            let t = (r.h * 0.14).max(1.5);
            draw_rectangle(r.x, c.y - t / 2.0, r.w, t, color);
        }
        Icon::Coin => {
            let rad = s / 2.0;
            draw_circle(c.x, c.y, rad, color);
            draw_circle_lines(
                c.x,
                c.y,
                rad * 0.68,
                (rad * 0.14).max(1.0),
                with_alpha(BLACK, 0.35),
            );
        }
        Icon::Bolt => {
            let o = vec2(c.x - s / 2.0, c.y - s / 2.0);
            draw_triangle(
                o + vec2(s * 0.62, 0.0),
                o + vec2(s * 0.2, s * 0.58),
                o + vec2(s * 0.55, s * 0.5),
                color,
            );
            draw_triangle(
                o + vec2(s * 0.45, s * 0.45),
                o + vec2(s * 0.8, s * 0.42),
                o + vec2(s * 0.38, s),
                color,
            );
        }
        Icon::Heart => {
            let o = vec2(c.x - s / 2.0, c.y - s / 2.0);
            let rr = s * 0.25;
            draw_circle(o.x + s * 0.29, o.y + s * 0.34, rr, color);
            draw_circle(o.x + s * 0.71, o.y + s * 0.34, rr, color);
            draw_triangle(
                o + vec2(s * 0.06, s * 0.42),
                o + vec2(s * 0.94, s * 0.42),
                o + vec2(s * 0.5, s * 0.94),
                color,
            );
        }
        Icon::Diamond => {
            draw_poly(c.x, c.y, 4, s * 0.48, 0.0, color);
        }
    }
}
