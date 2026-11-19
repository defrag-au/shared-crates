//! Rifts: where plates spread apart, a thin seam of molten rock shows, on
//! the land and on the sea floor (seen when the sea is drained). Stylised,
//! a little science fiction: a real ridge's magma stays a kilometre or so
//! under its rift valley. It glows hottest at the seam and where the plates
//! part fastest, and fades out either side.
//!
//! The seam is the plate layer's boundary ([`world_bake::plates::meeting`]),
//! looked up through a little noise so it wanders like a crack rather than
//! tracing the history's sample grid.

use glam::Vec3;
use world_bake::plates::{self, Boundary};
use world_bake::{Layer, PlateMotion};

use crate::colour::{Colour, rgb, smoothstep};
use crate::noise::gradient_noise;

/// How rifts are drawn. Starting values chosen by eye.
#[derive(Clone, Debug, PartialEq)]
pub struct RiftStyle {
    /// The seam's colour at its hottest, and the glow's as it cools.
    pub core: Colour,
    pub hot: Colour,
    /// How strongly a full glow takes over what is under it.
    pub strength: f32,
    /// Radians from the seam the glow stays full, and where it has faded.
    pub core_width: f32,
    pub fade_width: f32,
    /// Spreading rate (radians per million years) at which a rift glows
    /// fully; slower ones glow less. 0.015 is about 95 mm a year, a fast
    /// ridge; the 1g world's boundaries run 30 to 120.
    pub full_rate: f32,
    /// How far the seam wanders off the plate boundary, radians, and the
    /// noise's frequency on the unit sphere.
    pub wander: f32,
    pub wander_frequency: f32,
    /// How deep the seam's valley cuts into the drained floor, as a share of
    /// the floor's height range.
    pub valley: f32,
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
            strength: 0.85,
            core_width: 0.0012,
            fade_width: 0.006,
            full_rate: 0.015,
            wander: 0.004,
            wander_frequency: 40.0,
            valley: 0.06,
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

/// The glow's colour: the core where it is strongest, cooling to `hot`.
pub fn colour(glow: f32, style: &RiftStyle) -> Colour {
    crate::colour::mix(style.hot, style.core, glow * glow)
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
