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
//! Clips nest. A list scrolled inside a page that scrolls is clipped to both:
//! the inner clip is the intersection of the two, and leaving it puts the
//! outer one back. The scissor's state cannot be read back from GL, so the
//! clips in force are kept here, on the drawing thread, the way GL keeps the
//! scissor itself. A clip is a pixel rectangle on the target it was opened
//! on, so switch render targets outside a clip, not inside one.

use std::cell::RefCell;

use macroquad::prelude::*;
use macroquad_text::report;

/// A scissor: x and y from the top-left, then width and height, in pixels.
type Scissor = (i32, i32, i32, i32);

thread_local! {
    /// The clips in force, outermost first. Mirrors GL's scissor, which is
    /// global state on the drawing thread and cannot be queried.
    static CLIPS: RefCell<Vec<Scissor>> = const { RefCell::new(Vec::new()) };
}

/// Run `f` with drawing clipped to `rect`, in the current camera's units,
/// and to every clip already in force around it. Returns what `f` returns.
pub fn with_clip<T>(rect: Rect, f: impl FnOnce() -> T) -> T {
    // The reporter records text as it is drawn, before the scissor applies, so
    // it has to know the clip too: text wholly outside it is not visible and
    // must not be reported as off its surface.
    report::push_clip(rect);
    let scissor = {
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
        let own = pixel_rect(projection, rect, w, h);
        let scissor = CLIPS.with_borrow(|clips| match clips.last() {
            Some(outer) => intersect(*outer, own),
            None => own,
        });
        gl.quad_gl.scissor(Some(scissor));
        scissor
    };
    CLIPS.with_borrow_mut(|clips| clips.push(scissor));
    let out = f();
    let outer = CLIPS.with_borrow_mut(|clips| {
        clips.pop();
        clips.last().copied()
    });
    // SAFETY: as above.
    let gl = unsafe { get_internal_gl() };
    gl.quad_gl.scissor(outer);
    report::pop_clip();
    out
}

/// The part of two scissors both cover; empty (zero width or height) where
/// they do not meet.
fn intersect(a: Scissor, b: Scissor) -> Scissor {
    let left = a.0.max(b.0);
    let top = a.1.max(b.1);
    let right = (a.0 + a.2).min(b.0 + b.2);
    let bottom = (a.1 + a.3).min(b.1 + b.3);
    (left, top, (right - left).max(0), (bottom - top).max(0))
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

    #[test]
    fn a_nested_clip_is_cut_to_the_one_around_it() {
        // A page clip, and a list inside it that pokes out of its foot.
        assert_eq!(
            intersect((0, 100, 400, 500), (20, 400, 360, 300)),
            (20, 400, 360, 200)
        );
    }

    #[test]
    fn clips_that_do_not_meet_draw_nothing() {
        let (_, _, w, h) = intersect((0, 0, 100, 100), (200, 200, 50, 50));
        assert_eq!((w, h), (0, 0));
    }
}
