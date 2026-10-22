//! `ArtTile` — NFT art in a rounded frame: a shimmer while it loads, a fade as it lands.
//!
//! A tile that sits empty reads as broken; one that shimmers reads as on its
//! way. So every state is drawn:
//!
//! - **loading**: the base colour with a soft band of light sweeping across,
//!   clipped to the rounded corners;
//! - **arrived**: the art, faded in over [`TileStyle::fade`] seconds on top
//!   of the shimmer, so it does not pop;
//! - **failed**: the base colour with a muted slash, still, so a dead image
//!   does not look like one still coming.
//!
//! Time comes from the [`ArtLoader`]'s clock (the `now` its `update` was
//! given), never from the wall clock, so a `--shot` at a fixed moment shows
//! the same shimmer phase every run.

use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};
use macroquad::prelude::*;
use macroquad_gamekit::{Ease, RoundedArt, fill_rounded};

use crate::art::{ArtLoader, ArtView};

/// Colours and timings for [`ArtTile`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileStyle {
    /// The empty tile.
    pub base: Color,
    /// The shimmer's band at its brightest.
    pub glint: Color,
    /// The slash on a tile whose art failed.
    pub failed: Color,
    /// Seconds for one shimmer sweep.
    pub period: f32,
    /// Seconds for arriving art to fade in.
    pub fade: f32,
}

impl TileStyle {
    /// A dark tile with a faint cool glint, to sit on a dark background.
    /// Pass a game's own palette instead where it has one.
    pub const DARK: Self = Self {
        base: Color::new(0.16, 0.17, 0.24, 1.0),
        glint: Color::new(0.30, 0.33, 0.45, 1.0),
        failed: Color::new(0.40, 0.42, 0.52, 1.0),
        period: 1.4,
        fade: 0.28,
    };
}

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;

varying mediump vec2 uv;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    uv = texcoord;
}"#;

// The same rounded-box distance as the gamekit's `RoundedArt`, filled with
// the base colour and a soft diagonal band whose centre moves with `Phase`.
const FRAGMENT: &str = r#"#version 100
precision mediump float;

varying mediump vec2 uv;

uniform vec2 Size;
uniform float Radius;
uniform float Phase;
uniform vec4 Base;
uniform vec4 Glint;

void main() {
    vec2 p = uv * Size - Size * 0.5;
    vec2 q = abs(p) - (Size * 0.5 - vec2(Radius));
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - Radius;
    float a = clamp(0.5 - d, 0.0, 1.0);

    float span = Size.x + Size.y * 0.5;
    float x = uv.x * Size.x + (1.0 - uv.y) * Size.y * 0.5;
    float centre = mix(-0.35 * span, 1.35 * span, Phase);
    float w = (x - centre) / (0.22 * span);
    float band = exp(-w * w);

    vec4 c = mix(Base, Glint, band);
    gl_FragColor = vec4(c.rgb, c.a * a);
}"#;

/// Draws NFT art tiles. Host-owned: create once the window exists, keep.
pub struct ArtTile {
    pub style: TileStyle,
    rounded: RoundedArt,
    shimmer: Option<Material>,
    /// A white texel, so the shimmer quad has texture coordinates 0..1.
    white: Texture2D,
}

impl ArtTile {
    pub fn new(style: TileStyle) -> Self {
        let shimmer = load_material(
            ShaderSource::Glsl {
                vertex: VERTEX,
                fragment: FRAGMENT,
            },
            MaterialParams {
                uniforms: vec![
                    UniformDesc::new("Size", UniformType::Float2),
                    UniformDesc::new("Radius", UniformType::Float1),
                    UniformDesc::new("Phase", UniformType::Float1),
                    UniformDesc::new("Base", UniformType::Float4),
                    UniformDesc::new("Glint", UniformType::Float4),
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
        let shimmer = match shimmer {
            Ok(m) => Some(m),
            Err(e) => {
                eprintln!("macroquad-cnft: shimmer shader unavailable ({e:?}); tiles load flat");
                None
            }
        };
        Self {
            style,
            rounded: RoundedArt::new(),
            shimmer,
            white: Texture2D::from_rgba8(1, 1, &[255, 255, 255, 255]),
        }
    }

    /// Draw `url`'s art filling `dest`, whatever state it is in. Does not
    /// request it: ask the loader for what is on screen first.
    pub fn draw(&self, art: &mut ArtLoader, url: &str, dest: Rect, radius: f32) {
        let now = art.now();
        match art.view(url) {
            ArtView::Loading => self.loading(dest, radius, now),
            ArtView::Failed => self.failed(dest, radius),
            ArtView::Ready { texture, age } => {
                let t = if self.style.fade > 0.0 {
                    (age / self.style.fade).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                if t < 1.0 {
                    self.loading(dest, radius, now);
                }
                let alpha = Ease::CubicOut.apply(t);
                self.rounded
                    .draw(texture, dest, radius, Color::new(1.0, 1.0, 1.0, alpha));
            }
        }
    }

    /// The loading state on its own: for a frame whose art is not even
    /// known yet, such as a tile for a page still on its way.
    pub fn loading(&self, dest: Rect, radius: f32, now: f32) {
        let s = &self.style;
        let Some(m) = &self.shimmer else {
            fill_rounded(dest, radius, s.base);
            return;
        };
        let phase = if s.period > 0.0 {
            (now / s.period).fract()
        } else {
            0.0
        };
        m.set_uniform("Size", dest.size());
        m.set_uniform("Radius", radius.min(dest.w / 2.0).min(dest.h / 2.0));
        m.set_uniform("Phase", phase);
        m.set_uniform("Base", s.base.to_vec());
        m.set_uniform("Glint", s.glint.to_vec());
        gl_use_material(m);
        draw_texture_ex(
            &self.white,
            dest.x,
            dest.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(dest.size()),
                ..Default::default()
            },
        );
        gl_use_default_material();
    }

    /// The failed state: still, with a slash, so it does not read as loading.
    pub fn failed(&self, dest: Rect, radius: f32) {
        fill_rounded(dest, radius, self.style.base);
        let c = dest.center();
        let r = dest.w.min(dest.h) * 0.12;
        let w = (r * 0.18).max(1.0);
        draw_circle_lines(c.x, c.y, r, w, self.style.failed);
        draw_line(
            c.x - r * 0.7,
            c.y + r * 0.7,
            c.x + r * 0.7,
            c.y - r * 0.7,
            w,
            self.style.failed,
        );
    }
}
