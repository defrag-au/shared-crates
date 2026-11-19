//! Impacts through a planet's whole life: a heavy early bombardment tailing
//! off into a steady background that never stops, landing anywhere — ocean
//! as much as land.
//!
//! A crater is cut into whatever crust is there when it lands, so it rides
//! that plate afterwards. On land it stays, and erosion wears it down; on the
//! sea floor it is carried to a trench in time and consumed with the floor
//! it is on. The record wipes itself, as Earth's has: its oldest sea floor
//! is under 200 My old and holds almost no craters, while old continents
//! keep theirs.

use glam::Vec3;

use crate::noise::Rng;

/// When and how often impacts come.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bombardment {
    /// Impacts per My at the start: the early heavy bombardment.
    pub early: f32,
    /// e-folding time of the early rate, My.
    pub decay: f32,
    /// Impacts per My for ever after: the background.
    pub background: f32,
}

impl Bombardment {
    /// No impacts.
    pub const NONE: Self = Self {
        early: 0.0,
        decay: 1.0,
        background: 0.0,
    };

    /// Impacts expected per My at `time`.
    pub fn rate(&self, time: f32) -> f32 {
        self.early * (-time / self.decay.max(1e-3)).exp() + self.background
    }
}

/// One impact, as it landed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Impact {
    /// Its number, from 1, in the order impacts landed: the
    /// [`crate::Crust::scar`] its crater's crust carries.
    pub id: u32,
    /// My since the planet's start.
    pub time: f32,
    /// Where it struck, a unit direction in the fixed frame of the sphere
    /// (the plate under it has moved on since).
    pub dir: Vec3,
    /// Crater radius, radians.
    pub radius: f32,
    /// Bowl depth below the ground it struck, km.
    pub depth: f32,
    /// It struck sea floor.
    pub ocean: bool,
}

/// A crater still in the crust at the end of a run ([`crate::Sim::craters`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crater {
    pub impact: Impact,
    /// Where it is now: the centre of the crust that holds its heart, carried
    /// there by its plate since it landed.
    pub dir: Vec3,
    /// How deep its bowl still is below its rim, km: erosion and infill
    /// soften it with age.
    pub depth: f32,
    /// It is in sea floor.
    pub ocean: bool,
}

/// Largest crater radius, radians: a basin ~1300 km across on an Earth-sized
/// planet, the size of the Moon's largest (South Pole–Aitken).
pub const MAX_RADIUS: f32 = 0.2;
/// Crater sizes follow a power law, the shape of crater counts on the Moon:
/// the number larger than `r` falls as `r^−SIZE_SLOPE`.
const SIZE_SLOPE: f32 = 2.0;

/// A crater's radius, from a uniform draw `u`, between `min` (the grid's
/// resolution: smaller craters do not show) and [`MAX_RADIUS`].
pub fn radius(u: f32, min: f32) -> f32 {
    (min * (1.0 - u).max(1e-6).powf(-1.0 / SIZE_SLOPE)).min(MAX_RADIUS)
}

/// A crater's bowl depth, km, for its radius in km: deeper with size, but
/// shallower for its width the bigger it is (large basins relax), as with
/// the Moon's and Earth's (~2 km at 100 km across, ~5 km at most).
pub fn depth(radius_km: f32) -> f32 {
    (1.4 * (radius_km / 50.0).sqrt()).min(5.0)
}

/// The crater's change to the ground, in units of its depth, at `t` radii
/// from its centre: a cone-shaped bowl, a rim, and a low ejecta apron (the
/// shape world bakes use for their simple craters). Zero beyond 2.2 radii.
pub fn profile(t: f32) -> f32 {
    if t >= 2.2 {
        return 0.0;
    }
    let bowl = if t < 1.0 { -(1.0 - t) } else { 0.0 };
    let rim = (-((t - 1.0) * (t - 1.0)) / 0.03).exp() * 0.45;
    let apron = if t > 1.0 {
        0.12 * (-(t - 1.0) * 3.0).exp()
    } else {
        0.0
    };
    bowl + rim + apron
}

/// The impacts landing over `dt` My from `time`: a Poisson count at the
/// rate then, each at a uniformly random direction and moment in the step,
/// sized from the power law. Every impact draws the same numbers whatever
/// it hits, so the history of strikes is the seed's alone.
pub fn draw(
    rng: &mut Rng,
    bombardment: &Bombardment,
    time: f32,
    dt: f32,
    min_radius: f32,
    radius_km: f32,
) -> Vec<(f32, Vec3, f32, f32)> {
    let expected = bombardment.rate(time + dt * 0.5) * dt;
    // Poisson by inversion: fine for the small counts per step.
    let limit = (-expected).exp();
    let mut count = 0;
    let mut product = rng.range(0.0, 1.0);
    while product > limit && count < 1000 {
        count += 1;
        product *= rng.range(0.0, 1.0);
    }
    (0..count)
        .map(|_| {
            let when = time + rng.range(0.0, dt);
            let dir = rng.direction();
            let r = radius(rng.range(0.0, 1.0), min_radius);
            (when, dir, r, depth(r * radius_km))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rate_falls_from_heavy_to_a_background_that_stays() {
        let b = Bombardment {
            early: 2.0,
            decay: 150.0,
            background: 0.02,
        };
        assert!((b.rate(0.0) - 2.02).abs() < 1e-5);
        assert!(b.rate(300.0) < b.rate(0.0) * 0.2);
        assert!((b.rate(4000.0) - 0.02).abs() < 1e-4, "background for ever");
        assert_eq!(Bombardment::NONE.rate(0.0), 0.0);
    }

    #[test]
    fn sizes_follow_the_power_law_within_bounds() {
        let min = 0.017;
        assert_eq!(radius(0.0, min), min);
        assert!(radius(0.999_999, min) <= MAX_RADIUS);
        // Half are under √2 × the smallest: many small, few large.
        assert!((radius(0.5, min) - min * 2f32.sqrt()).abs() < 1e-5);
        assert!(depth(100.0) > 1.5 && depth(100.0) < 2.5);
        assert_eq!(depth(5000.0), 5.0);
    }

    #[test]
    fn a_crater_is_a_bowl_with_a_rim_and_nothing_far_off() {
        assert_eq!(profile(0.0), -1.0);
        assert!(profile(1.0) > 0.4);
        assert_eq!(profile(3.0), 0.0);
    }

    #[test]
    fn draws_are_seeded_and_count_with_the_rate() {
        let b = Bombardment {
            early: 0.0,
            decay: 1.0,
            background: 5.0,
        };
        let mut a = Rng::new(9);
        let mut c = Rng::new(9);
        let x = draw(&mut a, &b, 0.0, 2.0, 0.017, 6370.0);
        assert_eq!(x, draw(&mut c, &b, 0.0, 2.0, 0.017, 6370.0));
        let mut rng = Rng::new(1);
        let total: usize = (0..500)
            .map(|k| draw(&mut rng, &b, k as f32 * 2.0, 2.0, 0.017, 6370.0).len())
            .sum();
        // 10 a step expected over 500 steps.
        assert!((4500..5500).contains(&total), "{total}");
    }
}
