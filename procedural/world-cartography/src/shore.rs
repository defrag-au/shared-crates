//! The shore's strip, drawn on the land side of the coast from the chart's
//! own distance to it, as wide as the chart's scale wants. The kind comes
//! from the bake ([`world_bake::Shore::classify`] on its shore layers,
//! under the params it recorded); each kind's look is one match,
//! [`strip`].

use world_bake::{Marsh, Shore};

use crate::colour::{Colour, mix, rgb};
use crate::palette::Tones;

/// One pixel's shore: its kind, and how strongly it is that kind (0..1).
#[derive(Clone, Copy, Debug)]
pub struct Strip {
    pub kind: Shore,
    pub strength: f32,
}

impl Default for Strip {
    fn default() -> Self {
        Self {
            kind: Shore::Rocky,
            strength: 0.0,
        }
    }
}

/// How a kind of shore draws: its colour and widest strip, pixels (wider
/// than real at a world map's scale, as the swell's crests are). Rocky
/// shore has no strip: rock meets the sea.
pub fn strip(kind: Shore) -> Option<(Colour, f32)> {
    match kind {
        Shore::Rocky => None,
        // The dry ground's light tone, the cream of the green painting's
        // paths (226, 204, 124), a third of the way to white.
        Shore::Beach => Some((rgb(236, 222, 170), 3.0)),
        // Halfway between the boreal forest's mid green (58, 110, 64) and
        // the dry ground's mid tan (200, 174, 92): an olive mud.
        Shore::Marsh(Marsh::Salt) => Some((rgb(129, 142, 78), 4.0)),
        // Halfway between that mud and the boreal forest's shadow green
        // (40, 86, 52): mangrove.
        Shore::Marsh(Marsh::Mangrove) => Some((rgb(85, 114, 65), 4.0)),
    }
}

/// The strip's colour over `ground` at signed distance `sd` (pixels,
/// negative on land) from the coast.
pub fn paint(ground: Colour, sd: f32, shore: Strip) -> Colour {
    let Some((colour, widest)) = strip(shore.kind) else {
        return ground;
    };
    if sd >= 0.0 {
        return ground;
    }
    // Box-filtered coverage of the band from the coast to the strip's width.
    let cover = (widest * shore.strength + sd + 0.5).clamp(0.0, 1.0);
    mix(ground, colour, cover)
}

/// How rocky shores draw. Starting values tuned by eye.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShoreStyle {
    /// Widest cliff face, pixels, on a fully rocky shore: narrower than a
    /// beach, a wall seen from above.
    pub cliff_width: f32,
    /// How much harder surf breaks against a fully rocky shore than on
    /// sand, as a share of its strength.
    pub rock_surf: f32,
}

impl Default for ShoreStyle {
    fn default() -> Self {
        Self {
            cliff_width: 2.0,
            rock_surf: 0.5,
        }
    }
}

/// A rocky shore's cliff face over `ground` at signed distance `sd`
/// (pixels, negative on land): a narrow band of the look's own cliff tones,
/// light on a shore facing the light and shadow on one facing away
/// (`shadow`, 0 lit … 1 away; `crate::coast::shadow_side`), so the face
/// reads as a wall rather than as missing sand. Other kinds of shore are
/// left as they are.
pub fn cliff_face(
    ground: Colour,
    sd: f32,
    shore: Strip,
    cliff: &Tones,
    shadow: f32,
    style: &ShoreStyle,
) -> Colour {
    if shore.kind != Shore::Rocky || sd >= 0.0 {
        return ground;
    }
    let face = mix(cliff.light, cliff.shadow, shadow);
    let cover = (style.cliff_width * shore.strength + sd + 0.5).clamp(0.0, 1.0);
    mix(ground, face, cover)
}

/// How rocky the shore at a sea pixel is, 0..1, for the surf that breaks
/// against it.
pub fn rockiness(shore: Strip) -> f32 {
    if shore.kind == Shore::Rocky {
        shore.strength
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_beach_is_sand_at_the_shore_and_ground_inland() {
        let ground = rgb(10, 100, 10);
        let beach = Strip {
            kind: Shore::Beach,
            strength: 1.0,
        };
        let sand = strip(Shore::Beach).expect("beaches draw").0;
        assert_eq!(paint(ground, -0.5, beach), sand);
        assert_eq!(paint(ground, -5.0, beach), ground);
        // Rock meets the sea.
        let rocky = Strip {
            kind: Shore::Rocky,
            strength: 1.0,
        };
        assert_eq!(paint(ground, -0.5, rocky), ground);
    }

    #[test]
    fn a_rocky_shore_shows_its_face_in_the_light_and_shade() {
        let ground = rgb(10, 100, 10);
        let cliff = Tones::new(rgb(20, 20, 20), rgb(100, 100, 100), rgb(200, 200, 200));
        let rocky = Strip {
            kind: Shore::Rocky,
            strength: 1.0,
        };
        let style = ShoreStyle::default();
        let face = |sd, shore, shadow| cliff_face(ground, sd, shore, &cliff, shadow, &style);
        // Blending to a tone lands within float rounding of it.
        let near = |a: Colour, b: Colour| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
        assert!(near(face(-0.5, rocky, 0.0), cliff.light));
        assert!(near(face(-0.5, rocky, 1.0), cliff.shadow));
        assert_eq!(face(-5.0, rocky, 1.0), ground);
        // A beach has no face.
        let beach = Strip {
            kind: Shore::Beach,
            strength: 1.0,
        };
        assert_eq!(face(-0.5, beach, 1.0), ground);
    }
}
