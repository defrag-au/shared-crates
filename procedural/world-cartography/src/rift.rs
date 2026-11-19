//! Rifts: where plates spread apart, a thin seam of molten rock shows, on
//! the land and on the sea floor (seen when the sea is drained). Stylised,
//! a little science fiction: a real ridge's magma stays a kilometre or so
//! under its rift valley. It glows hottest at the seam and where the plates
//! part fastest, and fades out either side.
//!
//! The seam is the plate layer's boundary ([`world_bake::plates::meeting`]),
//! looked up through a little noise so it wanders like a crack. That
//! boundary is a staircase at the history's sample spacing, so a sheet is
//! drawn in two steps: a broad band about it ([`glow`]), then that band
//! blurred and cut back to a thin core ([`seams`]), which follows the
//! staircase's average path rather than its steps.

use glam::Vec3;
use world_bake::plates::{self, Boundary};
use world_bake::{Layer, PlateMotion};

use crate::colour::{Colour, rgb, smoothstep};
use crate::noise::gradient_noise;
use crate::sheet::{Grid, box_blur};

/// How rifts are drawn. Starting values chosen by eye.
#[derive(Clone, Debug, PartialEq)]
pub struct RiftStyle {
    /// The seam's colour at its hottest, and the glow's as it cools.
    pub core: Colour,
    pub hot: Colour,
    /// How strongly a full glow takes over what is under it.
    pub strength: f32,
    /// Radians from the boundary the broad band ([`glow`]) stays full, and
    /// where it has faded.
    pub core_width: f32,
    pub fade_width: f32,
    /// Radians the band is blurred over before it is cut back ([`seams`]):
    /// about the history's sample spacing, so the staircase smooths out.
    pub smoothing: f32,
    /// The blurred band's levels the seam's core runs between: below the
    /// first nothing glows, above the second it glows fully.
    pub cut: (f32, f32),
    /// Spreading rate (radians per million years) at which a rift glows
    /// fully; slower ones glow less. 0.015 is about 95 mm a year, a fast
    /// ridge; the 1g world's boundaries run 30 to 120.
    pub full_rate: f32,
    /// How far the seam wanders off the plate boundary, radians, and the
    /// noise's frequency on the unit sphere.
    pub wander: f32,
    pub wander_frequency: f32,
    /// How deep the seam's valley cuts into its ridge's crest, as a share of
    /// the sea's depth scale: the axial rift valley a slow-spreading ridge
    /// carries, 1 to 3 km deep on Earth. 0.15 of the abyss is about 0.9 km.
    pub valley: f32,
    /// Under the sea, the seam shows faintly through the water: its glow
    /// times this, fading by `exp(-water_fade · depth)` (depth 0..1 of the
    /// abyss). Stylised: a real ridge does not glow through the sea.
    pub through_water: f32,
    pub water_fade: f32,
    /// The glow's slow shimmer on a live globe: periods per second and how
    /// much of the glow it takes away at its dimmest.
    pub shimmer_rate: f32,
    pub shimmer: f32,
}

impl Default for RiftStyle {
    fn default() -> Self {
        Self {
            core: rgb(255, 232, 150),
            hot: rgb(232, 84, 30),
            strength: 0.6,
            core_width: 0.006,
            fade_width: 0.02,
            smoothing: 0.012,
            cut: (0.28, 0.55),
            full_rate: 0.008,
            wander: 0.004,
            wander_frequency: 40.0,
            valley: 0.15,
            through_water: 0.45,
            water_fade: 2.5,
            shimmer_rate: 0.25,
            shimmer: 0.35,
        }
    }
}

/// How brightly a rift glows at unit direction `dir`, 0..1: 0 away from any
/// spreading boundary.
pub fn glow(plates: &Layer, motions: &[PlateMotion], dir: Vec3, style: &RiftStyle) -> f32 {
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
        return 0.0;
    };
    if m.boundary() != Boundary::Spreading {
        return 0.0;
    }
    let heat = (m.opening / style.full_rate).clamp(0.0, 1.0);
    heat * (1.0 - smoothstep(style.core_width, style.fade_width, m.distance))
}

/// A sheet's seams from its broad bands (`band`, [`glow`] per pixel of
/// `grid`): blurred over [`RiftStyle::smoothing`], then cut back to a thin
/// core between [`RiftStyle::cut`]'s levels, so a seam follows its
/// boundary's average path. 0..1 per pixel.
pub fn seams(band: &[f32], grid: &Grid, style: &RiftStyle) -> Vec<f32> {
    let radius = ((style.smoothing / grid.step).round() as i32).max(1);
    let blurred = box_blur(&box_blur(band, grid, radius), grid, radius);
    blurred
        .iter()
        .map(|&b| smoothstep(style.cut.0, style.cut.1, b))
        .collect()
}

/// The glow's colour: `hot` across the seam, with a thin pale `core` only
/// where it is strongest.
pub fn colour(glow: f32, style: &RiftStyle) -> Colour {
    crate::colour::mix(style.hot, style.core, smoothstep(0.75, 1.0, glow))
}

/// How much of what is under it the glow covers: its strength, falling off
/// faster than the glow toward the seam's edges.
pub fn cover(glow: f32, style: &RiftStyle) -> f32 {
    (glow * glow * style.strength).clamp(0.0, 1.0)
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
}
