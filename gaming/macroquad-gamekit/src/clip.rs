//! `with_clip` — draw inside a rectangle and nowhere else, under whatever camera is set.
//!
//! macroquad's scissor is in the current target's pixels, so a caller drawing
//! in stage units (a gamekit stage, a storybook panel, a capture's render
//! target) would have to know the camera, the DPI and whether the target is a
//! texture. This takes a rectangle in the units being drawn in and works the
//! rest out from the current projection: the corners go to normalised device
//! coordinates, and from there to the active target's pixels.
//!
//! A scrolled view is the reason it exists: without it, rows scrolled out of
//! the top of a wall draw over whatever sits above it, unless every host
//! remembers to paint over them.
//!
//! Clips do not nest: the scissor's current state cannot be read back, so on
//! leaving, the clip is cleared rather than restored.

use macroquad::prelude::*;

/// Run `f` with drawing clipped to `rect`, in the current camera's units.
/// Returns what `f` returns.
pub fn with_clip<T>(rect: Rect, f: impl FnOnce() -> T) -> T {
    {
        // SAFETY: macroquad's own escape hatch to its GL state; used and
        // dropped here on the drawing thread, between draws.
        let gl = unsafe { get_internal_gl() };
        let (w, h) = match gl.quad_gl.get_active_render_pass() {
            Some(pass) => {
                let texture = gl.quad_context.render_pass_texture(pass);
                gl.quad_context.texture_size(texture)
            }
            None => {
                let (w, h) = macroquad::miniquad::window::screen_size();
                (w as u32, h as u32)
            }
        };
        let projection = gl.quad_gl.get_projection_matrix();
        gl.quad_gl.scissor(Some(pixel_rect(projection, rect, w, h)));
    }
    let out = f();
    // SAFETY: as above.
    let gl = unsafe { get_internal_gl() };
    gl.quad_gl.scissor(None);
    out
}

/// `rect` through `projection`, as a scissor on a `w`×`h` pixel target:
/// x and y from the top-left, as macroquad's scissor takes them.
pub fn pixel_rect(projection: Mat4, rect: Rect, w: u32, h: u32) -> (i32, i32, i32, i32) {
    let corners = [
        vec3(rect.x, rect.y, 0.0),
        vec3(rect.right(), rect.y, 0.0),
        vec3(rect.x, rect.bottom(), 0.0),
        vec3(rect.right(), rect.bottom(), 0.0),
    ]
    .map(|c| projection.project_point3(c));
    let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for c in corners {
        x0 = x0.min(c.x);
        x1 = x1.max(c.x);
        y0 = y0.min(c.y);
        y1 = y1.max(c.y);
    }
    let (wf, hf) = (w as f32, h as f32);
    // Each edge to the nearest pixel: the projection's float error puts an
    // exact edge a hair either side, and floor/ceil would grow the clip by a
    // pixel at random.
    let left = ((x0 + 1.0) * 0.5 * wf).round().clamp(0.0, wf);
    let right = ((x1 + 1.0) * 0.5 * wf).round().clamp(0.0, wf);
    // GL's y runs up from the bottom; the scissor wants it from the top.
    let top = ((1.0 - y1) * 0.5 * hf).round().clamp(0.0, hf);
    let bottom = ((1.0 - y0) * 0.5 * hf).round().clamp(0.0, hf);
    (
        left as i32,
        top as i32,
        (right - left) as i32,
        (bottom - top) as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A y-down camera over a 400×300 unit stage, as macroquad's default
    /// and the gamekit's window camera set up.
    fn y_down(w: f32, h: f32) -> Mat4 {
        Mat4::orthographic_rh_gl(0.0, w, h, 0.0, -1.0, 1.0)
    }

    #[test]
    fn a_rect_maps_to_the_same_place_in_pixels_at_one_to_one() {
        let p = y_down(400.0, 300.0);
        assert_eq!(
            pixel_rect(p, Rect::new(10.0, 20.0, 100.0, 50.0), 400, 300),
            (10, 20, 100, 50)
        );
    }

    #[test]
    fn a_high_dpi_target_scales_the_scissor() {
        // Units are points; the framebuffer has two pixels a point.
        let p = y_down(400.0, 300.0);
        assert_eq!(
            pixel_rect(p, Rect::new(10.0, 20.0, 100.0, 50.0), 800, 600),
            (20, 40, 200, 100)
        );
    }

    #[test]
    fn a_rect_partly_off_the_target_is_cut_to_it() {
        let p = y_down(400.0, 300.0);
        assert_eq!(
            pixel_rect(p, Rect::new(-50.0, 250.0, 100.0, 100.0), 400, 300),
            (0, 250, 50, 50)
        );
    }
}
