//! Rounded shapes and gradient fills: the surfaces every card, sheet, badge and
//! pill is drawn from.
//!
//! macroquad fills a shape with one colour and has no clipping. These build
//! what a card needs from meshes with per-vertex colour: a rounded fill, a
//! rounded outline, art clipped to rounded corners, and a gradient across a
//! rounded plate.
//!
//! They live here, under the widgets, so the sheet chrome
//! ([`crate::sheet_card`]) and the gamekit draw with the same hand. The gamekit
//! re-exports them from its `shape` and `paint` modules, so its callers did not
//! move.

use macroquad::models::{Mesh, Vertex, draw_mesh};
use macroquad::prelude::*;

/// Fill a rounded rectangle.
pub fn fill_rounded(rect: Rect, radius: f32, color: Color) {
    crate::draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, radius, color);
}

/// Outline a rounded rectangle, the line centred on the edge.
pub fn stroke_rounded(rect: Rect, radius: f32, thickness: f32, color: Color) {
    let r = radius.min(rect.w / 2.0).min(rect.h / 2.0).max(0.0);
    let (x0, y0, x1, y1) = (rect.x, rect.y, rect.x + rect.w, rect.y + rect.h);
    draw_line(x0 + r, y0, x1 - r, y0, thickness, color);
    draw_line(x0 + r, y1, x1 - r, y1, thickness, color);
    draw_line(x0, y0 + r, x0, y1 - r, thickness, color);
    draw_line(x1, y0 + r, x1, y1 - r, thickness, color);
    if r > 0.0 {
        // draw_arc takes degrees, clockwise from +x in screen space.
        for (cx, cy, start) in [
            (x1 - r, y0 + r, 270.0),
            (x1 - r, y1 - r, 0.0),
            (x0 + r, y1 - r, 90.0),
            (x0 + r, y0 + r, 180.0),
        ] {
            // `sides` is per full circle; 48 keeps a quarter smooth at card size.
            // The arc grows outward from its radius, so start half a line in.
            draw_arc(
                cx,
                cy,
                48,
                r - thickness / 2.0,
                start,
                thickness,
                90.0,
                color,
            );
        }
    }
}

/// `texture` filling `rect` — cropped to its shape as [`cover_source`] crops,
/// and clipped to its rounded corners (a circle, when `radius` is half its
/// side) — tinted by `tint`.
///
/// The clip is geometry rather than a shader: a fan over the rounded outline
/// with each vertex's texture coordinate read off the crop. So it needs
/// nothing made after the window exists, and suits a picture drawn anywhere.
pub fn fill_texture_rounded(texture: &Texture2D, rect: Rect, radius: f32, tint: Color) {
    let size = texture.size();
    let src = cover_source(size, vec2(rect.w, rect.h));
    let uv = |p: Vec2| {
        let fx = ((p.x - rect.x) / rect.w.max(1e-3)).clamp(0.0, 1.0);
        let fy = ((p.y - rect.y) / rect.h.max(1e-3)).clamp(0.0, 1.0);
        vec2(
            (src.x + fx * src.w) / size.x.max(1.0),
            (src.y + fy * src.h) / size.y.max(1.0),
        )
    };
    let outline = rounded_outline(rect, radius, 12);
    let centre = rect.center();
    let mut vertices = Vec::with_capacity(outline.len() + 1);
    let c = uv(centre);
    vertices.push(Vertex::new(centre.x, centre.y, 0.0, c.x, c.y, tint));
    for p in &outline {
        let t = uv(*p);
        vertices.push(Vertex::new(p.x, p.y, 0.0, t.x, t.y, tint));
    }
    let n = outline.len() as u16;
    let mut indices = Vec::with_capacity(outline.len() * 3);
    for i in 0..n {
        indices.extend([0, 1 + i, 1 + (i + 1) % n]);
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: Some(texture.clone()),
    });
}

/// The part of a `texture_size` image, in pixels, that fills a frame of
/// `frame` aspect without stretching: as much as fits, centred.
pub fn cover_source(texture_size: Vec2, frame: Vec2) -> Rect {
    if texture_size.x <= 0.0 || texture_size.y <= 0.0 || frame.x <= 0.0 || frame.y <= 0.0 {
        return Rect::new(0.0, 0.0, texture_size.x, texture_size.y);
    }
    let want = frame.x / frame.y;
    let have = texture_size.x / texture_size.y;
    if have > want {
        let w = texture_size.y * want;
        Rect::new((texture_size.x - w) / 2.0, 0.0, w, texture_size.y)
    } else {
        let h = texture_size.x / want;
        Rect::new(0.0, (texture_size.y - h) / 2.0, texture_size.x, h)
    }
}

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
            Gradient::Vertical { top, bottom } => mix(
                top,
                bottom,
                ((p.y - rect.y) / rect.h.max(1e-3)).clamp(0.0, 1.0),
            ),
            Gradient::Horizontal { left, right } => mix(
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

/// `a` towards `b` by `t`, every channel including alpha.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

/// Points around a rounded rectangle, clockwise from the top-left corner's
/// start, `segments` per corner.
pub fn rounded_outline(rect: Rect, radius: f32, segments: usize) -> Vec<Vec2> {
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_wide_image_in_a_tall_frame_is_cropped_at_the_sides() {
        let r = cover_source(vec2(400.0, 200.0), vec2(100.0, 100.0));
        assert_eq!(r, Rect::new(100.0, 0.0, 200.0, 200.0));
    }

    #[test]
    fn a_tall_image_in_a_wide_frame_is_cropped_top_and_bottom() {
        let r = cover_source(vec2(200.0, 400.0), vec2(200.0, 100.0));
        assert_eq!(r, Rect::new(0.0, 150.0, 200.0, 100.0));
    }

    #[test]
    fn a_matching_shape_uses_the_whole_image() {
        let r = cover_source(vec2(300.0, 150.0), vec2(60.0, 30.0));
        assert_eq!(r, Rect::new(0.0, 0.0, 300.0, 150.0));
    }

    #[test]
    fn a_degenerate_frame_uses_the_whole_image() {
        let r = cover_source(vec2(300.0, 150.0), vec2(0.0, 30.0));
        assert_eq!(r, Rect::new(0.0, 0.0, 300.0, 150.0));
    }
}
