//! `fill_gradient` and friends — the depth a printed card has and flat fills
//! do not: gradient plates, bevels, inner shadows, a seeded grain texture and
//! CRT scanlines.
//!
//! macroquad fills shapes with one colour. A frame that reads as metal or
//! parchment needs light across it, an edge that catches it, and a surface with
//! some tooth. These build that from meshes with per-vertex colour, layered
//! outlines, and a grain texture generated once from a seed.
//!
//! [`noise_pixels`] is plain data, tested without a window; everything else
//! draws, so its stories in `macroquad-storybook` ("gamekit / card frame") are
//! where it gets reviewed.

use macroquad::models::{Mesh, Vertex, draw_mesh};
use macroquad::prelude::*;

use crate::shape::stroke_rounded;
use crate::tween::Lerp;

/// How colour runs across a filled shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gradient {
    /// `top` at the top edge to `bottom` at the bottom edge.
    Vertical { top: Color, bottom: Color },
    /// `left` at the left edge to `right` at the right edge.
    Horizontal { left: Color, right: Color },
    /// `centre` in the middle to `edge` at the rim.
    Radial { centre: Color, edge: Color },
}

impl Gradient {
    fn at(self, rect: Rect, p: Vec2, rim: bool) -> Color {
        match self {
            Gradient::Vertical { top, bottom } => Color::lerp(
                top,
                bottom,
                ((p.y - rect.y) / rect.h.max(1e-3)).clamp(0.0, 1.0),
            ),
            Gradient::Horizontal { left, right } => Color::lerp(
                left,
                right,
                ((p.x - rect.x) / rect.w.max(1e-3)).clamp(0.0, 1.0),
            ),
            Gradient::Radial { centre, edge } => {
                if rim {
                    edge
                } else {
                    centre
                }
            }
        }
    }
}

/// Points around a rounded rectangle, clockwise from the top-left corner's
/// start, `segments` per corner.
pub(crate) fn rounded_outline(rect: Rect, radius: f32, segments: usize) -> Vec<Vec2> {
    let r = radius.min(rect.w / 2.0).min(rect.h / 2.0).max(0.0);
    let corners = [
        (vec2(rect.x + r, rect.y + r), std::f32::consts::PI),
        (
            vec2(rect.right() - r, rect.y + r),
            std::f32::consts::PI * 1.5,
        ),
        (vec2(rect.right() - r, rect.bottom() - r), 0.0),
        (
            vec2(rect.x + r, rect.bottom() - r),
            std::f32::consts::FRAC_PI_2,
        ),
    ];
    let mut pts = Vec::with_capacity(4 * (segments + 1));
    for (c, start) in corners {
        for i in 0..=segments {
            let a = start + std::f32::consts::FRAC_PI_2 * i as f32 / segments as f32;
            pts.push(c + vec2(a.cos(), a.sin()) * r);
        }
    }
    pts
}

/// Fill a rounded rectangle with a gradient.
pub fn fill_gradient(rect: Rect, radius: f32, gradient: Gradient) {
    let pts = rounded_outline(rect, radius, 10);
    let centre = rect.center();
    let mut vertices = Vec::with_capacity(pts.len() + 1);
    vertices.push(Vertex::new(
        centre.x,
        centre.y,
        0.0,
        0.0,
        0.0,
        gradient.at(rect, centre, false),
    ));
    for p in &pts {
        vertices.push(Vertex::new(
            p.x,
            p.y,
            0.0,
            0.0,
            0.0,
            gradient.at(rect, *p, true),
        ));
    }
    let n = pts.len() as u16;
    let mut indices = Vec::with_capacity(pts.len() * 3);
    for i in 0..n {
        indices.extend([0, 1 + i, 1 + (i + 1) % n]);
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

/// A filled circle shaded from `centre` to `edge`: a gem or a coin.
pub fn fill_circle_gradient(at: Vec2, radius: f32, centre: Color, edge: Color) {
    fill_gradient(
        Rect::new(at.x - radius, at.y - radius, radius * 2.0, radius * 2.0),
        radius,
        Gradient::Radial { centre, edge },
    );
}

/// An edge that catches the light: a highlight along the top and left, a
/// shadow along the bottom and right, each `width` wide.
pub fn bevel(rect: Rect, radius: f32, width: f32, light: Color, dark: Color) {
    let h = width / 2.0;
    stroke_rounded(
        Rect::new(rect.x + h, rect.y + h, rect.w - width, rect.h - width),
        (radius - h).max(0.0),
        width,
        dark,
    );
    stroke_rounded(
        Rect::new(rect.x, rect.y, rect.w - width, rect.h - width),
        radius,
        width,
        light,
    );
}

/// A shadow cast inward from the edge of a recess (an art window, a text
/// box): `depth` points deep, strongest at the edge.
pub fn inner_shadow(rect: Rect, radius: f32, depth: f32, color: Color) {
    let steps = depth.ceil().max(1.0) as usize;
    for i in 0..steps {
        let inset = i as f32 + 0.5;
        let a = color.a * (1.0 - i as f32 / steps as f32).powi(2);
        stroke_rounded(
            Rect::new(
                rect.x + inset,
                rect.y + inset,
                rect.w - 2.0 * inset,
                rect.h - 2.0 * inset,
            ),
            (radius - inset).max(0.0),
            1.0,
            Color { a, ..color },
        );
    }
}

/// Grain for a printed surface: `size` square, RGBA, grey values around mid
/// grey with alpha `strength` (0..1). Smooth noise at two scales, from `seed`,
/// so the same seed gives the same grain everywhere.
pub fn noise_pixels(size: u16, seed: u64, strength: f32) -> Vec<u8> {
    let n = size as usize;
    let lattice = |cells: usize, salt: u64| -> Vec<f32> {
        let mut x = seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        (0..(cells + 1) * (cells + 1))
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 40) as f32 / (1u64 << 24) as f32
            })
            .collect()
    };
    let sample = |grid: &[f32], cells: usize, u: f32, v: f32| -> f32 {
        let (fx, fy) = (u * cells as f32, v * cells as f32);
        let (x0, y0) = (fx.floor() as usize % cells, fy.floor() as usize % cells);
        let (tx, ty) = (fx.fract(), fy.fract());
        let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
        // Wrap at the far edge so the texture tiles.
        let at = |x: usize, y: usize| grid[(y % cells) * (cells + 1) + (x % cells)];
        let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * sx;
        let bot = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * sx;
        top + (bot - top) * sy
    };
    let (coarse, fine) = (8, 32);
    let (g1, g2) = (lattice(coarse, 1), lattice(fine, 2));
    let alpha = (strength.clamp(0.0, 1.0) * 255.0) as u8;
    let mut out = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
            let g = 0.65 * sample(&g1, coarse, u, v) + 0.35 * sample(&g2, fine, u, v);
            let c = (g.clamp(0.0, 1.0) * 255.0) as u8;
            out.extend([c, c, c, alpha]);
        }
    }
    out
}

