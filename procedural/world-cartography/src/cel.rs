//! The cel style: terraced mesas, flat banded light and an ink hierarchy, in
//! the palette of the Derp Birds paintings. Composes [`crate::terrain`],
//! [`crate::sea`], [`crate::craters`] and [`crate::rivers`] into pixels;
//! [`crate::trees`] is stamped on top afterwards.

use glam::Vec3;
use rayon::prelude::*;

use crate::coast;
use crate::colour::{mix, to_u8};
use crate::craters::{self, Crater};
use crate::noise::{gradient_noise, grain};
use crate::palette::{
    BOREAL_TOP, DESERT, DRY_TOP, FOREST_TOP, FROZEN, GREEN, MEADOW_TOP, PEAK_TOP, TUNDRA_TOP,
    Tones, VOLCANIC_TOP,
};
use crate::relief;
use crate::relief::Contrast;
use crate::sea;
use crate::sheet::{Biome, Cell, Forest, Grid, Polar};
use crate::shore::Strip;
use crate::style::Style;
use crate::terrain::{self, INK_MESA_EDGE, INK_TIER, Terrain};
use crate::waves::{self, Swell};

/// Everything the cel style draws from.
pub struct Chart<'a> {
    pub cells: &'a [Cell],
    pub grid: &'a Grid,
    /// River coverage, 0..1 per cell (see [`crate::rivers`]).
    pub river: &'a [f32],
    /// Sea depth, 0..1 per cell (see [`crate::sea::depth`]).
    pub sea_depth: &'a [f32],
    pub craters: &'a [Crater],
    pub sea: f32,
    pub relief: f32,
    pub tiers: f32,
    pub z_scale: f32,
    /// Heights come from the detail layer, not the smoothed base.
    pub detailed: bool,
    /// Paint the light into the land's colour. Off for a renderer that lights
    /// the relief itself: every surface takes its middle band.
    pub shade: bool,
    /// Paint the light into the craters' walls. Off for a renderer that
    /// stands craters up as real relief (the globe) and lights them there.
    pub crater_shade: bool,
    /// Draw the sea's fine lines: waterlines echoing the shore, and swell
    /// crests where there is swell. Off for sheets seen whole on a globe,
    /// where lines a pixel wide alias and clutter, and whose renderer draws
    /// moving crests itself.
    pub waterlines: bool,
    /// The bake's swell per cell, when it has the sea layers.
    pub swell: Option<&'a [Swell]>,
    /// The bake's shore per cell, when it has the shore layers.
    pub shore: Option<&'a [Strip]>,
    /// How it is all drawn.
    pub style: &'a Style,
}

/// The drawn sheet, and the land analysis it was drawn from.
pub struct Drawn {
    pub pixels: Vec<[u8; 3]>,
    pub terrain: Terrain,
}

