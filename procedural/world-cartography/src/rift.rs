//! Rifts: where plates spread apart, on the land and on the sea floor (seen
//! when the sea is drained).
//!
//! A rift is geology, not light: young dark basalt, a scar of cooled crust
//! and the valley down its axis. No planet anyone has imaged shows a
//! glowing river along a plate boundary, and nothing glows through an
//! ocean. Heat shows as a real one would from orbit: a few vents on the
//! fastest-spreading stretches, each a dark patch with a faint warm middle
//! by day and a point of light at night. Under the sea a rift is only the
//! shape of its valley. The recipe and its reasons are the compositor's
//! `docs/perps-world-map/rift-recipe.md`.
//!
//! The seam is the plate layer's boundary ([`world_bake::plates::meeting`]),
//! looked up through a little noise so it wanders like a crack. That
//! boundary is a staircase at the history's sample spacing, so a sheet is
//! drawn in two steps: a broad band about it ([`seam`]), then that band
//! blurred ([`sheet`]), which follows the staircase's average path rather
//! than its steps. The blurred band is not cut back to a ribbon. Its crest
//! is the seam's axis, and the sheet stores each pixel's distance from that
//! crest: a renderer places each layer at its own distance by a threshold,
//! and so can draw a line thinner than a pixel of the sheet.
//!
//! The band's own height is no measure of that distance: along a real
//! boundary the staircase smears it, so its crest stands at anything from
//! half to all of a straight seam's. The distance comes from its shape
//! instead ([`from_crest`]), which does not change with its height.

use glam::Vec3;
use world_bake::plates::{self, Boundary};
use world_bake::{Layer, PlateMotion};

use crate::colour::{Colour, rgb, smoothstep};
use crate::noise::gradient_noise;
use crate::sheet::{Grid, box_blur};
use rayon::prelude::*;

/// How rifts are drawn. Starting values chosen by eye, not sourced.
#[derive(Clone, Debug, PartialEq)]
pub struct RiftStyle {
    /// Radians from the boundary the broad band ([`seam`]) stays full, and
    /// where it has faded.
    pub core_width: f32,
    pub fade_width: f32,
    /// Radians the band is blurred over ([`sheet`]): about the history's
    /// sample spacing, so the staircase smooths out.
    pub smoothing: f32,
    /// Spreading rate (radians per million years) at which a rift is at
    /// full heat; slower ones are cooler. 0.015 is about 95 mm a year, a
    /// fast ridge; the 1g world's boundaries run 30 to 120.
    pub full_rate: f32,
    /// How far the seam wanders off the plate boundary, radians, and the
    /// noise's frequency on the unit sphere.
    pub wander: f32,
    pub wander_frequency: f32,
    /// How deep the seam's valley cuts into its ridge's crest, as a share of
    /// the sea's depth scale: the axial rift valley a slow-spreading ridge
    /// carries, 1 to 3 km deep on Earth. 0.15 of the abyss is about 0.9 km.
    pub valley: f32,
    /// Radians from the axis at which the valley has run out.
    pub valley_width: f32,

