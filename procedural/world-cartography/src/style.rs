//! How a chart is drawn: every tunable of the cel style in one place, by
//! the feature it shapes, each with documented defaults. A chart passes one
//! [`Style`]; another look is another `Style`, not edited constants.

use crate::coast::{CoastStyle, WaterlineStyle};
use crate::relief::ReliefStyle;
use crate::rift::RiftStyle;
use crate::sea::SeaStyle;
use crate::shore::ShoreStyle;
use crate::water::WaterStyle;
use crate::waves::WaveStyle;

/// The cel style's look.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub coast: CoastStyle,
    pub waterlines: WaterlineStyle,
    pub sea: SeaStyle,
    pub relief: ReliefStyle,
    pub waves: WaveStyle,
    pub shore: ShoreStyle,
    pub water: WaterStyle,
    pub rift: RiftStyle,
}
