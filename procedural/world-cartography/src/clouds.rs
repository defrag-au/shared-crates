//! Cloud cover, for a renderer to cut cloud shapes from (the Blender globe
//! keeps the faces above a threshold).

use glam::Vec3;
use rayon::prelude::*;
use world_bake::Layer;

use crate::noise::{gradient_noise, hash};
use crate::sheet::{Cell, Grid, box_blur};

/// The generator's cloud field is speckled at texel scale; clouds are drawn
/// from it averaged over about 2°.
pub const SMOOTHING: f32 = 0.035;

/// Where cloud cover comes from.
#[derive(Clone, Copy)]
pub enum Source {
    /// The climate simulation's cloud density alone.
    Simulated,
    /// The simulation for where, seeded noise for shape.
    Mixed,
    /// Only where cloud forms, as [`Source::Mixed`] reads it: for a renderer
    /// that makes the shapes itself, moving ([`MIX`]).
    Where,
}

/// How [`Source::Mixed`] makes cover: a base, plus where the simulation puts
/// cloud, plus [`shape`]; cloud stands where the sum passes [`THRESHOLD`]. A
/// live renderer mixes its own moving shape with a [`Source::Where`] sheet by
/// the same numbers.
pub const MIX: [f32; 3] = [0.2, 0.6, 0.55];
/// The cover above which a renderer draws cloud (the Blender globe's cut).
pub const THRESHOLD: f32 = 0.5;

/// Cloud shape, roughly -1..1: seeded noise stretched along latitude so
/// systems run east-west, warped so the streaks swirl. Low frequencies only:
/// a cartoon cloud is a bold shape, not a texture. The seed moves the pattern
/// without touching the planet.
pub fn shape(dir: Vec3, seed: u64) -> f32 {
    let shift = Vec3::new(
        hash(Vec3::splat(seed as f32 * 0.013)) * 50.0,
        hash(Vec3::splat(seed as f32 * 0.029)) * 50.0,
        hash(Vec3::splat(seed as f32 * 0.047)) * 50.0,
    );
    let p = Vec3::new(dir.x, dir.y * 1.6, dir.z) + shift;
    let warp = Vec3::new(
        gradient_noise(p * 1.3),
        gradient_noise(p * 1.3 + Vec3::splat(5.2)),
        gradient_noise(p * 1.3 + Vec3::splat(9.7)),
    ) * 0.22;
    let q = p + warp;
    0.65 * gradient_noise(q * 2.2) + 0.35 * gradient_noise(q * 5.0)
}

/// Cloud cover per cell, 0–255.
///
/// The climate simulation's cloud density (`density`, the bake's
/// `cloud_density` layer) is a faint speckled band along coasts plus a few
/// dense cyclones. Smoothed over ~2° on the ground and scaled to its 98th
/// percentile (not the single densest cyclone), the bands become cloud
/// systems and the cyclones stay the strongest weather. [`Source::Mixed`]
/// lets the simulation decide where cloud forms — its field averaged wider
/// still, so it reads as regions — and [`shape`] decide the shapes inside.
pub fn cover(cells: &[Cell], grid: &Grid, density: &Layer, source: Source, seed: u64) -> Vec<u8> {
    let radius = match source {
        Source::Simulated => SMOOTHING,
        Source::Mixed | Source::Where => SMOOTHING * 2.0,
    };
    let smoothed: Vec<f32> = cells
        .par_iter()
        .map(|c| {
            if c.valid {
                density.sample_disc(c.dir.to_array(), radius)
            } else {
                0.0
            }
        })
        .collect();
    // Seventeen taps alias a field this grainy; a separable blur over the
    // sheet removes what is left, so cut cloud edges are clean curves.
    let blur = (grid.w / 400).max(2) as i32;
    let smoothed = box_blur(&box_blur(&smoothed, grid, blur), grid, blur);
    let mut sorted: Vec<f32> = smoothed.iter().copied().step_by(17).collect();
    sorted.sort_by(f32::total_cmp);
    let p98 = sorted[sorted.len() * 98 / 100].max(f32::MIN_POSITIVE);
    cells
        .par_iter()
        .zip(smoothed.par_iter())
        .map(|(c, &density)| {
            let simulated = (density / p98).clamp(0.0, 1.0);
            let value = match source {
                Source::Simulated | Source::Where => simulated,
                Source::Mixed => MIX[0] + simulated * MIX[1] + shape(c.dir, seed) * MIX[2],
            };
            (value.clamp(0.0, 1.0) * 255.0) as u8
        })
        .collect()
}
