//! `profile` — where each frame's time goes, measured inside the game, in the browser as well as natively.
//!
//! The loop marks every frame and times its phases (`update`, `tick`, `draw`,
//! `glyphs`, `present`); a game or a kit module adds its own scopes with
//! [`scope`]:
//!
//! ```ignore
//! use macroquad_gamekit::profile;
//! let placed = profile::time("wall.place", || wall.place(area, scroll));
//! let _s = profile::scope("wall.draw"); // until the end of the block
//! ```
//!
//! Off by default, and then a scope is one thread-local read. `--profile`
//! turns it on, and a capture's report gains a `## Profile` section: the
//! drawn frames' build times, the busiest drawn frame as a tree, and every
//! scope's mean and worst cost across the recent frames. So a regression is
//! found by a capture, by anyone or anything that reads the report, with no
//! panel to open. [`enable`] turns it on from code.
//!
//! The clock is miniquad's (`date::now`), the one clock every macroquad
//! target has without wasm-bindgen. Natively it resolves microseconds; in a
//! browser it is `Date.now()`, so a scope under a millisecond reads as 0 or 1
//! there, and a web profile is read across many frames, not one. puffin is
//! not used for the same reason: its web clock needs wasm-bindgen.
//!
//! Frames the capture fast-forward runs without drawing are recorded as
//! `update only`, apart from drawn frames, so the busiest drawn frame is not
//! a skipped one and a skipped one is not compared with a drawn one.

use std::cell::RefCell;
use std::fmt::Write;

use perf_probe::scopes::{Scopes, frame_text, window_text};

/// The label of a frame the loop drew.
pub const DRAWN: &str = "frame";
/// The label of a frame the capture fast-forward simulated without drawing.
pub const UPDATE_ONLY: &str = "update only";

struct State {
    on: bool,
    scopes: Scopes,
}

/// The loop's scope that waits for the display: on macOS `next_frame` idles
/// there until vsync, which is not work and must not rank a frame as busy.
pub const PRESENT: &str = "present";

impl Default for State {
    fn default() -> Self {
        Self {
            on: false,
            scopes: Scopes::new().wait_on(PRESENT),
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn now_ms() -> f64 {
    macroquad::miniquad::date::now() * 1000.0
}

/// Start recording.
pub fn enable() {
    STATE.with(|s| s.borrow_mut().on = true);
}

/// Stop recording. What was recorded stays readable.
pub fn disable() {
    STATE.with(|s| s.borrow_mut().on = false);
}

pub fn enabled() -> bool {
    STATE.with(|s| s.borrow().on)
}

/// A timed scope, from now until the guard drops. Free when profiling is off.
#[must_use = "a scope lasts until its guard drops: bind it with `let _s = …`"]
pub struct Scope {
    live: bool,
}

/// Time `name` until the returned guard drops.
pub fn scope(name: &'static str) -> Scope {
    let live = STATE.with(|s| {
        let mut s = s.borrow_mut();
        if !s.on {
            return false;
        }
        s.scopes.enter(name, now_ms());
        true
    });
    Scope { live }
}

/// Time one call inline and hand back its value:
///
/// ```ignore
/// let placed = profile::time("wall.place", || wall.place(area, scroll));
/// ```
///
/// The extent is the closure, so it cannot be dropped early by mistake (a
/// guard bound as `let _ = scope(..)` times nothing). Use [`scope`] instead
/// where a closure cannot go: across an `.await`, or around code that uses
/// `?` or `return` to leave the enclosing function.
#[inline]
pub fn time<T>(name: &'static str, f: impl FnOnce() -> T) -> T {
    let _s = scope(name);
    f()
}

impl Drop for Scope {
    fn drop(&mut self) {
        if self.live {
            STATE.with(|s| s.borrow_mut().scopes.exit(now_ms()));
        }
    }
}

/// The loop starts a frame: [`DRAWN`] or [`UPDATE_ONLY`].
pub(crate) fn begin_frame(label: &'static str) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.on {
            s.scopes.begin_frame(label, now_ms());
        }
    });
}

