//! The chart's moment of the year: plants out of leaf lean to a dormant
//! straw, and seasonal snow lies over the ground in the frozen painting's
//! colours. A chart with no moment (full leaf, no snow) is drawn unchanged.

use crate::colour::{Colour, mix, rgb};

/// How the season shows. Every number is a starting value.
#[derive(Clone, Debug, PartialEq)]
pub struct SeasonStyle {
    /// The hue ground out of leaf leans to: dry grass and bare branches.
    /// Only its hue and saturation are used; the ground keeps its own
    /// lightness, so the cel bands and relief read the same.
    pub dormant: Colour,
    /// How far fully dormant ground leans to [`SeasonStyle::dormant`].
    pub dormancy: f32,
    /// The share of a top's snow a cliff holds: steep walls shed it.
    pub cliff_snow: f32,
}

impl Default for SeasonStyle {
    fn default() -> Self {
        SeasonStyle {
            dormant: rgb(170, 140, 92),
            dormancy: 0.75,
            cliff_snow: 0.35,
        }
    }
}

fn luma(c: Colour) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Ground at a `leaf` (0 dormant, 1 full leaf): its colour leaned to the
/// dormant hue at its own lightness.
pub fn turn(colour: Colour, leaf: f32, style: &SeasonStyle) -> Colour {
    let bare = 1.0 - leaf.clamp(0.0, 1.0);
    if bare <= 0.0 {
        return colour;
    }
    let scale = luma(colour) / luma(style.dormant).max(1e-6);
    let target = style.dormant.map(|c| c * scale);
    mix(colour, target, bare * style.dormancy)
}

/// The shape of a place's year, for a renderer that works out the moment
/// itself (the globe's shader): the annual mean temperature (kelvin, as the
/// chart reads it, peaks cooled by their height), and the bake's seasonal
/// layers (`world_bake::LayerName::SEASONAL`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Year {
    pub mean: f32,
    pub swing: f32,
    pub lag: f32,
    pub rain_season: f32,
}

/// The ranges [`encode_year`] spreads over a byte: the mean temperature's
/// (kelvin; −50 to 50 °C), the most swing, and the longest lag.
pub const MEAN_K: (f32, f32) = (223.15, 323.15);
pub const SWING_MAX_K: f32 = 40.0;
pub const LAG_MAX: f32 = 0.25;

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A place's year as an RGBA texel: mean in R (over [`MEAN_K`]), swing in G
/// (over [`SWING_MAX_K`]), lag in B (over [`LAG_MAX`]), rain season in A.
pub fn encode_year(year: &Year) -> [u8; 4] {
    [
        byte((year.mean - MEAN_K.0) / (MEAN_K.1 - MEAN_K.0)),
        byte(year.swing / SWING_MAX_K),
        byte(year.lag / LAG_MAX),
        byte(year.rain_season),
    ]
}

/// What the ground does with the season, as an RGBA texel beside
/// [`encode_year`]'s: in R the share of its plants that keep their leaves
/// (1 where nothing turns: cliffs, peaks, desert, ice, the sea); in G how
/// well fed its snow is (`world_live::GrowthModel::snow_fed`); in B the
/// share of that snow it holds (1 on a top, [`SeasonStyle::cliff_snow`] on a
/// cliff, 0 on the ice sheet and at sea, which the season leaves alone).
pub fn encode_ground(evergreen: f32, snow_fed: f32, snow_hold: f32) -> [u8; 4] {
    [byte(evergreen), byte(snow_fed), byte(snow_hold), 255]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_leaf_is_unchanged_and_dormant_ground_keeps_its_lightness() {
        let style = SeasonStyle::default();
        let grass = rgb(90, 160, 60);
        assert_eq!(turn(grass, 1.0, &style), grass);
        let dormant = turn(grass, 0.0, &style);
        assert_ne!(dormant, grass);
        assert!((luma(dormant) - luma(grass)).abs() < 0.01);
        // It leans to straw: redder than the grass was.
        assert!(dormant[0] > grass[0]);
    }
}
