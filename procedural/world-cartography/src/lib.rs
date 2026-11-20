//! Drawing a world bake as a map.
//!
//! One module per map feature, so a feature can be drawn anywhere a map is —
//! the PERPS sheets, a regional view, a future runtime port — without the
//! rest of the pipeline:
//!
//! | Module | Draws |
//! |---|---|
//! | [`sheet`] | the pixel grid, projections, and the sampled [`sheet::Cell`] each pixel reads |
//! | [`terrain`] | mesa terraces, the steepness gate, mesa cleanup and the ink hierarchy |
//! | [`coast`] | signed distance to the shore, which every coast mark is drawn from |
//! | [`sea`] | depth bands, band contours, and the coast's stroke and surf on the water side |
//! | [`craters`] | war craters: placement, their effect on height, and their drawing |
//! | [`rivers`] | rivers as tapered anti-aliased strokes (or per-pixel, without detail) |
//! | [`trees`] | conifers scattered and clumped over forest |
//! | [`clouds`] | cloud cover for a renderer to cut cloud shapes from |
//! | [`season`] | a moment of the year: plants out of leaf, seasonal snow |
//!
//! [`cel`] and [`atlas`] are the two styles; each composes the features into
//! pixels. [`palette`] holds the looks (colours sampled from the Derp Birds
//! paintings — style reference, PROPOSED, not canon), [`colour`] and
//! [`noise`] the small shared helpers.
//!
//! Nothing here generates a world: the input is a `world_bake` bake, read
//! without the generator.

pub mod atlas;
pub mod cel;
pub mod clouds;
pub mod coast;
pub mod colour;
pub mod craters;
pub mod noise;
pub mod palette;
pub mod relief;
pub mod rift;
pub mod rivers;
pub mod sea;
pub mod season;
pub mod sheet;
pub mod shore;
pub mod style;
pub mod terrain;
pub mod trees;
pub mod water;
pub mod waves;

/// Water's freezing point, Kelvin: the atlas style's snowline.
pub const FREEZING_K: f32 = 273.15;

/// Below this mean temperature the ground is an ice sheet: it draws in the
/// frozen palette, rivers stop, and the ice buries the ridges. Kelvin. The
/// generator's classifier's line (`planet_gen::biome::POLAR_ICE_C`, −15 °C);
/// between it and freezing lie tundra and boreal forest, which draw as land.
pub const ICE_SHEET_K: f32 = FREEZING_K - 15.0;
