//! `Baked` — draw something once into a texture, then move, turn and fade the
//! texture: how a card face is drawn every frame without laying out its text
//! every frame.
//!
//! A card's face (frame, art, cost, wrapped text) changes rarely but is drawn
//! sixty times a second, rotated in a fan. Baking it at the device's pixel
//! scale keeps the text sharp; drawing the texture through a [`Pose`] gives
//! rotation, scale and alpha for free, which immediate-mode text cannot do.
//!
//! The bake saves and restores the camera, so it can run mid-frame, inside
//! another render target (the storybook's `--shot` draws into one).
//!
//! ## Layered art needs a [`Baker`]
//!
//! macroquad blends alpha with the same factors as colour, so every partly
//! transparent layer drawn into a render target *lowers the target's alpha*:
//! an opaque card with grain, an inner shadow and a few translucent strokes on
//! it comes out a little see-through, and the card behind shows through it.
//! The colour is right; only the alpha is worn down. [`Baker::render`] bakes,
//! then copies the result through a shader with blending off that restores the
//! opacity, so layered faces stay solid. [`Baked::render`] is the plain bake,
//! fine for a face of opaque layers only.

use macroquad::miniquad::PipelineParams;
use macroquad::prelude::*;

use crate::sprite::Pose;

pub struct Baked {
    /// Held, not just its texture: dropping a `RenderTarget` frees the GL
    /// texture behind it, and the next texture allocated reuses the id, so a
    /// face kept only by its `Texture2D` turns into whatever is drawn next.
    target: RenderTarget,
    /// Size in points; the texture is this times the bake's pixel scale.
    pub size: Vec2,
}

/// A render target of `size` points at `k` pixels per point, drawn into by
/// `draw` with the origin top-left, the camera restored afterwards.
fn bake_into(size: Vec2, k: f32, draw: impl FnOnce()) -> RenderTarget {
    let w = (size.x * k).ceil().max(1.0) as u32;
    let h = (size.y * k).ceil().max(1.0) as u32;
    let target = render_target(w, h);
    target.texture.set_filter(FilterMode::Linear);
    let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, size.x, size.y));
    cam.render_target = Some(target.clone());
    push_camera_state();
    set_camera(&cam);
    clear_background(Color::new(0.0, 0.0, 0.0, 0.0));
    draw();
    pop_camera_state();
    target
}

impl Baked {
    /// Render `draw` into a new texture of `size` points at `pixel_scale`
    /// device pixels per point. `draw` works in points with the origin at
    /// the top-left, and is handed the pixel scale for crisp text.
    pub fn render(size: Vec2, pixel_scale: f32, draw: impl FnOnce(f32)) -> Baked {
        let k = pixel_scale.max(0.1);
        Baked {
            target: bake_into(size, k, || draw(k)),
            size,
        }
    }

    pub fn texture(&self) -> &Texture2D {
        &self.target.texture
    }

    /// Draw centred on `pose.pos`, turned, scaled and faded by the pose.
    pub fn draw(&self, pose: Pose, tint: Color) {
        let s = self.size * pose.scale;
        draw_texture_ex(
            &self.target.texture,
            pose.pos.x - s.x / 2.0,
            pose.pos.y - s.y / 2.0,
            Color::new(tint.r, tint.g, tint.b, tint.a * pose.alpha),
            DrawTextureParams {
                dest_size: Some(s),
                rotation: pose.rotation,
                // A render target made through `from_display_rect` comes out
                // upside down when sampled as a texture.
                flip_y: true,
                ..Default::default()
            },
        );
    }
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

// Alpha worn down by layering comes back to full; the antialiased rim (alpha
// well below one) keeps a soft edge.
const SEAL: &str = r#"#version 100
precision mediump float;

varying mediump vec2 uv;

uniform sampler2D Texture;

void main() {
    vec4 c = texture2D(Texture, uv);
    gl_FragColor = vec4(c.rgb, clamp(c.a * 1.6, 0.0, 1.0));
}"#;

/// Host-owned: bakes layered faces that stay opaque. Create once the window
/// exists and keep it; cloning shares the shader.
#[derive(Clone)]
pub struct Baker {
    seal: Option<Material>,
}

impl Baker {
    pub fn new() -> Self {
        let seal = load_material(
            ShaderSource::Glsl {
                vertex: VERTEX,
                fragment: SEAL,
            },
            MaterialParams {
                // No blending: the copy writes colour and alpha as computed.
                pipeline_params: PipelineParams::default(),
                ..Default::default()
            },
        );
        let seal = match seal {
            Ok(m) => Some(m),
            Err(e) => {
                eprintln!(
                    "macroquad-gamekit: bake seal shader unavailable ({e:?}); layered faces may show through"
                );
                None
            }
        };
        Self { seal }
    }

    /// [`Baked::render`], then sealed so translucent layers do not leave the
    /// face see-through.
    ///
    /// `draw` runs twice. macroquad caches glyphs in one atlas texture and,
    /// when a new size does not fit, doubles the atlas and deletes the old
    /// texture, so text already queued in the same batch draws from a dead
    /// texture and vanishes. A big face at a new pixel scale (a zoomed detail
    /// card) hits that on its first text. The first pass caches every glyph
    /// the face needs; clearing drops its queued draws; the second pass draws
    /// against an atlas that no longer moves.
    pub fn render(&self, size: Vec2, pixel_scale: f32, draw: impl Fn(f32)) -> Baked {
        let raw = Baked::render(size, pixel_scale, |k| {
            draw(k);
            clear_background(Color::new(0.0, 0.0, 0.0, 0.0));
            draw(k);
        });
        let Some(seal) = &self.seal else { return raw };
        let k = pixel_scale.max(0.1);
        let target = bake_into(size, k, || {
            gl_use_material(seal);
            draw_texture_ex(
                raw.texture(),
                0.0,
                0.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(size),
                    // Upright in the copy, so the copy flips like any bake.
                    flip_y: true,
                    ..Default::default()
                },
            );
            gl_use_default_material();
        });
        Baked { target, size }
    }
}

impl Default for Baker {
    fn default() -> Self {
        Self::new()
    }
}
