//! The cel style's land: where ground is stepped into mesas and where it is
//! drawn smooth, the frozen zone, forest patches, and which edges are inked.
//!
//! [`analyse`] works out all of it once per sheet; the [`crate::cel`] style
//! and the [`crate::trees`] scatter read the result.

use std::f32::consts::PI;

use glam::Vec3;
use rayon::prelude::*;

use crate::ICE_SHEET_K;
use crate::colour::smoothstep;
use crate::noise::{gradient_noise, grain};
use crate::sheet::{Cell, Grid, TreeCover, box_blur, components};

/// Whether ground at `temperature` (Kelvin, the chart's: the bake's with its
/// regional anomaly) in direction `dir` is ice sheet. A little noise
/// roughens the edge. The one test for it: the cel style's frozen zone and
/// where rivers stop both ask here, so a river never runs onto ice.
pub fn is_ice(temperature: f32, dir: Vec3) -> bool {
    temperature + (grain(dir, 8.0) - 0.5) * 3.0 < ICE_SHEET_K
}

/// The class of gentle ground: one class, so it draws no ink inside itself.
pub const GENTLE: u32 = u32::MAX - 1;

/// Tier crossings per radian of ground above which land is stepped into
/// mesas; the rest is drawn smooth. Each is the 67th percentile of land
/// steepness measured on seed 1204's global 4000-wide sheet (the run log
/// prints the distribution): the steepest third terraces. Fixed rather than
/// re-measured per sheet, so a regional view of a plain makes the same
/// choices as the world sheet. The detail layer is steeper everywhere than
/// the smoothed base, hence two values.
pub const TERRACE_STEEPNESS_BASE: f32 = 15.2;
pub const TERRACE_STEEPNESS_DETAIL: f32 = 22.9;

/// Ground sizes for the cleanup passes, radians: the steepness average, the
/// smallest mesa kept (an area, radians²) and the shortest ink stroke kept.
/// They are the pixel values first tuned on the 4000-wide global sheet
/// (8 px, 900 px², 30 px at 2π/4000 rad per px), held fixed on the ground.
pub const STEEPNESS_BLUR: f32 = 8.0 * std::f32::consts::TAU / 4000.0;
pub const MIN_MESA_AREA: f32 =
    900.0 * (std::f32::consts::TAU / 4000.0) * (std::f32::consts::TAU / 4000.0);
pub const MIN_STROKE_LENGTH: f32 = 30.0 * std::f32::consts::TAU / 4000.0;

/// Ink weights by [`Terrain::ink`] class.
pub const INK_NONE: u8 = 0;
pub const INK_COAST: u8 = 1;
pub const INK_MESA_EDGE: u8 = 2;
pub const INK_TIER: u8 = 3;

/// The land, analysed for drawing. Every vector is one entry per cell.
pub struct Terrain {
    pub is_land: Vec<bool>,
    /// Position on the tier ladder, unjittered: `floor` is the tier.
    pub smooth_u: Vec<f32>,
    /// Land height for relief renders, 0..1 of relief: terraced on mesas,
    /// continuous on gentle ground.
    pub terraced: Vec<f32>,
    /// `2 × tier + cliff` on mesa ground, [`GENTLE`] on gentle ground,
    /// `u32::MAX` at sea. Ink is drawn where it changes.
    pub class: Vec<u32>,
    pub frozen: Vec<bool>,
    /// Inside a forest patch: where trees clump.
    pub forest: Vec<bool>,
    /// [`INK_NONE`], [`INK_COAST`], [`INK_MESA_EDGE`] or [`INK_TIER`].
    pub ink: Vec<u8>,
    /// Signed distance to the shore, equator pixels: negative on land
    /// ([`crate::coast::signed_distance`]).
    pub coast: Vec<f32>,
}

impl Terrain {
    /// A cliff band on mesa ground.
    pub fn is_cliff(&self, i: usize) -> bool {
        self.class[i] != GENTLE && self.class[i] % 2 == 1
    }
}

