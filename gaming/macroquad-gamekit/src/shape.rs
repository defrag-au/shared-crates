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

/// The rounded fills, outlines and clipped art live in the widgets, where the
/// sheet chrome draws with them too; the kit's callers keep reaching them here.
pub use macroquad_widgets::shape::{
    cover_source, fill_rounded, fill_texture_rounded, stroke_rounded,
};

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
