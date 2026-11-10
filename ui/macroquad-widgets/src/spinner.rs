//! `Spinner` — work under way, as a ring: a comet chasing itself while how long is unknown, or an arc filling from twelve o'clock when the fraction is known.
//!
//! Stateless: it reads the painter's clock, so a capture on a fixed clock draws
//! the same frame every run. A host with a sentence to say alongside it wants
//! [`crate::Message::busy`] or [`crate::Message::progress`], which stack the ring
//! over the text as one block.
//!
//! The motion is the gamekit `Preloader`'s: the head turns steadily, the length
//! breathes, the tail fades out. This draws it as one mesh with per-vertex colour
//! rather than a run of translucent lines, for two reasons. Lines that overlap at
//! their joints double their alpha, which beads a fading tail. And macroquad
//! fills without anti-aliasing, so at this size a hard edge stair-steps; each
//! band here carries its own feather instead.

use std::f32::consts::{FRAC_PI_2, TAU};

use macroquad::models::{Mesh, Vertex, draw_mesh};
use macroquad::prelude::*;

use crate::Painter;
use crate::theme::with_alpha;

/// Radians a second the comet's head turns.
const TURN_RATE: f64 = 4.2;
/// Radians a second the comet's length breathes through.
const BREATH_RATE: f64 = 2.1;
/// The comet's shortest length, in radians.
const MIN_SWEEP: f32 = 0.9;
/// The comet's longest length, in radians.
const MAX_SWEEP: f32 = 2.5;
/// The ring's line as a fraction of its radius.
const WEIGHT: f32 = 0.16;
/// The thinnest line, in points, so a small ring stays legible.
const MIN_WEIGHT: f32 = 2.0;
/// The soft edge outside each band, in points.
const FEATHER: f32 = 0.8;
/// Segments in a whole turn.
const TURN_SEGMENTS: f32 = 96.0;
/// Points around a round end.
const CAP_POINTS: u16 = 16;

/// See the [module docs](self).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spinner {
    /// How far along, `0.0..=1.0`, or `None` when nobody can say.
    pub fraction: Option<f32>,
    /// The moving part's colour. `None` is the theme's accent.
    pub ink: Option<Color>,
}

/// The moving part's place at one moment, in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sweep {
    from: f32,
    length: f32,
}

impl Spinner {
    /// Work of unknown length: the comet.
    pub fn busy() -> Self {
        Self {
            fraction: None,
            ink: None,
        }
    }

    /// Work `fraction` done: the arc.
    pub fn at(fraction: f32) -> Self {
        Self {
            fraction: Some(fraction),
            ink: None,
        }
    }

    pub fn ink(mut self, ink: Color) -> Self {
        self.ink = Some(ink);
        self
    }

    /// Where the moving part is at `time` seconds.
    fn sweep(&self, time: f64) -> Sweep {
        match self.fraction {
            Some(fraction) => Sweep {
                from: -FRAC_PI_2,
                length: fraction.clamp(0.0, 1.0) * TAU,
            },
            None => {
                // Wrapped while still in f64, so a long wait keeps its precision.
                let head = (time * TURN_RATE).rem_euclid(TAU as f64) as f32 - FRAC_PI_2;
                let breath = 0.5 + 0.5 * (time * BREATH_RATE).sin() as f32;
                let length = MIN_SWEEP + (MAX_SWEEP - MIN_SWEEP) * breath;
                Sweep {
                    from: head - length,
                    length,
                }
            }
        }
    }

    /// Draw the ring centred in `rect`, as large as fits.
    pub fn show(&self, p: &Painter, rect: Rect) {
        let c = p.theme.color;
        let ink = self.ink.unwrap_or(c.accent);
        let centre = rect.center();
        let weight = (rect.w.min(rect.h) * 0.5 * WEIGHT).max(MIN_WEIGHT);
        let radius = rect.w.min(rect.h) * 0.5 - weight * 0.5 - FEATHER;
        if radius <= 0.0 {
            return;
        }
        let sweep = self.sweep(p.time());
        let mut mesh = Mesh {
            vertices: Vec::new(),
            indices: Vec::new(),
            texture: None,
        };
        band(&mut mesh, centre, radius, weight, -FRAC_PI_2, TAU, |_| {
            c.bg_highlight
        });
        let end = |angle: f32| centre + vec2(angle.cos(), angle.sin()) * radius;
        let head = sweep.from + sweep.length;
        match self.fraction {
            // Solid from end to end, and round at both.
            Some(_) if sweep.length > 0.0 => {
                band(
                    &mut mesh,
                    centre,
                    radius,
                    weight,
                    sweep.from,
                    sweep.length,
                    |_| ink,
                );
                cap(&mut mesh, end(sweep.from), weight * 0.5, ink);
                cap(&mut mesh, end(head), weight * 0.5, ink);
            }
            Some(_) => {}
            // Fading to nothing at the tail, so only the head has an end.
            None => {
                band(
                    &mut mesh,
                    centre,
                    radius,
                    weight,
                    sweep.from,
                    sweep.length,
                    |k| with_alpha(ink, ink.a * k * k),
                );
                cap(&mut mesh, end(head), weight * 0.5, ink);
            }
        }
        draw_mesh(&mesh);
    }
}

