//! The sea: cel depth regions, with the floor showing through near the
//! coasts.
//!
//! Three parts, kept apart so things can stand on the floor later
//! (underwater bases, wrecks) and be seen through the same water:
//!
//! 1. **The regions** ([`region`]): a few flat sea colours by depth, shallow
//!    turquoise to deep navy, with soft edges: the cel base.
//! 2. **The floor** ([`floor`]): sand in the shallows giving way to rock as
//!    the water deepens and as the floor steepens, lit by its own relief.
//! 3. **The water between** ([`sea`]): the floor seen through it fades by
//!    depth, red fastest, then green, then blue (Beer–Lambert absorption,
//!    [`through`]), toward the region's colour; how much of it shows is
//!    stepped into a few levels so the shallows read as distinct clearer
//!    tones rather than a photographic fade. Where the water is deep, only
//!    the region shows.
//!
//! The same functions serve the chart (CPU) and a renderer's shader, which
//! mirrors them, so the two agree.

use crate::colour::{Colour, mix, rgb, smoothstep};
use crate::palette::Tones;
use world_bake::Ramp;

/// How the sea floor and the water over it are drawn. Starting values tuned
/// by eye, except where a field says otherwise.
#[derive(Clone, Debug, PartialEq)]
pub struct WaterStyle {
    /// Sand on the floor: the beach's sand (236, 222, 170).
    pub sand: Tones,
    /// Rock on the floor: a grey stone, chosen by eye (not from a
    /// painting). The land cliffs' dark brown made shelf water darker than
    /// the deep sea, a murky ring round every coast.
    pub rock: Tones,
    /// The deep floor by its age ([`tone`]): young basalt, dark, where the
    /// floor has just formed at a ridge, and the pale sediment that buries
    /// it as it ages. Chosen by eye.
    pub basalt: Colour,
    pub sediment: Colour,
    /// The floor's age, millions of years, from bare basalt to buried in
    /// sediment, and how many flat bands that run is cut into: bands of
    /// age lie along the ridges that made them, as on a real map of the
    /// sea floor's age.
    pub age: Ramp,
    pub age_bands: f32,
    /// The cool the bare floor's shaded slopes lean to, in place of the
    /// land's violet ([`crate::relief::ReliefStyle::cool`]): a deep
    /// teal-navy, the memory of the water. Violet on grey rock reads as a
    /// stain.
    pub floor_shadow: Colour,
    /// Depth (0..1 of the deepest sea) from all sand to all rock.
    pub sand_to_rock: Ramp,
    /// Floor slope (rise per radian, in depth units) from settled sediment
    /// to bare rock: steep floor holds no sand at any depth.
    pub steep: Ramp,
    /// How fast each channel (red, green, blue) fades per unit of the
    /// water's optical thickness ([`WaterStyle::depth_curve`]): red first,
    /// then green, then blue, as in real water.
    pub absorption: [f32; 3],
    /// The water's optical thickness is depth (0..1 of the deepest sea)
    /// raised to this power. Depth is measured against the deepest trench,
    /// which crams the shelves and slopes into the bottom few percent; a
    /// power above 1 stretches the shallows, as a hypsometric tint
    /// stretches the lowlands, so they stay clear while the abyss still
    /// reaches the water's own colour.
    pub depth_curve: f32,
    /// The cel depth regions, shallow to deep: each the depth (0..1 of the
    /// deepest sea) it runs to and its colour. The flat-banded sea's
    /// regions, kept as the base.
    pub regions: Vec<(f32, Colour)>,
    /// How far a region's edge wanders, in depth: enough to bend an edge a
    /// few pixels, not to break up a region (the caller adds the noise).
    pub region_wander: f32,
    /// Pixels a region edge is blended across, each side.
    pub region_softness: f32,
    /// Each edge's own share of that softness, shallowest edge first (an
    /// edge past the list's end takes 1). Hard edges come forward and soft
    /// ones go back: the coast's regions are crisp, and the ridges' edge
    /// against the abyss is the softest on the planet.
    pub region_edges: Vec<f32>,
    /// How much of the floor shows is stepped into this many levels, with
    /// edges blended over this share of a level, so the shallows read as a
    /// few distinct clearer tones.
    pub clarity_steps: f32,
    pub clarity_softness: f32,
    /// The most of the floor that ever shows, at the clearest level: under
    /// water nothing is the beach's cream, so the shallowest water reads as
    /// a pale aqua and the cream starts at the dry sand
    /// (`docs/perps-world-map/art-bible.md` in the compositor).
    pub clarity_ceiling: f32,
    /// Light the floor's relief takes: how strongly its lit and shaded
    /// slopes differ from flat.
    pub floor_relief: f32,
    /// Vertical exaggeration of the floor's relief for its light: depth
    /// (0..1 of the deepest) per radian, as height for
    /// [`crate::relief::light`].
    pub floor_z: f32,
}

