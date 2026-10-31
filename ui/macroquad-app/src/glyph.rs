//! `Glyph` — the app shell's icons, drawn with lines and shapes so they are
//! crisp at any scale and need no icon font or asset.
//!
//! Each is drawn in a square box with a stroke that grows with it, in one
//! colour, in the outlined style a phone's own navigation uses. The set is the
//! one an app bar and a navigation bar need; a game's icons (swords, coins)
//! are the gamekit's `Icon`.

use std::f32::consts::PI;

use macroquad::prelude::*;
use macroquad_widgets::{fill_rounded, stroke_rounded};

/// See the [module docs](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Glyph {
    /// A chevron pointing left: back.
    Back,
    /// A cross: close, clear.
    Close,
    /// A lens: search.
    Search,
    /// Three bars of falling length: filter.
    Filter,
    /// Two arrows, up and down: sort.
    Sort,
    /// Four squares: a grid of items, browse.
    Grid,
    /// Nine dots: a grid of traits.
    Dots,
    /// A price tag: sell.
    Tag,
    /// A line rising over an axis: a market.
    Chart,
    /// A house: home.
    Home,
    /// A wallet.
    Wallet,
    /// A tick.
    Check,
    /// A padlock: locked until something is done.
    Lock,
    /// Three dots in a row: more.
    More,
    /// A box with an arrow leaving it: opens elsewhere.
    External,
    /// Two overlapping sheets: copy.
    Copy,
    /// A person: an account, a holder.
    Person,
}