/// A relief fraction as a position on the tier ladder, with the tier lines
/// jittered by noise so cliffs wander instead of following contours exactly.
pub fn tier_coordinate(t: f32, dir: glam::Vec3, tiers: f32) -> f32 {
    (t * tiers + (grain(dir, 22.0) - 0.5) * 0.45 + (grain(dir, 90.0) - 0.5) * 0.15).max(0.0)
}

/// Snap a tier coordinate onto mesa tiers. Each tier is a near-flat top
/// ending in a steep cliff band. Returns the height (0..1), the tier and
/// whether this is the cliff.
pub fn terrace(u: f32, tiers: f32, cliff_width: f32) -> (f32, u32, bool) {
    let k = u.floor();
    let f = u - k;
    if f > 1.0 - cliff_width {
        let c = (f - (1.0 - cliff_width)) / cliff_width;
        ((k + smoothstep(0.0, 1.0, c)) / tiers, k as u32, true)
    } else {
        ((k + f * 0.08) / tiers, k as u32, false)
    }
}

/// Analyse the sheet's land. `detailed` says heights come from the bake's
/// detail layer (steeper everywhere than the smoothed base).
pub fn analyse(
    cells: &[Cell],
    grid: &Grid,
    sea: f32,
    relief: f32,
    tiers: f32,
    detailed: bool,
) -> Terrain {
    let n = cells.len();
    // A cliff is a fixed share of a tier on the global sheet; zoomed in, the
    // same ground spans more pixels, so the share narrows to keep cliffs steep.
    let global_step = 2.0 * PI / 4000.0;
    let cliff_width = (0.24 * grid.step / global_step).clamp(0.03, 0.24);

    let is_land: Vec<bool> = cells.iter().map(|c| c.valid && c.land > 0.0).collect();

    // Where the tier ladder sits, smooth (for steepness) and jittered (for
    // the lines themselves). The power curve spreads tiers through the
    // lowlands, where most land is, instead of spending them all on peaks.
    let smooth_u: Vec<f32> = cells
        .par_iter()
        .map(|c| ((c.form - sea) / relief).clamp(0.0, 1.0).powf(0.65) * tiers)
        .collect();

    // Steepness — tier crossings per radian of ground — averaged so the gate
    // makes coherent regions rather than following every wrinkle.
    let steepness: Vec<f32> = (0..n)
        .into_par_iter()
        .map(|i| {
            if !is_land[i] {
                return 0.0;
            }
            let pick = |dx: i32, dy: i32| {
                let j = grid.clamped(i, dx, dy);
                if is_land[j] { smooth_u[j] } else { smooth_u[i] }
            };
            let dx = grid.step * grid.lat_scale(cells[i].dir);
            let gx = (pick(1, 0) - pick(-1, 0)) / (2.0 * dx);
            let gy = (pick(0, 1) - pick(0, -1)) / (2.0 * grid.step);
            (gx * gx + gy * gy).sqrt()
        })
        .collect();
    let blur = ((STEEPNESS_BLUR / grid.step).round() as i32).max(2);
    let steepness = box_blur(&box_blur(&steepness, grid, blur), grid, blur);
    let terrace_gate = {
        let mut land: Vec<f32> = (0..n)
            .filter(|&i| is_land[i])
            .map(|i| steepness[i])
            .step_by(11)
            .collect();
        land.sort_by(f32::total_cmp);
        let pct = |p: usize| {
            land.get(land.len().saturating_sub(1) * p / 100)
                .copied()
                .unwrap_or(0.0)
        };
        println!(
            "steepness (tiers/radian) p25 {:.1} · p50 {:.1} · p67 {:.1} · p90 {:.1}",
            pct(25),
            pct(50),
            pct(67),
            pct(90)
        );
        if detailed {
            TERRACE_STEEPNESS_DETAIL
        } else {
            TERRACE_STEEPNESS_BASE
        }
    };

    // The ice sheets, and which ground is steep enough to step. The climate
    // simulation's temperature; a little noise roughens the edge.
    let frozen: Vec<bool> = cells
        .par_iter()
        .map(|c| c.valid && is_ice(c.temperature, c.dir))
        .collect();
    // Ice is never terraced: stepped glaciers drew as scattered inked blobs.
    let steep: Vec<bool> = (0..n)
        .map(|i| is_land[i] && !frozen[i] && steepness[i] > terrace_gate)
        .collect();
    // Mesas smaller than a few tiers' worth of ground are speckle: let them
    // fall back to gentle ground.
    let min_mesa = ((MIN_MESA_AREA / (grid.step * grid.step)) as usize).max(40);
    let (mesa_id, mesa_size) = components(grid, &steep);
    let steep: Vec<bool> = (0..n)
        .map(|i| steep[i] && mesa_size[mesa_id[i] as usize] >= min_mesa)
        .collect();

    // Heights and classes.
    let derived: Vec<(f32, u32)> = (0..n)
        .into_par_iter()
        .map(|i| {
            if !is_land[i] {
                return (0.0, u32::MAX);
            }
            let u = tier_coordinate(smooth_u[i] / tiers, cells[i].dir, tiers);
            if steep[i] {
                let (terraced, tier, cliff) = terrace(u, tiers, cliff_width);
                (terraced, tier * 2 + cliff as u32)
            } else {
                (smooth_u[i] / tiers, GENTLE)
            }
        })
        .collect();
    let terraced: Vec<f32> = derived.iter().map(|d| d.0).collect();
    let class: Vec<u32> = derived.iter().map(|d| d.1).collect();

    // Forest patches: the generator's vegetation, broken into clumps by
    // noise, on gentle unfrozen ground.
    let forest: Vec<bool> = (0..n)
        .into_par_iter()
        .map(|i| {
            let c = &cells[i];
            is_land[i]
                && !frozen[i]
                && c.biome.tree_cover() == TreeCover::Forest
                && class[i] == GENTLE
                && c.vegetation * 1.4 + gradient_noise(c.dir * 11.0) * 0.5 > 0.78
        })
        .collect();

    // Ink, heaviest to lightest: coast, mesa edge (gentle meets stepped),
    // tier or cliff edge. Edge fragments too short to read are dropped.
    let mut ink: Vec<u8> = (0..n)
        .into_par_iter()
        .map(|i| {
            if !is_land[i] {
                return INK_NONE;
            }
            let four = [
                grid.clamped(i, 1, 0),
                grid.clamped(i, -1, 0),
                grid.clamped(i, 0, 1),
                grid.clamped(i, 0, -1),
            ];
            if four.iter().any(|&j| !is_land[j]) {
                INK_COAST
            } else if four.iter().any(|&j| class[j] != class[i]) {
                if class[i] == GENTLE || four.iter().any(|&j| class[j] == GENTLE) {
                    INK_MESA_EDGE
                } else {
                    INK_TIER
                }
            } else {
                INK_NONE
            }
        })
        .collect();
    let min_stroke = ((MIN_STROKE_LENGTH / grid.step) as usize).max(8);
    let edges: Vec<bool> = ink.iter().map(|&k| k >= INK_MESA_EDGE).collect();
    let (stroke_id, stroke_len) = components(grid, &edges);
    for i in 0..n {
        if edges[i] && stroke_len[stroke_id[i] as usize] < min_stroke {
            ink[i] = INK_NONE;
        }
    }

    let coast = crate::coast::signed_distance(&is_land, cells, grid);
    Terrain {
        is_land,
        smooth_u,
        terraced,
        class,
        frozen,
        forest,
        ink,
        coast,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terraces_are_flat_tops_then_cliffs() {
        let (top, tier, cliff) = terrace(2.3, 6.0, 0.24);
        assert_eq!((tier, cliff), (2, false));
        assert!((top - (2.0 + 0.3 * 0.08) / 6.0).abs() < 1e-6);
        let (_, tier, cliff) = terrace(2.9, 6.0, 0.24);
        assert_eq!((tier, cliff), (2, true));
        // Heights rise monotonically across a tier and its cliff.
        let mut last = -1.0;
        for k in 0..60 {
            let (h, _, _) = terrace(k as f32 * 0.1, 6.0, 0.24);
            assert!(h >= last);
            last = h;
        }
    }
}
