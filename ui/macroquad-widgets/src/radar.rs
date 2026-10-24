//! `Radar` — a round scope: range rings, a turning sweep, blips that flare as the sweep passes and fade after, and an optional gauge arc around the rim.
//!
//! Stateless, like every widget here: the host passes the blips and a time
//! (through [`Painter::at_time`](crate::Painter::at_time)), and the sweep's
//! angle is computed from it, so a capture at an exact moment is the same
//! scope every run.
//!
//! The form, for a reader who glances at it:
//!
//! - **Range is distance from the centre**, 0 at the middle, 1 at the rim.
//!   What the range *means* is the host's: BLOCKWATCH puts a block's age there,
//!   so blips drift outward as blocks get older.
//! - **Bearing is a quarter-turn-free fraction**: 0 is straight up, 0.25 is
//!   right, clockwise, so a host derives it from a hash or an index without
//!   thinking in radians.
//! - **A blip flares when the sweep crosses it** and fades over the rest of
//!   the turn. That is what makes a scope read as live.
//! - **The gauge** is a rim arc filled clockwise from the top: a probability,
//!   a countdown. The track is drawn under it so an empty gauge still shows
//!   its scale.
//!
//! Text is the host's: a radar has no label of its own, so it never fights
//! the layout around it.

use std::f32::consts::TAU;

use macroquad::prelude::*;

use crate::Painter;
use crate::theme::with_alpha;

/// One contact on the scope.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blip {
    /// 0 is up, clockwise, in turns (0 to 1).
    pub bearing: f32,
    /// 0 at the centre, 1 at the rim.
    pub range: f32,
    /// 1 is a full blip; less draws it smaller and dimmer (an old contact).
    pub strength: f32,
}

/// What the radar draws.
#[derive(Clone, Copy, Debug)]
pub struct RadarVm<'a> {
    pub blips: &'a [Blip],
    /// The rim gauge, 0 to 1, if any.
    pub gauge: Option<f32>,
    /// Seconds per turn of the sweep.
    pub sweep_secs: f32,
    /// The scope's ink; `None` is the theme's accent.
    pub ink: Option<Color>,
}

impl<'a> RadarVm<'a> {
    pub fn new(blips: &'a [Blip]) -> Self {
        Self {
            blips,
            gauge: None,
            sweep_secs: 4.0,
            ink: None,
        }
    }

    pub fn gauge(mut self, fill: f32) -> Self {
        self.gauge = Some(fill.clamp(0.0, 1.0));
        self
    }

    pub fn sweep_secs(mut self, secs: f32) -> Self {
        self.sweep_secs = secs.max(0.1);
        self
    }

    pub fn ink(mut self, color: Color) -> Self {
        self.ink = Some(color);
        self
    }
}

/// Where the sweep points at `t`, in turns (0 up, clockwise).
pub fn sweep_bearing(t: f64, sweep_secs: f32) -> f32 {
    (t / sweep_secs.max(0.1) as f64).rem_euclid(1.0) as f32
}

/// How far behind the sweep `bearing` is, in turns (0 just swept, near 1 about
/// to be).
pub fn behind_sweep(bearing: f32, sweep: f32) -> f32 {
    (sweep - bearing).rem_euclid(1.0)
}

/// A bearing in turns as a screen direction (y down).
fn dir(bearing: f32) -> Vec2 {
    let a = bearing * TAU;
    vec2(a.sin(), -a.cos())
}

/// Draw the scope centred on `centre`, radius `r`.
pub fn radar(p: &Painter, vm: &RadarVm, centre: Vec2, r: f32) {
    let ink = vm.ink.unwrap_or(p.theme.color.accent);
    let sweep = sweep_bearing(p.time(), vm.sweep_secs);

    // The glass, and the rings and crosshair etched on it.
    draw_circle(centre.x, centre.y, r, with_alpha(ink, 0.06));
    for ring in [1.0 / 3.0, 2.0 / 3.0] {
        draw_circle_lines(centre.x, centre.y, r * ring, 1.0, with_alpha(ink, 0.16));
    }
    draw_circle_lines(centre.x, centre.y, r, 1.0, with_alpha(ink, 0.35));
    draw_line(
        centre.x - r,
        centre.y,
        centre.x + r,
        centre.y,
        1.0,
        with_alpha(ink, 0.12),
    );
    draw_line(
        centre.x,
        centre.y - r,
        centre.x,
        centre.y + r,
        1.0,
        with_alpha(ink, 0.12),
    );

    // The sweep: a wedge fading behind the line.
    const TRAIL: f32 = 0.18;
    const SLICES: usize = 14;
    for i in 0..SLICES {
        let a0 = sweep - TRAIL * i as f32 / SLICES as f32;
        let a1 = sweep - TRAIL * (i + 1) as f32 / SLICES as f32;
        let fade = 1.0 - i as f32 / SLICES as f32;
        draw_triangle(
            centre,
            centre + dir(a0) * r,
            centre + dir(a1) * r,
            with_alpha(ink, 0.22 * fade * fade),
        );
    }
    let tip = centre + dir(sweep) * r;
    draw_line(centre.x, centre.y, tip.x, tip.y, 1.5, with_alpha(ink, 0.9));

    // Blips: bright just after the sweep, fading over the turn.
    for b in vm.blips {
        let at = centre + dir(b.bearing) * r * b.range.clamp(0.0, 1.0);
        let behind = behind_sweep(b.bearing, sweep);
        let flare = (1.0 - behind).powi(2);
        let s = b.strength.clamp(0.0, 1.0);
        let size = 1.2 + 1.6 * s;
        draw_circle(
            at.x,
            at.y,
            size * (1.6 + flare),
            with_alpha(ink, 0.15 * flare * s),
        );
        draw_circle(
            at.x,
            at.y,
            size,
            with_alpha(ink, (0.25 + 0.75 * flare) * (0.4 + 0.6 * s)),
        );
    }

    // The rim gauge: a track, then the fill clockwise from the top. `draw_arc`
    // grows its thickness outward, so it sits just outside the rim.
    if let Some(fill) = vm.gauge {
        draw_arc(
            centre.x,
            centre.y,
            48,
            r + 2.0,
            -90.0,
            3.0,
            360.0,
            with_alpha(ink, 0.12),
        );
        if fill > 0.0 {
            draw_arc(
                centre.x,
                centre.y,
                48,
                r + 2.0,
                -90.0,
                3.0,
                360.0 * fill,
                ink,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sweep_turns_once_per_period_from_the_top() {
        assert_eq!(sweep_bearing(0.0, 4.0), 0.0);
        assert!((sweep_bearing(1.0, 4.0) - 0.25).abs() < 1e-6);
        assert!((sweep_bearing(5.0, 4.0) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn a_blip_just_swept_is_barely_behind() {
        assert!(behind_sweep(0.24, 0.25) < 0.02);
        assert!(behind_sweep(0.26, 0.25) > 0.98, "about to be swept");
    }

    #[test]
    fn bearings_run_clockwise_from_up_in_screen_space() {
        let up = dir(0.0);
        let right = dir(0.25);
        assert!(up.y < -0.99 && up.x.abs() < 1e-6);
        assert!(right.x > 0.99 && right.y.abs() < 1e-5);
    }
}
