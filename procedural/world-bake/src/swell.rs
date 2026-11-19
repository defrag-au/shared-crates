//! How a bake solves its swell ([`crate::LayerName::SEA`], baked by
//! `world_baker::bake_swell`).

use serde::{Deserialize, Serialize};

use crate::shore::Ramp;

/// How a bake solves the swell's travel time and the surf's exposure.
/// Recorded in the manifest with the layers it made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SwellParams {
    /// Depth at which waves first feel the bottom, as a share of the
    /// deepest sea: the shelf edge. Earth's shelves break at ~0.2 km over
    /// oceans ~4 km deep, about 5%; 8% was tuned by eye.
    pub shelf_share: f32,
    /// Slowest the swell travels, as a share of its deep-water speed.
    /// Without a floor the speed falls to nothing at the shore and the
    /// crests pile into one line.
    pub slowest: f32,
    /// Exposure every reached shallow texel has, however it faces: swell
    /// wraps round headlands.
    pub exposure_floor: f32,
    /// Smoothing of the depth the swell feels, the travel time it gives,
    /// and the exposure.
    pub smoothing: Smoothing,
    /// The prevailing swell by latitude: west under the trades and the
    /// polar easterlies, east under the westerlies between. Degrees of
    /// latitude over which the westerlies take over from the trades, and
    /// over which the polar easterlies take over from them; near each the
    /// two meet and the swell is weak (the horse latitudes, ~30°).
    pub westerlies_from: Ramp,
    pub westerlies_to: Ramp,
}

/// Smoothing passes, each a mean with the 4-neighbours, as counted on a
/// grid of side `at_size`. Repeated means spread like diffusion, a reach of
/// about √passes texels, so a grid of another side scales the passes by
/// its side squared: the smoothing covers the same ground at any
/// resolution. (Fixed pass counts smeared a coarse grid's whole shelf away.)
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Smoothing {
    /// Of the floor's depth: waves feel the floor averaged over their own
    /// length, and the raw floor's grain bent the crests into squiggles.
    pub depth: f32,
    /// Of the travel time: the grid's steps leave a fine ripple in the
    /// front that crests, drawn as its level lines, would show.
    pub time: f32,
    pub exposure: f32,
    pub at_size: u32,
}

impl Smoothing {
    /// `passes` (one of this struct's) scaled to a grid of side `size`.
    pub fn passes(&self, passes: f32, size: usize) -> usize {
        (passes * (size as f32 / self.at_size as f32).powi(2)).round() as usize
    }
}

impl Default for SwellParams {
    fn default() -> Self {
        Self {
            shelf_share: 0.08,
            slowest: 0.35,
            exposure_floor: 0.25,
            smoothing: Smoothing {
                depth: 12.0,
                time: 4.0,
                exposure: 6.0,
                at_size: 1024,
            },
            westerlies_from: Ramp::new(25.0, 35.0),
            westerlies_to: Ramp::new(55.0, 65.0),
        }
    }
}