impl Default for WaterStyle {
    fn default() -> Self {
        Self {
            sand: Tones::new(rgb(206, 190, 140), rgb(236, 222, 170), rgb(248, 240, 200)),
            rock: Tones::new(rgb(88, 82, 74), rgb(120, 112, 98), rgb(156, 146, 126)),
            // A dark grey, not a black: young floor covers whole basins, and
            // a fill that dark belongs to ink. The rift's scar is darker.
            basalt: rgb(74, 72, 84),
            sediment: rgb(150, 142, 128),
            age: Ramp::new(5.0, 90.0),
            age_bands: 4.0,
            floor_shadow: rgb(30, 52, 84),
            sand_to_rock: Ramp::new(0.05, 0.35),
            steep: Ramp::new(2.0, 8.0),
            absorption: [40.0, 16.0, 7.0],
            depth_curve: 1.6,
            regions: vec![
                (0.06, rgb(132, 214, 214)),
                (0.22, rgb(84, 176, 202)),
                // The open sea, ridge and abyss, sits within a few points
                // of lightness: a mid-ocean ridge is a whisper, not a
                // shelf.
                (0.5, rgb(48, 86, 158)),
                (f32::INFINITY, rgb(40, 62, 132)),
            ],
            region_wander: 0.012,
            region_softness: 1.5,
            region_edges: vec![0.5, 0.7, 5.0],
            clarity_steps: 3.0,
            clarity_softness: 0.15,
            // 0.45 left the shallows a pale mint the colour of the ice.
            clarity_ceiling: 0.25,
            floor_relief: 0.6,
            floor_z: 0.1,
        }
    }
}

/// The cel region's colour at `depth` (0..1 of the deepest sea), its edges
/// blended over `soft` of depth either side, each by its own share
/// ([`WaterStyle::region_edges`]).
pub fn region(depth: f32, soft: f32, style: &WaterStyle) -> Colour {
    let regions = &style.regions;
    // A hard edge still needs a span to divide by.
    let soft = soft.max(1e-6);
    let mut colour = regions[0].1;
    for k in 0..regions.len().saturating_sub(1) {
        let limit = regions[k].0;
        let soft = soft * style.region_edges.get(k).copied().unwrap_or(1.0);
        colour = mix(
            colour,
            regions[k + 1].1,
            smoothstep(limit - soft, limit + soft, depth),
        );
    }
    colour
}

/// A sea pixel's colour: the floor (`floor`, [`floor`]) seen through water
/// `depth` deep, over the cel region at `region_depth` (the depth with its
/// edge wander) blended over `soft` of depth.
pub fn sea(floor: Colour, depth: f32, region_depth: f32, soft: f32, style: &WaterStyle) -> Colour {
    let water = region(region_depth, soft, style);
    let seen = through(floor, depth, water, style);
    // How much of the floor shows: the light kept through the water,
    // averaged over the channels, stepped into a few levels.
    let thickness = depth.max(0.0).powf(style.depth_curve);
    let kept = style
        .absorption
        .iter()
        .map(|k| (-k * thickness).exp())
        .sum::<f32>()
        / 3.0;
    mix(
        water,
        seen,
        style.clarity_ceiling * stepped(kept, style.clarity_steps, style.clarity_softness),
    )
}

/// `v` (0..1) stepped into `steps` levels, each edge blended over
/// `softness` of a level.
fn stepped(v: f32, steps: f32, softness: f32) -> f32 {
    let x = v.clamp(0.0, 1.0) * steps;
    let k = x.floor();
    let f = x - k;
    ((k + smoothstep(0.5 - softness, 0.5 + softness, f)) / steps).min(1.0)
}