/// Draw `glyph` centred in `rect` (its shorter side is the glyph's size).
pub fn draw_glyph(glyph: Glyph, rect: Rect, color: Color) {
    let s = rect.w.min(rect.h);
    let c = rect.center();
    // The drawing box: a little inside the rect, as icon fonts pad theirs.
    let b = Rect::new(c.x - s * 0.42, c.y - s * 0.42, s * 0.84, s * 0.84);
    let t = (s * 0.085).max(1.0);
    let at = |fx: f32, fy: f32| vec2(b.x + b.w * fx, b.y + b.h * fy);
    let line = |a: Vec2, z: Vec2| draw_line(a.x, a.y, z.x, z.y, t, color);
    // Joints get a dot, so a polyline's corners are round rather than notched.
    let joint = |p: Vec2| draw_circle(p.x, p.y, t * 0.5, color);
    let poly = |pts: &[Vec2]| {
        for w in pts.windows(2) {
            line(w[0], w[1]);
        }
        for p in pts {
            joint(*p);
        }
    };
    match glyph {
        Glyph::Back => poly(&[at(0.62, 0.18), at(0.30, 0.50), at(0.62, 0.82)]),
        Glyph::Close => {
            poly(&[at(0.22, 0.22), at(0.78, 0.78)]);
            poly(&[at(0.78, 0.22), at(0.22, 0.78)]);
        }
        Glyph::Search => {
            let o = at(0.42, 0.42);
            draw_circle_lines(o.x, o.y, b.w * 0.27, t, color);
            poly(&[at(0.62, 0.62), at(0.84, 0.84)]);
        }
        Glyph::Filter => {
            poly(&[at(0.14, 0.26), at(0.86, 0.26)]);
            poly(&[at(0.28, 0.50), at(0.72, 0.50)]);
            poly(&[at(0.42, 0.74), at(0.58, 0.74)]);
        }
        Glyph::Sort => {
            poly(&[at(0.34, 0.80), at(0.34, 0.20)]);
            poly(&[at(0.18, 0.36), at(0.34, 0.20), at(0.50, 0.36)]);
            poly(&[at(0.66, 0.20), at(0.66, 0.80)]);
            poly(&[at(0.50, 0.64), at(0.66, 0.80), at(0.82, 0.64)]);
        }
        Glyph::Grid => {
            let r = b.w * 0.06;
            for (fx, fy) in [(0.12, 0.12), (0.54, 0.12), (0.12, 0.54), (0.54, 0.54)] {
                let p = at(fx, fy);
                stroke_rounded(Rect::new(p.x, p.y, b.w * 0.34, b.h * 0.34), r, t, color);
            }
        }
        Glyph::Dots => {
            for fy in [0.18, 0.50, 0.82] {
                for fx in [0.18, 0.50, 0.82] {
                    let p = at(fx, fy);
                    draw_circle(p.x, p.y, t * 0.95, color);
                }
            }
        }
        Glyph::Tag => {
            poly(&[
                at(0.14, 0.14),
                at(0.52, 0.14),
                at(0.88, 0.50),
                at(0.50, 0.88),
                at(0.14, 0.52),
                at(0.14, 0.14),
            ]);
            let h = at(0.34, 0.34);
            draw_circle(h.x, h.y, t * 0.9, color);
        }
        Glyph::Chart => {
            poly(&[at(0.12, 0.12), at(0.12, 0.88), at(0.88, 0.88)]);
            poly(&[at(0.26, 0.66), at(0.46, 0.46), at(0.60, 0.58), at(0.84, 0.28)]);
        }
        Glyph::Home => {
            poly(&[
                at(0.12, 0.48),
                at(0.50, 0.14),
                at(0.88, 0.48),
            ]);
            poly(&[
                at(0.24, 0.40),
                at(0.24, 0.86),
                at(0.76, 0.86),
                at(0.76, 0.40),
            ]);
        }
        Glyph::Wallet => {
            let body = Rect::new(b.x + b.w * 0.10, b.y + b.h * 0.24, b.w * 0.80, b.h * 0.60);
            stroke_rounded(body, b.w * 0.10, t, color);
            let flap = Rect::new(b.x + b.w * 0.56, b.y + b.h * 0.44, b.w * 0.34, b.h * 0.20);
            stroke_rounded(flap, b.w * 0.06, t, color);
        }
        Glyph::Check => poly(&[at(0.16, 0.52), at(0.40, 0.76), at(0.84, 0.26)]),
        Glyph::Lock => {
            let body = Rect::new(b.x + b.w * 0.18, b.y + b.h * 0.44, b.w * 0.64, b.h * 0.44);
            fill_rounded(body, b.w * 0.08, color);
            let o = at(0.50, 0.44);
            let r = b.w * 0.20;
            // The shackle: the top half of a ring, with legs down to the body.
            let steps = 10;
            let arc: Vec<Vec2> = (0..=steps)
                .map(|i| {
                    let a = PI + PI * i as f32 / steps as f32;
                    vec2(o.x + r * a.cos(), o.y - b.h * 0.08 + r * a.sin())
                })
                .collect();
            poly(&arc);
            line(vec2(o.x - r, o.y - b.h * 0.08), vec2(o.x - r, o.y));
            line(vec2(o.x + r, o.y - b.h * 0.08), vec2(o.x + r, o.y));
        }
        Glyph::More => {
            for fx in [0.20, 0.50, 0.80] {
                let p = at(fx, 0.50);
                draw_circle(p.x, p.y, t * 1.1, color);
            }
        }
        Glyph::External => {
            poly(&[
                at(0.44, 0.18),
                at(0.18, 0.18),
                at(0.18, 0.82),
                at(0.82, 0.82),
                at(0.82, 0.56),
            ]);
            poly(&[at(0.48, 0.52), at(0.84, 0.16)]);
            poly(&[at(0.58, 0.16), at(0.84, 0.16), at(0.84, 0.42)]);
        }
        Glyph::Copy => {
            let back = Rect::new(b.x + b.w * 0.12, b.y + b.h * 0.12, b.w * 0.52, b.h * 0.52);
            stroke_rounded(back, b.w * 0.08, t, color);
            let front = Rect::new(b.x + b.w * 0.36, b.y + b.h * 0.36, b.w * 0.52, b.h * 0.52);
            stroke_rounded(front, b.w * 0.08, t, color);
        }
        Glyph::Person => {
            let head = at(0.50, 0.32);
            draw_circle_lines(head.x, head.y, b.w * 0.17, t, color);
            let o = at(0.50, 0.92);
            let r = b.w * 0.34;
            let steps = 12;
            let arc: Vec<Vec2> = (0..=steps)
                .map(|i| {
                    let a = PI + PI * i as f32 / steps as f32;
                    vec2(o.x + r * a.cos(), o.y + r * 0.9 * a.sin())
                })
                .collect();
            poly(&arc);
        }
    }
}