/// The loop finishes a frame.
pub(crate) fn end_frame() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.on {
            s.scopes.end_frame(now_ms());
        }
    });
}

/// The `## Profile` section of a capture report, or nothing when off.
pub fn report() -> String {
    STATE.with(|s| {
        let s = s.borrow();
        if !s.on {
            return String::new();
        }
        render(&s.scopes, Some(macroquad::telemetry::textures_count()))
    })
}

/// `textures` is macroquad's live texture count, read from its context, so
/// `None` where there is none (a test).
fn render(scopes: &Scopes, textures: Option<usize>) -> String {
    let mut out = String::from("\n## Profile\n\n");
    let mut drawn: Vec<f64> = scopes
        .frames()
        .filter(|f| f.label == DRAWN)
        .map(|f| f.work_ms())
        .collect();
    let waited: Vec<f64> = scopes
        .frames()
        .filter(|f| f.label == DRAWN)
        .map(|f| f.wait_ms)
        .collect();
    let skipped = scopes.frames().filter(|f| f.label == UPDATE_ONLY).count();
    drawn.sort_by(|a, b| a.total_cmp(b));
    let at = |q: f64| {
        drawn
            .get(((drawn.len().saturating_sub(1)) as f64 * q).round() as usize)
            .copied()
            .unwrap_or(0.0)
    };
    let mean_wait = waited.iter().sum::<f64>() / waited.len().max(1) as f64;
    let _ = writeln!(
        out,
        "{} drawn frames recorded, {skipped} update-only. Drawn frame work: p50 {:.2} ms, \
         p95 {:.2} ms, worst {:.2} ms (60 Hz allows 16.67). Waiting in `{PRESENT}` besides: \
         {mean_wait:.2} ms a frame on average.{}",
        drawn.len(),
        at(0.5),
        at(0.95),
        drawn.last().copied().unwrap_or(0.0),
        textures.map_or(String::new(), |n| format!(" Textures alive: {n}."))
    );
    let _ = writeln!(
        out,
        "Times are wall clock (a capture runs on a fixed simulation clock, but its work is \
         timed for real). The capture's own frame is not among them."
    );
    match scopes.busiest(Some(DRAWN)) {
        Some(frame) => {
            let _ = writeln!(out, "\n### Busiest drawn frame\n");
            out.push_str(&frame_text(frame));
        }
        None => {
            let _ = writeln!(out, "\nNo drawn frame was recorded.");
        }
    }
    let window = scopes.window(Some(DRAWN));
    if !window.is_empty() {
        let _ = writeln!(out, "\n### Every scope, across the drawn frames\n");
        out.push_str(&window_text(&window, drawn.len()));
    }
    if let Some(frame) = scopes.busiest(Some(UPDATE_ONLY)) {
        let _ = writeln!(out, "\n### Busiest update-only frame\n");
        out.push_str(&frame_text(frame));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_separates_drawn_frames_from_skipped_ones() {
        let mut s = Scopes::new();
        s.begin_frame(UPDATE_ONLY, 0.0);
        s.enter("update", 0.0);
        s.exit(30.0);
        s.end_frame(30.0);
        for k in 0..4 {
            let t = 100.0 + f64::from(k) * 20.0;
            s.begin_frame(DRAWN, t);
            s.enter("draw", t);
            s.exit(t + 2.0 + f64::from(k));
            s.end_frame(t + 3.0 + f64::from(k));
        }
        let text = render(&s, None);
        assert!(
            text.contains("4 drawn frames recorded, 1 update-only"),
            "{text}"
        );
        assert!(text.contains("worst 6.00 ms"), "{text}");
        assert!(text.contains("### Busiest drawn frame"), "{text}");
        assert!(text.contains("frame 4 (frame) · 6.00 ms"), "{text}");
        assert!(text.contains("### Busiest update-only frame"), "{text}");
    }

    #[test]
    fn scopes_cost_nothing_and_record_nothing_when_off() {
        assert!(!enabled());
        let guard = scope("anything");
        assert!(!guard.live);
        assert_eq!(report(), "");
        assert_eq!(time("inline", || 2 + 2), 4, "the value comes back");
    }
}