/// [`noise_pixels`] as a repeating texture, ready to lay over a surface.
pub fn noise_texture(size: u16, seed: u64, strength: f32) -> Texture2D {
    let tex = Texture2D::from_rgba8(size, size, &noise_pixels(size, seed, strength));
    tex.set_filter(FilterMode::Linear);
    tex
}

/// CRT scanlines over `rect`: a one-unit line of `color` every `spacing`
/// units. Lay it over a screen-within-the-screen (a radar, a terminal, a
/// viewport) with a low alpha; it reads as phosphor without hiding anything.
pub fn scanlines(rect: Rect, spacing: f32, color: Color) {
    let spacing = spacing.max(2.0);
    let mut y = rect.y;
    while y < rect.bottom() {
        draw_rectangle(rect.x, y, rect.w, 1.0_f32.min(rect.bottom() - y), color);
        y += spacing;
    }
}

/// Lay a grain texture over `rect` at its natural size, tiled.
pub fn grain(texture: &Texture2D, rect: Rect, tint: Color) {
    let (tw, th) = (texture.width(), texture.height());
    let mut y = rect.y;
    while y < rect.bottom() {
        let h = (rect.bottom() - y).min(th);
        let mut x = rect.x;
        while x < rect.right() {
            let w = (rect.right() - x).min(tw);
            draw_texture_ex(
                texture,
                x,
                y,
                tint,
                DrawTextureParams {
                    dest_size: Some(vec2(w, h)),
                    source: Some(Rect::new(0.0, 0.0, w, h)),
                    ..Default::default()
                },
            );
            x += tw;
        }
        y += th;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_repeatable_and_varies() {
        let a = noise_pixels(32, 7, 0.5);
        assert_eq!(a, noise_pixels(32, 7, 0.5), "same seed, same grain");
        assert_ne!(a, noise_pixels(32, 8, 0.5), "another seed, another grain");
        let greys: Vec<u8> = a.as_chunks::<4>().0.iter().map(|p| p[0]).collect();
        let (lo, hi) = (*greys.iter().min().unwrap(), *greys.iter().max().unwrap());
        assert!(hi - lo > 60, "it has visible variation: {lo}..{hi}");
        assert!(
            a.as_chunks::<4>().0.iter().all(|p| p[3] == 127),
            "alpha is the strength"
        );
    }

    #[test]
    fn the_outline_runs_round_the_box() {
        let r = Rect::new(0.0, 0.0, 100.0, 50.0);
        let pts = rounded_outline(r, 10.0, 4);
        assert_eq!(pts.len(), 20);
        assert!(
            pts.iter()
                .all(|p| p.x >= -1e-3 && p.x <= 100.001 && p.y >= -1e-3 && p.y <= 50.001)
        );
        assert!(
            (pts[0] - vec2(0.0, 10.0)).length() < 1e-3,
            "starts at the top-left corner's left end"
        );
    }

    #[test]
    fn a_vertical_gradient_runs_top_to_bottom() {
        let r = Rect::new(0.0, 0.0, 10.0, 100.0);
        let g = Gradient::Vertical {
            top: BLACK,
            bottom: WHITE,
        };
        assert_eq!(g.at(r, vec2(5.0, 0.0), true), BLACK);
        assert_eq!(g.at(r, vec2(5.0, 100.0), true), WHITE);
        assert!((g.at(r, vec2(5.0, 50.0), true).r - 0.5).abs() < 1e-5);
    }
}