pub fn render(chart: &Chart) -> Drawn {
    let Chart {
        cells,
        grid,
        river,
        sea_depth,
        craters,
        sea,
        relief,
        tiers,
        z_scale,
        detailed,
        shade,
        crater_shade,
        waterlines,
        swell,
        shore: strips,
        style,
    } = *chart;
    let lighting = &style.relief;
    // Unlit, every surface sits in its middle band.
    let paint = |set: &Tones, lit: f32, height: f32, contrast: Contrast| {
        if shade {
            relief::paint(set, lit, height, contrast, lighting)
        } else {
            set.mid
        }
    };
    let t = terrain::analyse(cells, grid, sea, relief, tiers, detailed);
    let lines = waterlines.then(|| coast::Waterlines::new(&t.coast, grid, &style.waterlines));
    // The height the tops' light reads: the land form, generalised.
    let forms: Vec<f32> = cells.iter().map(|c| c.form).collect();
    let shading = relief::shading_height(&forms, grid, lighting);

    let pixels: Vec<[u8; 3]> = (0..cells.len())
        .into_par_iter()
        .map(|i| {
            let here = &cells[i];
            if !here.valid {
                return [16, 20, 38];
            }
            let at = |dx: i32, dy: i32| grid.clamped(i, dx, dy);
            let four = [at(1, 0), at(-1, 0), at(0, 1), at(0, -1)];
            let lat_scale = grid.lat_scale(here.dir);
            let shadow_side = coast::shadow_side(&t.coast, i, grid, lat_scale);
            let shore = coast::marks(t.coast[i], shadow_side, &style.coast);

            if !t.is_land[i] {
                let ink = sea::Ink {
                    coast: &shore,
                    waterline: lines.as_ref().map_or(0.0, |w| w.ink(i, here.dir)),
                    // The chart is one moment: the clock at zero.
                    crest: match swell {
                        Some(s) if waterlines => waves::crest(i, s, grid, 0.0, &style.waves),
                        _ => 0.0,
                    },
                    surf: swell.map_or(1.0, |s| s[i].exposure),
                    rock: strips.map_or(0.0, |s| crate::shore::rockiness(s[i])),
                };
                return sea::cel(i, here.dir, four, sea_depth, &t.is_land, &ink, style);
            }

            let look = match here.biome {
                _ if t.frozen[i] => &FROZEN,
                Biome::Desert(_) => &DESERT,
                _ => &GREEN,
            };
            let (dx, dy) = (grid.step * lat_scale, grid.step);
            let z = relief * z_scale;
            let ex = (t.terraced[four[0]] - t.terraced[four[1]]) / (2.0 * dx) * z;
            let ey = (t.terraced[four[2]] - t.terraced[four[3]]) / (2.0 * dy) * z;
            let lit = relief::light(ex, ey, lighting);
            let cliff = t.is_cliff(i);
            let height = ((here.form - sea) / relief).clamp(0.0, 1.0);

            let mut colour = if cliff {
                paint(&look.cliff, lit, height, lighting.cliffs)
            } else {
                // Tops: shade from the generalised land form, not the
                // terrace.
                let fx = (shading[four[0]] - shading[four[1]]) / (2.0 * dx) * z_scale;
                let fy = (shading[four[2]] - shading[four[3]]) / (2.0 * dy) * z_scale;
                let form = relief::light(fx, fy, lighting);
                let tier = t.smooth_u[i].floor();
                let wet = here.moisture + (grain(here.dir, 18.0) - 0.5) * 0.14;
                // Large, soft-edged patches of a second grass break up the
                // plains; forest patches sit darker under their trees.
                let meadow = gradient_noise(here.dir * 4.0 + Vec3::splat(3.3)) > 0.12;
                // Volcanoes, ice and peaks override the biome; then the
                // biome's kind; then, on green ground, how dry it runs and
                // whether a forest or meadow patch stands there.
                let set = if here.biome == Biome::Volcanic {
                    &VOLCANIC_TOP
                } else if t.frozen[i] {
                    &look.top
                } else if tier >= tiers - 2.0 {
                    &PEAK_TOP
                } else {
                    match here.biome {
                        Biome::Desert(_) => &look.top,
                        Biome::Polar(Polar::Tundra) => &TUNDRA_TOP,
                        Biome::Grassland(_) => &DRY_TOP,
                        _ if wet < 0.24 => &DRY_TOP,
                        Biome::Forest(Forest::Boreal) => &BOREAL_TOP,
                        _ if t.forest[i] => &FOREST_TOP,
                        _ if meadow => &MEADOW_TOP,
                        _ => &look.top,
                    }
                };
                let contrast = if t.frozen[i] {
                    lighting.ice
                } else {
                    lighting.ground
                };
                paint(set, form, height, contrast)
            };

            if let Some(spot) = craters::spot(craters, here.dir) {
                colour = craters::draw(colour, &spot, grid.step, look, crater_shade);
            }

            // The shore, from the coast's distance field: the land's own
            // colour darkened just inside it, then a beach or marsh strip
            // where the bake says one suits, or a cliff face where neither
            // does (not on ice).
            colour = mix(
                colour,
                mix(colour, [0.0; 3], style.coast.shade_darken),
                shore.shade,
            );
            if let (Some(strips), false) = (strips, t.frozen[i]) {
                colour = crate::shore::paint(colour, t.coast[i], strips[i]);
                colour = crate::shore::cliff_face(
                    colour,
                    t.coast[i],
                    strips[i],
                    &look.cliff,
                    shadow_side,
                    &style.shore,
                );
            }

            // Rivers, across the strip to the sea, and falls where a river
            // crosses a cliff, blended by the stroke's anti-aliased coverage.
            let water = if cliff { look.fall } else { look.river };
            colour = mix(colour, water, river[i]);

            // The stroke, in the water's hue, so no land ink on the coast.
            colour = mix(colour, style.coast.stroke, shore.stroke);

            let weight = match t.ink[i] {
                INK_MESA_EDGE => 0.7,
                INK_TIER => 0.5,
                _ => 0.0,
            };
            colour = mix(colour, look.ink, weight);
            to_u8(colour)
        })
        .collect();

    Drawn { pixels, terrain: t }
}
