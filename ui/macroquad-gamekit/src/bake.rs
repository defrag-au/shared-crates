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
//! Blending into a transparent target darkens partly transparent edges
//! slightly. Faces with an opaque background (every card) show it only at
//! their rounded corners.

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

impl Baked {
    /// Render `draw` into a new texture of `size` points at `pixel_scale`
    /// device pixels per point. `draw` works in points with the origin at
    /// the top-left, and is handed the pixel scale for crisp text.
    pub fn render(size: Vec2, pixel_scale: f32, draw: impl FnOnce(f32)) -> Baked {
        let k = pixel_scale.max(0.1);
        let w = (size.x * k).ceil().max(1.0) as u32;
        let h = (size.y * k).ceil().max(1.0) as u32;
        let target = render_target(w, h);
        target.texture.set_filter(FilterMode::Linear);
        let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, size.x, size.y));
        cam.render_target = Some(target.clone());
        push_camera_state();
        set_camera(&cam);
        clear_background(Color::new(0.0, 0.0, 0.0, 0.0));
        draw(k);
        pop_camera_state();
        Baked { target, size }
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