/// An arc of a ring with feathered edges, `ink` read along it from 0 to 1.
fn band(
    mesh: &mut Mesh,
    centre: Vec2,
    radius: f32,
    weight: f32,
    from: f32,
    length: f32,
    ink: impl Fn(f32) -> Color,
) {
    let (outer, inner) = (radius + weight * 0.5, radius - weight * 0.5);
    let segments = ((length / TAU * TURN_SEGMENTS).ceil() as u16).max(2);
    let base = mesh.vertices.len() as u16;
    for i in 0..=segments {
        let k = f32::from(i) / f32::from(segments);
        let angle = from + length * k;
        let dir = vec2(angle.cos(), angle.sin());
        let solid = ink(k);
        let clear = with_alpha(solid, 0.0);
        for (r, color) in [
            (outer + FEATHER, clear),
            (outer, solid),
            (inner, solid),
            ((inner - FEATHER).max(0.0), clear),
        ] {
            let at = centre + dir * r;
            mesh.vertices
                .push(Vertex::new(at.x, at.y, 0.0, 0.0, 0.0, color));
        }
    }
    for i in 0..segments {
        let row = base + i * 4;
        for across in 0..3 {
            let (a, b) = (row + across, row + across + 4);
            mesh.indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
}

/// A round end: a disc with a feathered edge.
fn cap(mesh: &mut Mesh, at: Vec2, radius: f32, ink: Color) {
    let base = mesh.vertices.len() as u16;
    mesh.vertices
        .push(Vertex::new(at.x, at.y, 0.0, 0.0, 0.0, ink));
    let clear = with_alpha(ink, 0.0);
    for i in 0..CAP_POINTS {
        let angle = f32::from(i) / f32::from(CAP_POINTS) * TAU;
        let dir = vec2(angle.cos(), angle.sin());
        for (r, color) in [(radius, ink), (radius + FEATHER, clear)] {
            let point = at + dir * r;
            mesh.vertices
                .push(Vertex::new(point.x, point.y, 0.0, 0.0, 0.0, color));
        }
    }
    let rim = |i: u16| base + 1 + (i % CAP_POINTS) * 2;
    for i in 0..CAP_POINTS {
        let (a, b) = (rim(i), rim(i + 1));
        mesh.indices
            .extend([base, a, b, a, a + 1, b, b, a + 1, b + 1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_fraction_fills_from_twelve_oclock() {
        let half = Spinner::at(0.5).sweep(123.0);
        assert_eq!(half.from, -FRAC_PI_2);
        assert_eq!(half.length, TAU * 0.5);
    }

    #[test]
    fn a_fraction_out_of_range_is_held_to_the_ring() {
        assert_eq!(Spinner::at(-1.0).sweep(0.0).length, 0.0);
        assert_eq!(Spinner::at(4.0).sweep(0.0).length, TAU);
    }

    #[test]
    fn the_comet_breathes_between_its_lengths_and_keeps_turning() {
        let spinner = Spinner::busy();
        let moments: Vec<Sweep> = (0..600)
            .map(|i| spinner.sweep(f64::from(i) / 60.0))
            .collect();
        assert!(
            moments
                .iter()
                .all(|s| (MIN_SWEEP - 1e-3..=MAX_SWEEP + 1e-3).contains(&s.length))
        );
        let head = |s: &Sweep| s.from + s.length;
        assert_ne!(head(&moments[0]), head(&moments[1]));
    }

    /// A page left open for a week still draws a turning comet, not a stuck one.
    #[test]
    fn a_long_wait_keeps_its_precision() {
        let spinner = Spinner::busy();
        let week = 7.0 * 86_400.0;
        let (a, b) = (spinner.sweep(week), spinner.sweep(week + 1.0 / 60.0));
        let step = (b.from + b.length) - (a.from + a.length);
        let step = step.rem_euclid(TAU);
        let expected = (TURN_RATE / 60.0) as f32;
        assert!((step - expected).abs() < 1e-3, "{step} against {expected}");
    }
}
