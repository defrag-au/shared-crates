//! `Starfield` — a seeded sky of stars in depth layers that drift and twinkle, drawn behind a space scene, a title or a whole window.
//!
//! Stars are placed once from a seed, as fractions of whatever rect they are
//! drawn into, so one field fills a phone or a wide window without being
//! regenerated, and the same seed is the same sky. Each star has a depth: near
//! stars are bigger, brighter and drift faster than far ones, which is the
//! parallax that makes a flat backdrop read as space.
//!
//! Time is an argument, as everywhere in the kit: [`Starfield::draw`] takes
//! the clock, so a capture at `--at 3` is the same sky every run.
//!
//! [`Starfield::place`] is plain arithmetic and tested without a window.

use std::f32::consts::TAU;

use macroquad::prelude::*;

/// One star, in the field's own terms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Star {
    /// Where it sits at time 0, as fractions (0 to 1) of the rect.
    pub at: Vec2,
    /// 0 is the farthest layer, 1 the nearest.
    pub depth: f32,
    /// Its own twinkle offset, 0 to 1.
    pub phase: f32,
}

/// A seeded sky. Build it once; draw it every frame.
#[derive(Clone, Debug)]
pub struct Starfield {
    stars: Vec<Star>,
    /// Units a second the nearest layer moves; farther layers move less.
    pub drift: Vec2,
    /// The stars' colour; their brightness comes from depth and twinkle.
    pub tint: Color,
    /// The nearest stars' size in units; the farthest are a third of it.
    pub size: f32,
}

impl Starfield {
    /// `count` stars from `seed`, still, white, 2 units at the front.
    pub fn new(seed: u64, count: usize) -> Self {
        let mut rng = seed.max(1);
        let mut unit = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng >> 40) as f32 / (1u64 << 24) as f32
        };
        let stars = (0..count)
            .map(|_| {
                let at = vec2(unit(), unit());
                // Squared, so most stars are far and a few are near, as in a
                // real sky.
                let d = unit();
                Star {
                    at,
                    depth: d * d,
                    phase: unit(),
                }
            })
            .collect();
        Self {
            stars,
            drift: Vec2::ZERO,
            tint: WHITE,
            size: 2.0,
        }
    }

    /// The nearest layer moves `v` units a second.
    pub fn drift(mut self, v: Vec2) -> Self {
        self.drift = v;
        self
    }

    pub fn tint(mut self, color: Color) -> Self {
        self.tint = color;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn stars(&self) -> &[Star] {
        &self.stars
    }

    /// Where `star` is in `rect` at time `t`: drifted by its depth and wrapped
    /// back into the rect, so the sky never runs out.
    pub fn place(&self, star: &Star, rect: Rect, t: f32) -> Vec2 {
        let speed = 0.15 + 0.85 * star.depth;
        let moved = star.at * vec2(rect.w, rect.h) + self.drift * speed * t;
        vec2(
            rect.x + moved.x.rem_euclid(rect.w.max(1.0)),
            rect.y + moved.y.rem_euclid(rect.h.max(1.0)),
        )
    }

    /// How bright `star` is at `t`, 0 to 1.
    pub fn brightness(star: &Star, t: f32) -> f32 {
        let base = 0.3 + 0.7 * star.depth;
        let rate = 0.6 + 2.4 * star.phase;
        let twinkle = 0.7 + 0.3 * (t * rate + star.phase * TAU).sin();
        base * twinkle
    }

    /// Draw every star in `rect` at time `t`. Near stars get a faint halo.
    pub fn draw(&self, rect: Rect, t: f32) {
        for star in &self.stars {
            let at = self.place(star, rect, t);
            let b = Self::brightness(star, t);
            let s = self.size * (0.35 + 0.65 * star.depth);
            let c = Color::new(self.tint.r, self.tint.g, self.tint.b, self.tint.a * b);
            if star.depth > 0.6 {
                let halo = Color::new(c.r, c.g, c.b, c.a * 0.18);
                draw_circle(at.x, at.y, s * 1.8, halo);
            }
            draw_rectangle(at.x - s * 0.5, at.y - s * 0.5, s, s, c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_is_the_same_sky() {
        assert_eq!(Starfield::new(9, 50).stars(), Starfield::new(9, 50).stars());
        assert_ne!(
            Starfield::new(9, 50).stars(),
            Starfield::new(10, 50).stars()
        );
    }

    #[test]
    fn stars_stay_inside_the_rect_however_long_they_drift() {
        let field = Starfield::new(3, 200).drift(vec2(-40.0, 15.0));
        let rect = Rect::new(10.0, 20.0, 390.0, 844.0);
        for t in [0.0, 1.0, 100.0, 12_345.0] {
            for s in field.stars() {
                let p = field.place(s, rect, t);
                assert!(rect.contains(p), "{p:?} at t={t}");
            }
        }
    }

    #[test]
    fn near_stars_drift_further_than_far_ones() {
        let field = Starfield::new(1, 0).drift(vec2(10.0, 0.0));
        let rect = Rect::new(0.0, 0.0, 1000.0, 10.0);
        let far = Star {
            at: Vec2::ZERO,
            depth: 0.0,
            phase: 0.0,
        };
        let near = Star { depth: 1.0, ..far };
        assert!(field.place(&near, rect, 5.0).x > field.place(&far, rect, 5.0).x);
    }

    #[test]
    fn brightness_stays_between_zero_and_one() {
        for s in Starfield::new(4, 100).stars() {
            for t in 0..50 {
                let b = Starfield::brightness(s, t as f32 * 0.37);
                assert!((0.0..=1.0).contains(&b));
            }
        }
    }
}