    /// The sea floor beside the seam, drained: young basalt, how much of
    /// the floor it covers near the axis, and the radians from the axis it
    /// stays full to and has gone by. A lit surface.
    pub flank: Colour,
    pub flank_cover: f32,
    pub flank_width: (f32, f32),
    /// Cooled crust: the scar. A lit surface, nearly opaque, out to
    /// `crust_width` radians from the axis, its edge moved in and out by
    /// `crust_ragged` radians of noise at `ragged_frequency`. On land the
    /// scar is `land_crust` as wide: the ground there is lighter and busier
    /// than the sea floor, so a narrower one already shows.
    pub crust: Colour,
    pub crust_cover: f32,
    pub crust_width: f32,
    pub crust_ragged: f32,
    pub ragged_frequency: f32,
    pub land_crust: f32,
    /// A vent: molten rock at the surface, a short lens on the seam's axis
    /// with a paler core `core_share` of its width. `vent_width` is its
    /// half-width in radians at its middle. Never cream or white.
    pub vent: Colour,
    pub vent_core: Colour,
    pub vent_width: f32,
    pub core_share: f32,
    /// Vents are few. One stands where a noise along the seam (at
    /// `vent_frequency`) runs above a level: this at no heat (out of the
    /// noise's reach: a cool rift has none) and this at full heat. On land
    /// the level is `land_vents` higher still.
    pub vents: (f32, f32),
    pub vent_frequency: f32,
    pub land_vents: f32,
    /// How much of a vent's glow shows in full daylight: from orbit, lava
    /// by day is a dark patch with a faint warm middle, and a point of
    /// light only at night.
    pub day_glow: f32,
    /// A vent's bloom, at night only: ADDED light, fading out by
    /// `bloom_width` radians.
    pub bloom: Colour,
    pub bloom_width: f32,
    pub bloom_strength: f32,
    /// A vent sits a little off its scar's middle: the profile is looked up
    /// through two noises, each (radians, frequency).
    pub meander: (f32, f32),
    pub meander_fine: (f32, f32),
    /// On a live globe a vent swells and thins: seconds per cycle, and the
    /// share of its width it swings by.
    pub breathe_period: f32,
    pub breathe: f32,
}

impl Default for RiftStyle {
    fn default() -> Self {
        Self {
            core_width: 0.006,
            fade_width: 0.02,
            smoothing: 0.012,
            full_rate: 0.008,
            wander: 0.004,
            wander_frequency: 40.0,
            valley: 0.15,
            valley_width: 0.016,

            flank: rgb(52, 50, 62),
            flank_cover: 0.55,
            flank_width: (0.012, 0.030),
            crust: rgb(40, 26, 34),
            crust_cover: 0.92,
            crust_width: 0.0125,
            crust_ragged: 0.004,
            ragged_frequency: 120.0,
            land_crust: 0.6,
            vent: rgb(255, 112, 32),
            vent_core: rgb(255, 206, 92),
            vent_width: 0.003,
            core_share: 0.4,
            vents: (1.6, 0.55),
            vent_frequency: 70.0,
            land_vents: 0.1,
            day_glow: 0.2,
            bloom: rgb(255, 88, 48),
            bloom_width: 0.006,
            bloom_strength: 0.25,
            meander: (0.004, 60.0),
            meander_fine: (0.0015, 200.0),
            breathe_period: 8.0,
            breathe: 0.15,
        }
    }
}

/// A rift's broad band at one place.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Seam {
    /// The band's shape, 0..1: 1 within [`RiftStyle::core_width`] of a
    /// spreading boundary, 0 beyond [`RiftStyle::fade_width`] and away from
    /// any.
    pub band: f32,
    /// How hot the rift is, 0..1: its spreading rate against
    /// [`RiftStyle::full_rate`]. 0 where there is no band.
    pub heat: f32,
}

/// The rift's band at unit direction `dir`.
pub fn seam(plates: &Layer, motions: &[PlateMotion], dir: Vec3, style: &RiftStyle) -> Seam {
    // Look up the boundary from a point nudged by noise, so the seam
    // wanders.
    let f = dir * style.wander_frequency;
    let nudge = Vec3::new(
        gradient_noise(f),
        gradient_noise(f + Vec3::splat(17.3)),
        gradient_noise(f + Vec3::splat(41.9)),
    ) * style.wander;
    let p = (dir + nudge).normalize();
    let rings = (style.fade_width / (style.core_width * 0.5)).ceil().max(1.0) as u32;
    let Some(m) = plates::meeting(plates, motions, p.to_array(), style.fade_width, rings) else {
        return Seam::default();
    };
    if m.boundary() != Boundary::Spreading {
        return Seam::default();
    }
    Seam {
        band: 1.0 - smoothstep(style.core_width, style.fade_width, m.distance),
        heat: (m.opening / style.full_rate).clamp(0.0, 1.0),
    }
}

