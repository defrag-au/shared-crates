//! The atlas style: smooth multi-light shaded relief, biome colours graded by
//! climate, depth-tinted sea with isobaths. Optionally lit from an external
//! relief render (e.g. Blender) blended with its own hillshade.

use glam::Vec3;
use rayon::prelude::*;

use crate::FREEZING_K;
use crate::colour::{mix, smoothstep};
use crate::craters::Crater;
use crate::noise::grain;
use crate::palette::biome_rgb;
use crate::sheet::{Biome, Cell, Desert, Forest, Grassland, Grid, Polar};

/// Everything the atlas style draws from.
pub struct Chart<'a> {
    pub cells: &'a [Cell],
    pub grid: &'a Grid,
    pub river: &'a [f32],
    pub sea_depth: &'a [f32],
    pub craters: &'a [Crater],
    pub relief: f32,
    pub z_scale: f32,
    /// Hillshade the land (otherwise flat colour, for an external shade to
    /// multiply over).
    pub shade: bool,
    /// An external relief render per cell, normalised so flat ground is 1.0.
    pub external_shade: Option<&'a [f32]>,
    /// How much of the light comes from `external_shade` (1.0 = all of it).
    pub shade_mix: f32,
}

pub fn render(chart: &Chart) -> Vec<[u8; 3]> {
    let Chart {
        cells,
        grid,
        river,
        sea_depth,
        craters,
        relief,
        z_scale,
        shade,
        external_shade,
        shade_mix,
    } = *chart;
    let lights: [(Vec3, f32); 3] = [
        (Vec3::new(-1.0, -1.0, 1.414).normalize(), 0.55),
        (Vec3::new(-1.0, 0.0, 1.0).normalize(), 0.25),
        (Vec3::new(0.0, -1.0, 1.0).normalize(), 0.20),
    ];
    let flat = lights.iter().map(|(l, w)| l.z * w).sum::<f32>();
    let isobaths = [0.12_f32, 0.3, 0.55];

    (0..cells.len())
        .into_par_iter()
        .map(|i| {
            let here = &cells[i];
            if !here.valid {
                return [16, 20, 38];
            }
            let at = |dx: i32, dy: i32| &cells[grid.clamped(i, dx, dy)];
            let dx = grid.step * grid.lat_scale(here.dir);
            let dy = grid.step;

            // Coast coverage from the signed land score, anti-aliased over a
            // pixel by its local gradient.
            let gx = (at(1, 0).land - at(-1, 0).land) * 0.5;
            let gy = (at(0, 1).land - at(0, -1).land) * 0.5;
            let grad = (gx * gx + gy * gy).sqrt().max(1e-5);
            let coverage = (here.land / grad + 0.5).clamp(0.0, 1.0);

            // Land.
            let ex = (at(1, 0).elevation - at(-1, 0).elevation) / (2.0 * dx) * z_scale;
            let ey = (at(0, 1).elevation - at(0, -1).elevation) / (2.0 * dy) * z_scale;
            let normal = Vec3::new(-ex, -ey, 1.0).normalize();
            let internal = lights
                .iter()
                .map(|(l, weight)| normal.dot(*l).max(0.0) * weight)
                .sum::<f32>()
                / flat;
            // Geometric blend: the external plate's shadows and broad form,
            // the internal shade's crisp slopes.
            let lit = match external_shade {
                Some(shade) => {
                    internal.max(0.05).powf(1.0 - shade_mix) * shade[i].max(0.05).powf(shade_mix)
                }
                None => internal,
            };
            let slope = (ex * ex + ey * ey).sqrt();

            let kelvin = here.temperature;
            let wet = here.moisture + (grain(here.dir, 24.0) - 0.5) * 0.12;
            let desert = biome_rgb(Biome::Desert(Desert::Hot));
            let steppe = biome_rgb(Biome::Grassland(Grassland::Steppe));
            let rainforest = biome_rgb(Biome::Forest(Forest::TropicalRain));
            let climate = if wet < 0.3 {
                mix(desert, steppe, wet / 0.3)
            } else {
                mix(steppe, rainforest, (wet - 0.3) / 0.4)
            };
            let green = mix(climate, [0.27, 0.45, 0.25], here.vegetation * 0.55);
            let tundra = biome_rgb(Biome::Polar(Polar::Tundra));
            let cold = mix(green, tundra, (FREEZING_K + 6.0 - kelvin) / 14.0);
            let base = if here.biome == Biome::Volcanic {
                mix(cold, biome_rgb(Biome::Volcanic), 0.8)
            } else {
                cold
            };
            let rock = mix(
                base,
                biome_rgb(Biome::Alpine),
                smoothstep(0.35, 1.1, slope) * (0.4 + here.roughness * 0.6),
            );
            let snowline = smoothstep(FREEZING_K, FREEZING_K - 8.0, kelvin)
                * (1.0 - smoothstep(0.9, 2.2, slope) * 0.6);
            let snowy = mix(rock, biome_rgb(Biome::Polar(Polar::Ice)), snowline);
            let scorch = (-world_bake::crater_offset(craters, here.dir.to_array())
                / (relief * 0.2))
                .clamp(0.0, 1.0);
            let mut land = mix(snowy, [0.22, 0.15, 0.18], scorch * 0.7);
            if shade {
                let tone = (0.62 + 0.38 * lit).clamp(0.25, 1.35);
                land = [land[0] * tone, land[1] * tone, land[2] * tone];
                if lit > 1.0 {
                    land = mix(land, [1.0, 0.97, 0.88], (lit - 1.0) * 0.35);
                }
            }
            // Rivers do not run across ice.
            let river_tint = [0.30, 0.52, 0.62];
            land = mix(land, river_tint, river[i] * (1.0 - snowline));

            // Sea.
            let depth = sea_depth[i];
            let mut sea_rgb = mix([0.40, 0.69, 0.72], [0.07, 0.17, 0.28], depth.powf(0.6));
            // Shallows: a smooth halo off the coast instead of rings.
            let shore = smoothstep(-0.5, -0.05, here.land);
            sea_rgb = mix(sea_rgb, [0.62, 0.84, 0.82], shore * 0.55);
            for &contour in &isobaths {
                let right = sea_depth[grid.clamped(i, 1, 0)];
                let down = sea_depth[grid.clamped(i, 0, 1)];
                if (depth - contour).signum() != (right - contour).signum()
                    || (depth - contour).signum() != (down - contour).signum()
                {
                    sea_rgb = mix(sea_rgb, [0.80, 0.90, 0.92], 0.18);
                }
            }

            let mut rgb = mix(sea_rgb, land, coverage);
            // Coastline ink: strongest where coverage crosses one half.
            let edge = 1.0 - (coverage * 2.0 - 1.0).abs();
            rgb = mix(rgb, [0.12, 0.13, 0.15], edge * 0.65);

            [
                (rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
                (rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
                (rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
            ]
        })
        .collect()
}
