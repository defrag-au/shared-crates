//! The sea: depth, the floor seen through the water ([`crate::water`]), and
//! the coast's waterlines, surf and stroke on the water side.

use crate::coast;
use crate::colour::{Colour, to_u8};
use crate::sheet::{Cell, Grid, box_blur};
use crate::style::Style;
use crate::water;

/// How the sea's surf is drawn. Starting values tuned by eye.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeaStyle {
    /// Share of its full strength the surf keeps on a fully sheltered
    /// shore.
    pub sheltered_surf: f32,
    /// Blur of the floor's depth, as a share of the sheet's width: enough
    /// to drop the generator's texel grain, not the floor's relief.
    pub floor_blur: f32,
}

impl Default for SeaStyle {
    fn default() -> Self {
        Self {
            sheltered_surf: 0.45,
            floor_blur: 1.0 / 2000.0,
        }
    }
}

/// Depth below `level`, 0..1 of the deepest point, per cell. The generator's
/// sea floor is noisy at texel scale; this is blurred so depth tints and
/// contours read as basins rather than speckle.
pub fn depth(cells: &[Cell], grid: &Grid, level: f32, deep: f32) -> Vec<f32> {
    blurred_depth(cells, grid, level, deep, (grid.w / 300).max(2) as i32)
}

/// [`depth`], blurred only by `radius` pixels.
pub fn blurred_depth(cells: &[Cell], grid: &Grid, level: f32, deep: f32, radius: i32) -> Vec<f32> {
    let raw: Vec<f32> = cells
        .iter()
        .map(|c| ((level - c.elevation) / (level - deep).max(1e-6)).clamp(0.0, 1.0))
        .collect();
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
    /// The floor under it ([`Floor`]).
    pub floor: Floor,
}

/// The sea floor at a pixel.
#[derive(Clone, Copy, Debug)]
pub struct Floor {
    /// Depth, 0..1 of the deepest sea.
    pub depth: f32,
    /// Rise per radian, in depth units.
    pub slope: f32,
    /// Light on its relief, 1 on flat floor ([`crate::relief::light`]).
    pub lit: f32,
    /// The depth the cel regions read: `depth` with its edges' wander
    /// ([`crate::water::WaterStyle::region_wander`]).
    pub region_depth: f32,
    /// Depth a region edge is blended across here: the style's softness in
    /// pixels, times how much depth changes across a pixel.
    pub soft: f32,
}

/// A sea pixel in the cel style: its depth region, with the floor showing
/// through in the shallows ([`crate::water::sea`]), then the shore's marks;
/// with its coverage (255).
///
/// Without `fill` the water is left out: the marks alone, over nothing,
/// with their coverage in the second value, for a renderer that draws the
/// floor and the water itself (the globe's shader) and lays the marks over.
pub fn cel(ink: &Ink, style: &Style, fill: bool) -> ([u8; 3], u8) {
    let (sea, coast) = (&style.sea, &style.coast);
    // Colour and coverage, mixed mark by mark: each mark covers what is
    // under it by its own strength.
    let (mut colour, mut cover) = if fill {
        let floor = water::floor(
            ink.floor.depth,
            ink.floor.slope,
            ink.floor.lit,
            &style.water,
        );
        let f = &ink.floor;
        (
            water::sea(floor, f.depth, f.region_depth, f.soft, &style.water),
            1.0f32,
        )
    } else {
        ([0.0; 3], 0.0)
    };
    let mut over = |mark: Colour, strength: f32| {
        let s = strength.clamp(0.0, 1.0);
        // Straight colour over coverage: blend as if over the cover so
        // far, then re-normalise.
        let new_cover = cover + (1.0 - cover) * s;
        if new_cover > 0.0 {
            for c in 0..3 {
                colour[c] = (colour[c] * cover * (1.0 - s) + mark[c] * s) / new_cover;
            }
        }
        cover = new_cover;
    };
    // The shore: waterlines out to sea and swell crests over the shallows,
    // then surf (harder where the swell meets the shore squarely), then the
    // stroke over it.
    over(coast.stroke, ink.waterline);
    over(coast.surf, ink.crest);
    let surf = (ink.coast.surf
        * (sea.sheltered_surf + (1.0 - sea.sheltered_surf) * ink.surf)
        * (1.0 + style.shore.rock_surf * ink.rock))
        .min(1.0);
    over(coast.surf, surf);
    over(coast.stroke, ink.coast.stroke);
    (to_u8(colour), (cover * 255.0).round() as u8)
}