/// How brightly a rift's broad band glows at unit direction `dir`, 0..1:
/// its shape times its heat, 0 away from any spreading boundary.
pub fn glow(plates: &Layer, motions: &[PlateMotion], dir: Vec3, style: &RiftStyle) -> f32 {
    let s = seam(plates, motions, dir, style);
    s.heat * s.band
}

/// A sheet's rifts, per pixel of its grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    /// How near each pixel is to a seam's axis, 0..1: `1 − distance /`
    /// [`RiftStyle::reach`], so 1 on the axis and 0 a reach or more from
    /// it. Lower along the axis where a seam ends, so the layers nearest
    /// the axis taper out there first.
    pub profile: Vec<f32>,
    /// Each seam's heat, 0..1: constant across it, varying along it.
    pub heat: Vec<f32>,
}

/// The blurred band below this share of a straight seam's crest is no seam.
const FLOOR: f32 = 1e-3;
/// A seam fades out along its axis where its blurred band's crest falls
/// between these shares of a straight seam's: its ends, where the boundary
/// stops spreading.
const ENDS: (f32, f32) = (0.1, 0.35);

/// A sheet's rifts from its broad bands (`seams`, [`seam`] per pixel of
/// `grid`): the band blurred over [`RiftStyle::smoothing`] so it follows its
/// boundary's average path, then each pixel's distance from the blurred
/// band's crest ([`from_crest`]); the heat spread across the band by the
/// same blur.
pub fn sheet(seams: &[Seam], grid: &Grid, style: &RiftStyle) -> Sheet {
    let radius = blur_radius(grid.step, style);
    let blur = |v: &[f32]| box_blur(&box_blur(v, grid, radius), grid, radius);
    let band: Vec<f32> = seams.iter().map(|s| s.band).collect();
    let warmth: Vec<f32> = seams.iter().map(|s| s.band * s.heat).collect();
    let (band, warmth) = (blur(&band), blur(&warmth));
    let across = straight(grid.step, style);
    let peak = across[0];
    let crest = log_curvature(&across, grid.step);
    let reach = style.reach();
    let log: Vec<f32> = band
        .iter()
        .map(|&b| if b > FLOOR * peak { b.ln() } else { f32::NAN })
        .collect();
    let profile = (0..band.len())
        .into_par_iter()
        .map(|i| {
            let Some(seen) = from_crest(&log, grid, i, crest) else {
                return 0.0;
            };
            // Judged on the crest's height, not this pixel's: a seam fades
            // along its axis, never across it.
            let ended = 1.0 - smoothstep(ENDS.0, ENDS.1, band[i] * seen.rise / peak);
            (1.0 - seen.distance / reach - ended).clamp(0.0, 1.0)
        })
        .collect();
    Sheet {
        profile,
        // A normalised blur: the heat of the band about each pixel.
        heat: band
            .iter()
            .zip(&warmth)
            .map(|(&b, &w)| if b > FLOOR * peak { (w / b).clamp(0.0, 1.0) } else { 0.0 })
            .collect(),
    }
}

/// Pixels between the samples a derivative is taken across.
const SPAN: i32 = 2;

