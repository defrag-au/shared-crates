//! `Preloader` — a cover over a screen's first frames, lifted once the screen
//! can draw complete.
//!
//! A screen's first frames are its worst: data may still be loading, the
//! window may still be settling, and with the [glyph gate](crate::text::enable_glyph_gate)
//! on, text is held back for a frame while its glyphs are cached. Drawn bare,
//! that is panels with no words in them. The preloader draws an opaque cover
//! with a small spinner over all of it, and fades it away the first time the
//! caller says the screen is ready, revealing a screen that is already whole.
//!
//! It lifts once and never comes back: later loading is the screen's own
//! business, drawn in its own place.

use std::f32::consts::{FRAC_PI_2, TAU};

use macroquad::prelude::*;

use crate::ease::Ease;
use crate::paint::{Gradient, fill_gradient};
use crate::tween::Animated;

/// Seconds the cover takes to fade.
const LIFT_SECONDS: f32 = 0.45;
/// Seconds before the spinner shows: a fast start shows only the colour.
const SPINNER_DELAY: f32 = 0.25;

#[derive(Clone, Debug)]
pub struct Preloader {
    cover: Animated<f32>,
    clock: f32,
    lifted: bool,
}

impl Default for Preloader {
    fn default() -> Self {
        Self::new()
    }
}

impl Preloader {
    pub fn new() -> Self {
        Self {
            cover: Animated::new(1.0),
            clock: 0.0,
            lifted: false,
        }
    }

    /// Already lifted: for captures and tests, which want the screen itself.
    pub fn lifted() -> Self {
        Self {
            cover: Animated::new(0.0),
            clock: 0.0,
            lifted: true,
        }
    }

    /// Advance by `dt`. `ready` is the caller's word that this frame drew the
    /// screen complete: its data is in and no text was held back.
    pub fn update(&mut self, dt: f32, ready: bool) {
        self.clock += dt;
        if ready && !self.lifted {
            self.lifted = true;
            self.cover.to(0.0, LIFT_SECONDS, Ease::CubicInOut);
        }
        self.cover.update(dt);
    }

    /// Whether the cover is drawn at all.
    pub fn active(&self) -> bool {
        self.cover.get() > 0.0
    }

    /// Whether it has been told the screen is ready.
    pub fn is_lifted(&self) -> bool {
        self.lifted
    }

    /// How opaque the cover is, 1 to 0.
    pub fn cover(&self) -> f32 {
        self.cover.get()
    }

    /// Draw over `area`, after the screen. `top` and `bottom` should be the
    /// screen's own background, so the lift reads as the content arriving
    /// rather than a curtain. `accent` colours the spinner.
    pub fn draw(&self, area: Rect, top: Color, bottom: Color, accent: Color) {
        let cover = self.cover.get();
        if cover <= 0.0 {
            return;
        }
        let opaque = |c: Color| Color::new(c.r, c.g, c.b, cover);
        fill_gradient(
            area,
            0.0,
            Gradient::Vertical {
                top: opaque(top),
                bottom: opaque(bottom),
            },
        );
        let appear = ((self.clock - SPINNER_DELAY) / 0.3).clamp(0.0, 1.0);
        let alpha = appear * cover * cover;
        if alpha <= 0.0 {
            return;
        }
        // An arc that chases itself: its head turns steadily while its length
        // breathes, and its tail fades into the cover.
        let centre = area.center();
        let radius = 14.0 * (0.85 + 0.15 * cover);
        // Wrapped, so a long load keeps its precision.
        let head = (self.clock * 4.2).rem_euclid(TAU) - FRAC_PI_2;
        let length = 0.9 + 1.6 * (0.5 + 0.5 * (self.clock * 2.1).sin());
        draw_circle_lines(
            centre.x,
            centre.y,
            radius,
            2.0,
            Color::new(accent.r, accent.g, accent.b, 0.12 * alpha),
        );
        const SEGMENTS: usize = 24;
        let mut previous = centre + vec2(head.cos(), head.sin()) * radius;
        for i in 1..=SEGMENTS {
            let t = i as f32 / SEGMENTS as f32;
            let angle = head - length * t;
            let p = centre + vec2(angle.cos(), angle.sin()) * radius;
            let fade = Ease::QuadIn.apply(1.0 - t);
            draw_line(
                previous.x,
                previous.y,
                p.x,
                p.y,
                2.5,
                Color::new(accent.r, accent.g, accent.b, alpha * fade),
            );
            previous = p;
        }
        let dot = centre + vec2(head.cos(), head.sin()) * radius;
        draw_circle(
            dot.x,
            dot.y,
            2.2,
            Color::new(accent.r, accent.g, accent.b, alpha),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_covers_until_ready_then_lifts_once_and_stays_lifted() {
        let mut p = Preloader::new();
        for _ in 0..30 {
            p.update(1.0 / 60.0, false);
        }
        assert_eq!(p.cover(), 1.0);
        p.update(1.0 / 60.0, true);
        assert!(p.is_lifted());
        for _ in 0..60 {
            p.update(1.0 / 60.0, false);
        }
        assert!(
            !p.active(),
            "gone, and a later 'not ready' does not bring it back"
        );
    }

    #[test]
    fn a_lifted_preloader_draws_nothing_from_the_start() {
        assert!(!Preloader::lifted().active());
    }
}
