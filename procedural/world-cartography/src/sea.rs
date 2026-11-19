//! The sea: depth, the cel style's flat depth bands with soft, wandering
//! edges, and the coast's waterlines, surf and stroke on the water side.

use glam::Vec3;

use crate::coast;
use crate::colour::{mix, smoothstep, to_u8};
use crate::noise::gradient_noise;
use crate::palette::SEA_BANDS;
use crate::sheet::{Cell, Grid, box_blur};
use crate::style::Style;

/// How the sea's bands and surf are drawn. Starting values tuned by eye.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeaStyle {
    /// How far a depth band's edge wanders, in depth (0..1 of the
    /// deepest): enough to bend an edge a few pixels, not to break up a
    /// band.
    pub band_wander: f32,
    /// Pixels a band edge is blended across, each side.
    pub band_softness: f32,
    /// Share of its full strength the surf keeps on a fully sheltered
    /// shore.
    pub sheltered_surf: f32,
}

impl Default for SeaStyle {
    fn default() -> Self {
        Self {
            band_wander: 0.012,
            band_softness: 1.5,
            sheltered_surf: 0.45,
        }
    }
}

/// Depth below `level`, 0..1 of the deepest point, per cell. The generator's
/// sea floor is noisy at texel scale; this is blurred so depth tints and
/// contours read as basins rather than speckle.
pub fn depth(cells: &[Cell], grid: &Grid, level: f32, deep: f32) -> Vec<f32> {
    let raw: Vec<f32> = cells
        .iter()
        .map(|c| ((level - c.elevation) / (level - deep).max(1e-6)).clamp(0.0, 1.0))
        .collect();
    let radius = (grid.w / 300).max(2) as i32;
    box_blur(&box_blur(&raw, grid, radius), grid, radius)
}

/// What inks a sea pixel besides its depth.
pub struct Ink<'a> {
    /// The coast's marks ([`crate::coast::marks`]).
    pub coast: &'a coast::Marks,
    /// Waterlines ([`crate::coast::Waterlines::ink`]), 0..1.
    pub waterline: f32,
    /// Swell crests ([`crate::waves::crest`]), 0..1.
    pub crest: f32,
    /// How hard the surf breaks here, 0..1: 1 without swell layers.
    pub surf: f32,
    /// How rocky the shore here is, 0..1 ([`crate::shore::rockiness`]):
    /// surf breaks harder against rock. 0 without shore layers.
    pub rock: f32,
}

/// A sea pixel in the cel style, in direction `dir`. `is_land` is per cell;
/// `four` are the cell's right, left, down and up neighbours.
pub fn cel(
    i: usize,
    dir: Vec3,
    four: [usize; 4],
    depth: &[f32],
    is_land: &[bool],
    ink: &Ink,
    style: &Style,
) -> [u8; 3] {
    let (sea, coast) = (&style.sea, &style.coast);
    // The bands, with their edges wandering a little and blended over a
    // couple of pixels: a painted wash rather than a stepped raster with a
    // seam line between steps.
    let d = depth[i]
        + gradient_noise(dir * 9.0 + Vec3::splat(2.9)) * sea.band_wander
        + gradient_noise(dir * 37.0 + Vec3::splat(8.3)) * sea.band_wander * 0.35;
    // Depth change across a pixel, from the sea neighbours: the blend spans
    // about `band_softness` pixels whatever the slope of the sea floor.
    let across = four
        .iter()
        .filter(|&&j| !is_land[j])
        .map(|&j| (depth[j] - depth[i]).abs())
        .fold(0.0f32, f32::max)
        .max(1e-4);
    let soft = across * sea.band_softness;
    let mut colour = SEA_BANDS[0].1;
    for k in 0..SEA_BANDS.len() - 1 {
        let limit = SEA_BANDS[k].0;
        colour = mix(
            colour,
            SEA_BANDS[k + 1].1,
            smoothstep(limit - soft, limit + soft, d),
        );
    }
    // The shore: waterlines out to sea and swell crests over the shallows,
    // then surf (harder where the swell meets the shore squarely), then the
    // stroke over it.
    colour = mix(colour, coast.stroke, ink.waterline);
    colour = mix(colour, coast.surf, ink.crest);
    let surf = (ink.coast.surf
        * (sea.sheltered_surf + (1.0 - sea.sheltered_surf) * ink.surf)
        * (1.0 + style.shore.rock_surf * ink.rock))
        .min(1.0);
    colour = mix(colour, coast.surf, surf);
    colour = mix(colour, coast.stroke, ink.coast.stroke);
    to_u8(colour)
}