/// The crest of the blurred band whose logarithm is `log` (NaN where there
/// is no band), seen from pixel `i`; `None` off the band. `crest` is
/// how sharply a straight seam's logarithm curves at its crest
/// ([`log_curvature`]).
///
/// Across a blurred band the height is close to a bell, `A·exp(−d²/2s²)`.
/// Its logarithm then slopes at `d/s²` and curves at `1/s²`, so the slope
/// over the curvature is `d`, whatever the height `A` and the width `s`:
/// Newton's step to the crest. The slope and curvature are taken across the
/// seam, along the direction the logarithm curves most, so a band whose
/// height changes along its axis still has its crest on the axis.
fn from_crest(log: &[f32], grid: &Grid, i: usize, crest: f32) -> Option<Crest> {
    let at = |dx: i32, dy: i32| {
        grid.neighbour(i, dx * SPAN, dy * SPAN)
            .map(|j| log[j])
            .filter(|v| v.is_finite())
    };
    let c = at(0, 0)?;
    let (e, w, s, n) = (at(1, 0)?, at(-1, 0)?, at(0, 1)?, at(0, -1)?);
    let (se, sw, ne, nw) = (at(1, 1)?, at(-1, 1)?, at(1, -1)?, at(-1, -1)?);
    // A pixel is narrower east-west toward the poles of a global sheet.
    let narrow = if grid.global {
        let lat = (0.5 - ((i / grid.w) as f32 + 0.5) / grid.h as f32) * std::f32::consts::PI;
        lat.cos().max(0.05)
    } else {
        1.0
    };
    let (dx, dy) = (grid.step * SPAN as f32 * narrow, grid.step * SPAN as f32);
    let (gx, gy) = ((e - w) / (2.0 * dx), (s - n) / (2.0 * dy));
    let (hxx, hyy) = ((e - 2.0 * c + w) / (dx * dx), (s - 2.0 * c + n) / (dy * dy));
    let hxy = (se - sw - ne + nw) / (4.0 * dx * dy);
    // The direction it curves most (downward): across the seam.
    let (mean, half) = ((hxx + hyy) * 0.5, (hxx - hyy) * 0.5);
    let curve = mean - (half * half + hxy * hxy).sqrt();
    let (a, b) = ((curve - hyy, hxy), (hxy, curve - hxx));
    let v = if a.0 * a.0 + a.1 * a.1 >= b.0 * b.0 + b.1 * b.1 { a } else { b };
    let len = (v.0 * v.0 + v.1 * v.1).sqrt();
    let slope = if len > 0.0 {
        let across = (gx * v.0 + gy * v.1) / len;
        let along = (gy * v.0 - gx * v.1) / len;
        // Where the band also curves down along its axis (a seam's end, a
        // lone stub), the crest is a point, not a line: count the slope
        // along it by how much of the curvature across it is along too.
        let other = 2.0 * mean - curve;
        let round = if curve < 0.0 { (other / curve).clamp(0.0, 1.0) } else { 0.0 };
        (across * across + round * round * along * along).sqrt()
    } else {
        // Curving alike every way: any direction is across.
        (gx * gx + gy * gy).sqrt()
    };
    // Off the crest a blurred band curves more than a bell does, and far out
    // in its tail much more, so the step alone falls short and then turns
    // back toward the axis. The slope over the crest's own curvature errs
    // the other way. Their geometric mean holds to about a tenth out to the
    // crust's edge on a straight seam, and never turns back.
    let distance = slope / ((-curve).max(crest) * crest).sqrt();
    // A bell `distance` from its crest stands `exp(slope · distance / 2)`
    // below it.
    Some(Crest {
        distance,
        rise: (0.5 * slope * distance).exp(),
    })
}

/// A blurred band's crest seen from a pixel on the band.
struct Crest {
    /// Radians to it.
    distance: f32,
    /// How many times higher than the band here it stands.
    rise: f32,
}

/// How deep a rift's valley cuts at a pixel, as a share of the sea's depth
/// scale: [`RiftStyle::valley`] on a hot seam's axis, running out by
/// [`RiftStyle::valley_width`] from it, less on a cooler seam.
pub fn valley(profile: f32, heat: f32, style: &RiftStyle) -> f32 {
    let distance = (1.0 - profile.clamp(0.0, 1.0)) * style.reach();
    let v = 1.0 - smoothstep(0.0, style.valley_width, distance);
    v * v * heat * style.valley
}

/// One pixel of the rift sheet for a renderer, as RGBA8: the profile in R,
/// the heat in G, B spare, A opaque. To be sampled with linear filtering.
pub fn encode(profile: f32, heat: f32) -> [u8; 4] {
    let unit = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [unit(profile), unit(heat), 0, 255]
}