/// The sea floor's colour at `depth` (0..1 of the deepest sea), with its
/// `slope` (rise per radian, in depth units) and the light on it (`lit`,
/// 1 on flat floor; [`crate::relief::light`]).
pub fn floor(depth: f32, slope: f32, lit: f32, style: &WaterStyle) -> Colour {
    let rock = rock(depth, slope, style);
    let tone = |t: &Tones| {
        // Light and shade on the floor's own relief, eased toward flat.
        let l = 1.0 + (lit - 1.0) * style.floor_relief;
        if l >= 1.0 {
            mix(t.mid, t.light, (l - 1.0).min(1.0))
        } else {
            mix(t.mid, t.shadow, (1.0 - l).min(1.0))
        }
    };
    mix(tone(&style.sand), tone(&style.rock), rock)
}

/// How much of the floor is rock rather than sand, 0..1: more with depth,
/// and all of it where the floor is steep.
pub fn rock(depth: f32, slope: f32, style: &WaterStyle) -> f32 {
    style.sand_to_rock.at(depth).max(style.steep.at(slope))
}

/// Where each of the floor's materials sits on its tone ([`tone`]), 0..1:
/// sand, sediment, rock, basalt. A renderer's shader mirrors them.
pub const TONE_STOPS: [f32; 4] = [0.0, 0.4, 0.7, 1.0];

/// What the sea floor is made of, as one number 0..1 along
/// [`TONE_STOPS`], for a renderer that draws the floor bare: sand on the
/// shelves; below them, by the floor's `age` in millions of years, young
/// basalt near the ridge that made it paling to old sediment in bands; and
/// bare rock wherever it is steep. Without an age (a bake with no tectonic
/// history) the deep floor is rock, as [`rock`] has it.
pub fn tone(depth: f32, slope: f32, age: Option<f32>, style: &WaterStyle) -> f32 {
    let [sand, sediment, rock, basalt] = TONE_STOPS;
    let deep = match age {
        Some(age) => {
            let bands = style.age_bands.max(1.0);
            let old = (style.age.at(age) * bands).round() / bands;
            basalt + (sediment - basalt) * old
        }
        None => rock,
    };
    let deep = deep + (rock - deep) * style.steep.at(slope);
    sand + (deep - sand) * style.sand_to_rock.at(depth)
}

/// The bare floor's colour at a [`tone`], unlit.
pub fn tone_colour(tone: f32, style: &WaterStyle) -> Colour {
    let [_, sediment, rock, basalt] = TONE_STOPS;
    let between = |a: f32, b: f32| ((tone - a) / (b - a)).clamp(0.0, 1.0);
    let c = mix(style.sand.mid, style.sediment, between(0.0, sediment));
    let c = mix(c, style.rock.mid, between(sediment, rock));
    mix(c, style.basalt, between(rock, basalt))
}

/// One pixel of the sea floor for a renderer that draws the water itself,
/// as RGBA8:
///
/// - R: depth (0..1 of the deepest sea) as its square root, so the
///   shallows, where the floor shows, get most of the 8 bits. A renderer
///   squares it back.
/// - G: what the floor is made of: its rock share ([`rock`]), or its
///   [`tone`] for a renderer that knows [`TONE_STOPS`].
/// - B: reserved for structures on the floor (underwater bases, wrecks),
///   0 for now.
/// - A: 255 at sea, 0 on land.
pub fn encode_floor(depth: f32, rock: f32, sea: bool) -> [u8; 4] {
    if !sea {
        return [0, 0, 0, 0];
    }
    let unit = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [unit(depth.max(0.0).sqrt()), unit(rock), 0, 255]
}

