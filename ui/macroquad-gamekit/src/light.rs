//! `Lighting` — soft shadows, glows and a foil sheen, drawn by shader on a
//! quad that turns with the thing it lights.
//!
//! A card lifting off a table casts a soft shadow; a playable card can glow;
//! a held card catches the light in a moving band, the way a foil trading card
//! does. macroquad has no blur and no additive light, so each of these is a
//! small fragment shader over a quad drawn through `draw_texture_ex`, which
//! gives the rotation and scale of a [`Pose`] for free.
//!
//! Host-owned like [`crate::RoundedArt`]: create one after the window exists
//! and keep it. On a backend without GLSL it says so once and draws nothing,
//! rather than squares.

use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};
use macroquad::prelude::*;

use crate::sprite::Pose;

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

// A rounded box blurred by `Blur`: the quad is the box plus `Blur` all round.
const SOFT: &str = r#"#version 100
precision mediump float;

varying mediump vec2 uv;
varying lowp vec4 color;

uniform vec2 Size;
uniform float Radius;
uniform float Blur;

void main() {
    vec2 p = (uv - 0.5) * Size;
    vec2 half_box = Size * 0.5 - vec2(Blur);
    vec2 q = abs(p) - (half_box - vec2(Radius));
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - Radius;
    float a = 1.0 - smoothstep(-Blur, Blur, d);
    gl_FragColor = vec4(color.rgb, color.a * a);
}"#;

// A diagonal band of light, faintly prismatic, inside a rounded box.
const SHEEN: &str = r#"#version 100
precision mediump float;

varying mediump vec2 uv;
varying lowp vec4 color;

uniform vec2 Size;
uniform float Radius;
uniform float Phase;
uniform float Strength;

void main() {
    vec2 p = (uv - 0.5) * Size;
    vec2 q = abs(p) - (Size * 0.5 - vec2(Radius));
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - Radius;
    float inside = clamp(0.5 - d, 0.0, 1.0);
    float t = (uv.x + uv.y) * 0.5;
    float band = exp(-pow((t - Phase) / 0.11, 2.0));
    vec3 prism = 0.75 + 0.25 * cos(6.2832 * (t * 1.6 + vec3(0.0, 0.33, 0.67)));
    float a = band * inside * Strength;
    gl_FragColor = vec4(prism * color.rgb * a, a);
}"#;

fn material(fragment: &str, uniforms: Vec<UniformDesc>, additive: bool) -> Option<Material> {
    let blend = if additive {
        BlendState::new(
            Equation::Add,
            BlendFactor::One,
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        )
    } else {
        BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        )
    };
    match load_material(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment,
        },
        MaterialParams {
            uniforms,
            pipeline_params: PipelineParams {
                color_blend: Some(blend),
                ..Default::default()
            },
            ..Default::default()
        },
    ) {
        Ok(m) => Some(m),
        Err(e) => {
            eprintln!("macroquad-gamekit: lighting shader unavailable ({e:?}); drawing none");
            None
        }
    }
}

/// Host-owned: create once the window exists, keep, draw through.
pub struct Lighting {
    soft: Option<Material>,
    sheen: Option<Material>,
    white: Texture2D,
}

impl Lighting {
    pub fn new() -> Self {
        let soft = material(
            SOFT,
            vec![
                UniformDesc::new("Size", UniformType::Float2),
                UniformDesc::new("Radius", UniformType::Float1),
                UniformDesc::new("Blur", UniformType::Float1),
            ],
            false,
        );
        let sheen = material(
            SHEEN,
            vec![
                UniformDesc::new("Size", UniformType::Float2),
                UniformDesc::new("Radius", UniformType::Float1),
                UniformDesc::new("Phase", UniformType::Float1),
                UniformDesc::new("Strength", UniformType::Float1),
            ],
            true,
        );
        Self {
            soft,
            sheen,
            white: Texture2D::from_rgba8(1, 1, &[255, 255, 255, 255]),
        }
    }

    /// A quad of `size` points centred on `pose`, through `m`.
    fn quad(&self, m: &Material, pose: Pose, size: Vec2, color: Color) {
        let s = size * pose.scale;
        gl_use_material(m);
        draw_texture_ex(
            &self.white,
            pose.pos.x - s.x / 2.0,
            pose.pos.y - s.y / 2.0,
            color,
            DrawTextureParams {
                dest_size: Some(s),
                rotation: pose.rotation,
                ..Default::default()
            },
        );
        gl_use_default_material();
    }

    /// A soft rounded shadow (or, in a light colour, a glow) under something of
    /// `size` at `pose`, spread by `blur` points. Offset the pose to cast it.
    pub fn soft(&self, pose: Pose, size: Vec2, radius: f32, blur: f32, color: Color) {
        let Some(m) = &self.soft else { return };
        let padded = size + Vec2::splat(blur * 2.0);
        m.set_uniform("Size", padded);
        m.set_uniform("Radius", radius.min(size.x / 2.0).min(size.y / 2.0));
        m.set_uniform("Blur", blur.max(0.5));
        self.quad(m, pose, padded, color);
    }

    /// A foil sheen across something of `size` at `pose`: a band of light at
    /// `phase` (0 top-left corner, 1 bottom-right) with `strength` 0..1.
    pub fn sheen(&self, pose: Pose, size: Vec2, radius: f32, phase: f32, strength: f32) {
        let Some(m) = &self.sheen else { return };
        m.set_uniform("Size", size);
        m.set_uniform("Radius", radius.min(size.x / 2.0).min(size.y / 2.0));
        m.set_uniform("Phase", phase);
        m.set_uniform("Strength", strength.clamp(0.0, 1.0));
        self.quad(m, pose, size, WHITE);
    }

    /// Whether the shaders loaded on this backend.
    pub fn lit(&self) -> bool {
        self.soft.is_some() && self.sheen.is_some()
    }
}

impl Default for Lighting {
    fn default() -> Self {
        Self::new()
    }
}