impl RiftStyle {
    /// Radians from a seam's axis that a sheet's profile runs over, 1 to 0:
    /// a layer `w` radians from the axis lies at profile `1 − w / reach`.
    /// About where the blurred band ends.
    pub fn reach(&self) -> f32 {
        self.fade_width + self.smoothing
    }
}

fn blur_radius(step: f32, style: &RiftStyle) -> i32 {
    ((style.smoothing / step).round() as i32).max(1)
}

/// How sharply the logarithm of a straight seam's blurred band (`across`,
/// from [`straight`]) curves at its crest, per radian squared.
fn log_curvature(across: &[f32], step: f32) -> f32 {
    let span = SPAN as usize;
    let d = step * SPAN as f32;
    (2.0 * (across[0].ln() - across[span].ln()) / (d * d)).max(f32::MIN_POSITIVE)
}

/// The blurred band across a straight seam on a grid of `step` radians: its
/// value at the axis, then at each pixel out from it, as [`sheet`] blurs it.
fn straight(step: f32, style: &RiftStyle) -> Vec<f32> {
    let radius = blur_radius(step, style) as usize;
    // Far enough out that the band and both blurs have ended.
    let half = (style.fade_width / step).ceil() as usize + 4 * radius + 2;
    let n = 2 * half + 1;
    let band: Vec<f32> = (0..n)
        .map(|i| {
            let d = (i as f32 - half as f32).abs() * step;
            1.0 - smoothstep(style.core_width, style.fade_width, d)
        })
        .collect();
    let blur = |v: &[f32]| -> Vec<f32> {
        (0..n)
            .map(|i| {
                let (lo, hi) = (i.saturating_sub(radius), (i + radius).min(n - 1));
                // Past the ends the band is 0, as it is far from a seam.
                v[lo..=hi].iter().sum::<f32>() / (2 * radius + 1) as f32
            })
            .collect()
    };
    blur(&blur(&band))[half..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use world_bake::{LayerInfo, LayerName, octahedral};

    /// Two plates split at the equator, parting at `rate` either way.
    fn split(rate: f32) -> (Layer, Vec<PlateMotion>) {
        let size = 512u32;
        let values = (0..size * size)
            .map(|i| {
                let d = octahedral::uv_to_dir(
                    ((i % size) as f32 + 0.5) / size as f32,
                    ((i / size) as f32 + 0.5) / size as f32,
                );
                if d[1] >= 0.0 { 0.0 } else { 1.0 }
            })
            .collect();
        let info = LayerInfo {
            name: LayerName::Plate,
            file: String::new(),
            units: String::new(),
            min: 0.0,
            max: 1.0,
            size: None,
        };
        // About +x: at +z the northern plate heads north, the southern south.
        let motions = vec![
            PlateMotion {
                axis: [1.0, 0.0, 0.0],
                rate: -rate,
            },
            PlateMotion {
                axis: [1.0, 0.0, 0.0],
                rate,
            },
        ];
        (Layer::from_values(info, size, values), motions)
    }

    #[test]
    fn a_rift_glows_at_its_seam_and_not_far_from_it() {
        let style = RiftStyle {
            wander: 0.0,
            ..RiftStyle::default()
        };
        let (plates, motions) = split(0.01);
        let at = |lat: f32| glow(&plates, &motions, Vec3::new(0.0, lat, 1.0).normalize(), &style);
        assert!(at(0.003) > 0.5, "near the seam: {}", at(0.003));
        assert_eq!(at(0.05), 0.0, "far from it");
        // Closing plates do not glow.
        let (plates, closing) = split(-0.01);
        assert_eq!(
            glow(&plates, &closing, Vec3::new(0.0, 0.003, 1.0).normalize(), &style),
            0.0
        );
    }

    /// A sheet with one straight seam along its middle row, at `heat`, its
    /// band `height` tall (1 as [`seam`] draws it; a real boundary's
    /// staircase smears it lower).
    fn straight_sheet(heat: f32, height: f32, style: &RiftStyle) -> (Grid, Sheet) {
        let (w, h) = (64usize, 129usize);
        let grid = Grid {
            w,
            h,
            global: true,
            step: std::f32::consts::PI / 2000.0,
        };
        let seams: Vec<Seam> = (0..w * h)
            .map(|i| {
                let d = ((i / w) as f32 - 64.0).abs() * grid.step;
                let band = height * (1.0 - smoothstep(style.core_width, style.fade_width, d));
                Seam {
                    band,
                    heat: if band > 0.0 { heat } else { 0.0 },
                }
            })
            .collect();
        let sheet = sheet(&seams, &grid, style);
        (grid, sheet)
    }

    #[test]
    fn a_seams_profile_is_its_distance_from_the_axis() {
        let style = RiftStyle::default();
        let (grid, sheet) = straight_sheet(1.0, 1.0, &style);
        let at = |row: usize| sheet.profile[row * grid.w + 10];
        assert!((at(64) - 1.0).abs() < 1e-3, "on the axis: {}", at(64));
        assert!(at(0) < 1e-4, "far from it: {}", at(0));
        // Out from the axis it only falls.
        for row in 64..128 {
            assert!(at(row + 1) <= at(row) + 1e-4, "row {row}: {} then {}", at(row), at(row + 1));
        }
        // Out to the crust's edge, the distance it stores is the true one to
        // within a fifth (and a hundredth of the reach).
        let reach = style.reach();
        for row in 65..=72 {
            let true_distance = (row - 64) as f32 * grid.step;
            assert!(true_distance <= style.crust_width + grid.step);
            let stored = (1.0 - at(row)) * reach;
            assert!(
                (stored - true_distance).abs() < 0.2 * true_distance + 0.01 * reach,
                "{true_distance} rad out, stored as {stored}"
            );
        }
    }

    #[test]
    fn a_smeared_seam_keeps_its_axis_and_its_widths() {
        // A real boundary's staircase leaves the blurred band at anything
        // from half to all of a straight seam's height. The layers are
        // placed by distance, so that must not move them.
        let style = RiftStyle::default();
        let (grid, full) = straight_sheet(1.0, 1.0, &style);
        let (_, low) = straight_sheet(1.0, 0.5, &style);
        for row in 54..=74 {
            let i = row * grid.w + 10;
            assert!(
                (full.profile[i] - low.profile[i]).abs() < 1e-3,
                "row {row}: {} at full height, {} at half",
                full.profile[i],
                low.profile[i]
            );
        }
    }

    #[test]
    fn heat_is_the_same_across_a_seam_and_is_not_its_shape() {
        let style = RiftStyle::default();
        let (grid, cool) = straight_sheet(0.4, 1.0, &style);
        let (_, hot) = straight_sheet(1.0, 1.0, &style);
        // The profile does not change with heat.
        assert_eq!(cool.profile, hot.profile);
        // Across the seam, wherever its profile is drawn, the heat is the
        // seam's.
        for row in 54..=74 {
            let i = row * grid.w + 10;
            assert!(cool.profile[i] > 0.05, "row {row}: {}", cool.profile[i]);
            assert!((cool.heat[i] - 0.4).abs() < 1e-3, "row {row}: {}", cool.heat[i]);
        }
    }

    #[test]
    fn the_valley_is_deepest_on_a_hot_seams_axis() {
        let style = RiftStyle::default();
        assert_eq!(valley(1.0, 1.0, &style), style.valley);
        assert_eq!(valley(0.0, 1.0, &style), 0.0);
        assert_eq!(valley(1.0, 0.0, &style), 0.0);
        assert!(valley(0.5, 1.0, &style) < 0.6 * style.valley);
    }

    #[test]
    fn the_sheet_keeps_the_profile_and_the_heat_apart() {
        assert_eq!(encode(1.0, 0.5), [255, 128, 0, 255]);
        assert_eq!(encode(0.0, 0.0), [0, 0, 0, 255]);
    }
}