/// `floor` seen through water `depth` deep (0..1 of the deepest sea) whose
/// own colour is `water`.
pub fn through(floor: Colour, depth: f32, water: Colour, style: &WaterStyle) -> Colour {
    let thickness = depth.max(0.0).powf(style.depth_curve);
    std::array::from_fn(|c| {
        let kept = (-style.absorption[c] * thickness).exp();
        floor[c] * kept + water[c] * (1.0 - kept)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_floor_runs_from_sand_to_rock_with_depth_and_steepness() {
        let style = WaterStyle::default();
        // A full blend lands within float rounding of its end.
        let near = |a: Colour, b: Colour| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
        assert!(near(floor(0.0, 0.0, 1.0, &style), style.sand.mid));
        assert!(near(floor(0.5, 0.0, 1.0, &style), style.rock.mid));
        // Steep shallow floor is rock too.
        assert!(near(floor(0.0, 10.0, 1.0, &style), style.rock.mid));
    }

    #[test]
    fn the_bare_floor_is_sand_on_shelves_and_pales_with_age_below_them() {
        let style = WaterStyle::default();
        let near = |a: Colour, b: Colour| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
        let colour = |depth, slope, age| tone_colour(tone(depth, slope, age, &style), &style);
        // A shelf is sand whatever its age.
        assert!(near(colour(0.0, 0.0, Some(0.0)), style.sand.mid));
        // Deep floor: basalt where it has just formed, sediment once old.
        assert!(near(colour(0.6, 0.0, Some(0.0)), style.basalt));
        assert!(near(colour(0.6, 0.0, Some(200.0)), style.sediment));
        // Steep floor is bare rock at any age, and all of it with no age.
        assert!(near(colour(0.6, 10.0, Some(0.0)), style.rock.mid));
        assert!(near(colour(0.6, 0.0, None), style.rock.mid));
        // Age comes in flat bands: two ages in one band are one tone.
        assert_eq!(tone(0.6, 0.0, Some(60.0), &style), tone(0.6, 0.0, Some(70.0), &style));
        assert_ne!(tone(0.6, 0.0, Some(20.0), &style), tone(0.6, 0.0, Some(70.0), &style));
    }

    #[test]
    fn the_floor_sheet_keeps_shallow_depths_fine_and_reserves_a_slot() {
        let [r, g, b, a] = encode_floor(0.04, 0.25, true);
        // sqrt(0.04) = 0.2: the shallows spread over the low byte range.
        assert_eq!(r, 51);
        assert_eq!(g, 64);
        assert_eq!(b, 0, "reserved for structures");
        assert_eq!(a, 255);
        assert_eq!(encode_floor(0.5, 1.0, false), [0, 0, 0, 0], "land");
    }

    #[test]
    fn shallow_water_shows_the_floor_and_deep_water_only_itself() {
        let style = WaterStyle::default();
        let sand = style.sand.mid;
        let water = style.regions.last().unwrap().1;
        assert_eq!(through(sand, 0.0, water, &style), sand);
        let deep = through(sand, 1.0, water, &style);
        assert!(
            deep.iter().zip(water).all(|(a, b)| (a - b).abs() < 1e-3),
            "{deep:?}"
        );
        // Red goes first: a little water over sand is bluer than the sand.
        let shallow = through(sand, 0.02, water, &style);
        assert!(shallow[0] / shallow[2] < sand[0] / sand[2], "{shallow:?}");
    }

    #[test]
    fn the_regions_are_flat_inside_and_soft_only_at_their_edges() {
        let style = WaterStyle::default();
        let colour = |i: usize| style.regions[i].1;
        assert_eq!(region(0.0, 0.01, &style), colour(0));
        assert_eq!(region(0.14, 0.01, &style), colour(1));
        assert_eq!(region(0.9, 0.01, &style), colour(3));
        // Halfway across an edge, halfway between its colours.
        let edge = region(0.22, 0.01, &style);
        let half = mix(colour(1), colour(2), 0.5);
        assert!(edge.iter().zip(half).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn the_floor_shows_in_steps_and_not_at_all_in_deep_water() {
        let style = WaterStyle::default();
        let sand = style.sand.mid;
        // Deep: only the region.
        let deep = 0.6;
        assert_eq!(
            sea(sand, deep, deep, 0.0, &style),
            region(deep, 0.0, &style)
        );
        // Clarity is flat within a step: two depths a little apart in the
        // same step show the floor equally.
        assert_eq!(stepped(0.40, 3.0, 0.15), stepped(0.45, 3.0, 0.15));
        assert_eq!(stepped(0.40, 3.0, 0.15), 1.0 / 3.0);
        assert_eq!(stepped(1.0, 3.0, 0.15), 1.0);
        assert_eq!(stepped(0.0, 3.0, 0.15), 0.0);
    }
}
