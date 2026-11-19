//! The looks.
//!
//! The cel colours are sampled from Derp Birds environment paintings — the
//! world PERPS were first imagined in. Style reference only: PROPOSED, not
//! canon.

use crate::colour::{Colour, rgb};
use crate::sheet::{Biome, Desert, Forest, Grassland, Polar};

/// A surface's three cel tones, darkest to lightest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tones {
    /// Facing away from the light.
    pub shadow: Colour,
    /// Flat ground, and any surface drawn unlit.
    pub mid: Colour,
    /// Facing the light.
    pub light: Colour,
}

impl Tones {
    pub const fn new(shadow: Colour, mid: Colour, light: Colour) -> Self {
        Self { shadow, mid, light }
    }
}

/// One look: tops and cliffs in their cel tones, and the colours drawn over
/// them.
pub struct Look {
    pub top: Tones,
    pub cliff: Tones,
    pub river: Colour,
    pub fall: Colour,
    pub tree: Colour,
    pub ink: Colour,
}

/// The green painting: lime grass, ochre cliffs, cyan water, yellow conifers.
pub const GREEN: Look = Look {
    top: Tones::new(rgb(82, 142, 42), rgb(120, 180, 50), rgb(162, 206, 80)),
    cliff: Tones::new(rgb(96, 66, 36), rgb(150, 110, 56), rgb(194, 154, 90)),
    river: rgb(78, 168, 222),
    fall: rgb(228, 246, 252),
    tree: rgb(200, 180, 48),
    ink: rgb(42, 30, 24),
};

/// The frozen painting: mint snow, periwinkle cliffs, magenta trees. The
/// mint stays in the snow's shadow; its mid and light are lifted and paled
/// so ice is the lightest large mass on the planet. The painting's water is
/// pink; here it is a pale periwinkle, as pink is the hue kept for the
/// game's markers, which most need to be seen across the ice
/// (`docs/perps-world-map/art-bible.md` in the compositor).
pub const FROZEN: Look = Look {
    top: Tones::new(rgb(126, 178, 172), rgb(186, 222, 210), rgb(228, 244, 236)),
    cliff: Tones::new(rgb(60, 78, 134), rgb(98, 118, 170), rgb(150, 162, 206)),
    river: rgb(176, 190, 236),
    fall: rgb(228, 234, 252),
    tree: rgb(198, 44, 116),
    ink: rgb(36, 36, 72),
};

/// The red painting (a Derp Birds desert outpost): rust-orange mesa tops,
/// red canyon walls, blue water. Sampled: tops from its canyon floor, ground
/// and sunlit path; cliffs from its shadowed, mid and lit walls; the river
/// from its waterfall. The fall's foam, the ink and the tree are not in the
/// painting as flat colours: the foam is the river lightened, the ink the
/// shadowed wall darkened, and deserts take no trees. The tops keep the
/// painting's hue and lightness at about four fifths of its chroma (sampled:
/// 219,95,49 / 226,126,70 / 245,152,90): light, saturated orange is kept for
/// heat, and the sampled shadow tone was the rift's own colour.
pub const DESERT: Look = Look {
    top: Tones::new(rgb(192, 104, 66), rgb(212, 134, 86), rgb(232, 164, 114)),
    cliff: Tones::new(rgb(119, 25, 20), rgb(142, 34, 30), rgb(184, 59, 52)),
    river: rgb(81, 178, 197),
    fall: rgb(214, 240, 244),
    tree: rgb(200, 180, 48),
    ink: rgb(62, 16, 14),
};
/// Tundra: halfway between the frozen painting's snow and the meadow grass,
/// tone by tone (sage).
pub const TUNDRA_TOP: Tones =
    Tones::new(rgb(117, 158, 108), rgb(157, 190, 128), rgb(196, 217, 156));
/// Boreal forest: the forest green, darker and cooler.
pub const BOREAL_TOP: Tones = Tones::new(rgb(40, 86, 52), rgb(58, 110, 64), rgb(82, 136, 80));
/// Dry ground: the cream-tan of the painting's paths.
pub const DRY_TOP: Tones = Tones::new(rgb(164, 134, 64), rgb(200, 174, 92), rgb(226, 204, 124));
/// High ground: the grey-violet of the painting's distant mountains.
pub const PEAK_TOP: Tones = Tones::new(rgb(112, 106, 138), rgb(148, 145, 174), rgb(188, 184, 208));
/// The walls under high ground: `PEAK_TOP`'s grey-violet, darker, so a range
/// reads as one family. Chosen by eye, not from a painting.
pub const PEAK_CLIFF: Tones = Tones::new(rgb(70, 64, 98), rgb(98, 92, 128), rgb(132, 128, 160));
/// Volcanic ground: the maroon of the magma background.
pub const VOLCANIC_TOP: Tones = Tones::new(rgb(92, 30, 42), rgb(124, 36, 56), rgb(162, 62, 72));
/// Biome patch palettes for gentle, unfrozen ground, besides `GREEN.top`.
pub const FOREST_TOP: Tones = Tones::new(rgb(56, 108, 40), rgb(80, 136, 46), rgb(108, 162, 60));
pub const MEADOW_TOP: Tones = Tones::new(rgb(108, 138, 44), rgb(146, 168, 58), rgb(184, 196, 90));

/// The atlas style's biome colours.
pub fn biome_rgb(biome: Biome) -> Colour {
    match biome {
        Biome::Forest(Forest::TropicalRain) => [0.33, 0.52, 0.30],
        Biome::Desert(Desert::Hot) => [0.87, 0.76, 0.52],
        // The lime of the field.
        Biome::Grassland(Grassland::Steppe) => [0.72, 0.80, 0.38],
        Biome::Polar(Polar::Tundra) => [0.70, 0.71, 0.61],
        Biome::Polar(Polar::Ice) => [0.94, 0.95, 0.96],
        // The grey of the ridge.
        Biome::Alpine => [0.62, 0.60, 0.56],
        // The maroon of below.
        Biome::Volcanic => [0.45, 0.16, 0.20],
        _ => [0.50, 0.66, 0.36],
    }
}
