//! `RoundedArt` — art cropped to fill a frame and clipped to its rounded
//! corners, plus the rounded outline every card and badge needs.
//!
//! macroquad has no clipping or masking, so a picture in a rounded card frame
//! is either square-cornered or drawn through a shader. [`RoundedArt`] holds
//! that shader: the host creates one after the window exists, keeps it, and
//! draws through it. Where the shader cannot load (a backend without GLSL), it
//! says so once and draws square corners instead, rather than nothing.
//!
//! [`cover_source`] is the crop: the largest part of the image with the
//! frame's shape, centred, so art fills the frame without stretching.

use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};
use macroquad::prelude::*;

/// Fill a rounded rectangle.
pub fn fill_rounded(rect: Rect, radius: f32, color: Color) {
    macroquad_widgets::draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, radius, color);
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

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;

varying mediump vec2 uv;
varying lowp vec4 color;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}"#;

// The rounded box's signed distance, from the quad's own texture coordinates:
// `Crop` maps the cropped source back to 0..1 across the frame.
const FRAGMENT: &str = r#"#version 100
precision mediump float;

varying mediump vec2 uv;
varying lowp vec4 color;

uniform sampler2D Texture;
uniform vec2 Size;
uniform float Radius;
uniform vec4 Crop;

void main() {
    vec2 local = (uv - Crop.xy) / Crop.zw;
    vec2 p = local * Size - Size * 0.5;
    vec2 q = abs(p) - (Size * 0.5 - vec2(Radius));
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - Radius;
    float a = clamp(0.5 - d, 0.0, 1.0);
    vec4 c = texture2D(Texture, uv) * color;
    gl_FragColor = vec4(c.rgb, c.a * a);
}"#;

/// Host-owned: create once the window exists, keep, draw through.
pub struct RoundedArt {
    material: Option<Material>,
}

impl RoundedArt {
    pub fn new() -> Self {
        let material = load_material(
            ShaderSource::Glsl {
                vertex: VERTEX,
                fragment: FRAGMENT,
            },
            MaterialParams {
                uniforms: vec![
                    UniformDesc::new("Size", UniformType::Float2),
                    UniformDesc::new("Radius", UniformType::Float1),
                    UniformDesc::new("Crop", UniformType::Float4),
                ],
                pipeline_params: PipelineParams {
                    color_blend: Some(BlendState::new(
                        Equation::Add,
                        BlendFactor::Value(BlendValue::SourceAlpha),
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    )),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let material = match material {
            Ok(m) => Some(m),
            Err(e) => {
                eprintln!(
                    "macroquad-gamekit: rounded art shader unavailable ({e:?}); corners will be square"
                );
                None
            }
        };
        Self { material }
    }

    /// Whether corners are actually rounded on this backend.
    pub fn rounds(&self) -> bool {
        self.material.is_some()
    }

    /// Draw `texture` filling `dest`, cropped to its shape and clipped to
    /// rounded corners of `radius`.
    pub fn draw(&self, texture: &Texture2D, dest: Rect, radius: f32, tint: Color) {
        let tex = vec2(texture.width(), texture.height());
        let src = cover_source(tex, dest.size());
        let params = DrawTextureParams {
            dest_size: Some(dest.size()),
            source: Some(src),
            ..Default::default()
        };
        let Some(m) = &self.material else {
            draw_texture_ex(texture, dest.x, dest.y, tint, params);
            return;
        };
        m.set_uniform("Size", dest.size());
        m.set_uniform("Radius", radius.min(dest.w / 2.0).min(dest.h / 2.0));
        m.set_uniform(
            "Crop",
            vec4(src.x / tex.x, src.y / tex.y, src.w / tex.x, src.h / tex.y),
        );
        gl_use_material(m);
        draw_texture_ex(texture, dest.x, dest.y, tint, params);
        gl_use_default_material();
    }
}

impl Default for RoundedArt {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
