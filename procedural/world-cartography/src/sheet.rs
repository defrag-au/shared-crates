//! The sheet: which part of the sphere a picture shows, how pixels map to
//! directions and back, the pixel grid's neighbours, and the [`Cell`] each
//! pixel samples from the bake.

use std::f32::consts::PI;

use glam::Vec3;
use rayon::prelude::*;

/// A direction for a longitude and latitude, radians (`world_bake`'s
/// convention: pole on +y, longitude from +x towards +z).
pub fn lon_lat_to_dir(lon: f32, lat: f32) -> Vec3 {
    Vec3::from_array(world_bake::lon_lat_to_dir(lon, lat))
}

/// A regional view: an orthographic disc centred on a point.
#[derive(Clone, Copy)]
pub struct Region {
    pub centre: Vec3,
    pub east: Vec3,
    pub north: Vec3,
    /// Half the view's width, as the sine of half its angular span.
    pub half: f32,
}

/// What a picture shows: the whole globe (equirectangular) or a region.
#[derive(Clone, Copy)]
pub struct Sheet {
    pub w: usize,
    pub h: usize,
    pub region: Option<Region>,
}

impl Sheet {
    pub fn global(w: usize, h: usize) -> Self {
        Self { w, h, region: None }
    }

    /// A region centred on `lon_deg, lat_deg`, `span_deg` across.
    pub fn regional(w: usize, h: usize, lon_deg: f32, lat_deg: f32, span_deg: f32) -> Self {
        let centre = lon_lat_to_dir(lon_deg.to_radians(), lat_deg.to_radians());
        let east = Vec3::new(-lon_deg.to_radians().sin(), 0.0, lon_deg.to_radians().cos());
        let north = centre.cross(east).normalize() * -1.0;
        let half = (span_deg.to_radians() * 0.5).sin();
        Self {
            w,
            h,
            region: Some(Region {
                centre,
                east,
                north,
                half,
            }),
        }
    }

    pub fn grid(&self) -> Grid {
        Grid {
            w: self.w,
            h: self.h,
            global: self.region.is_none(),
            step: match self.region {
                None => 2.0 * PI / self.w as f32,
                Some(r) => 2.0 * r.half / self.w as f32,
            },
        }
    }

    /// The direction a pixel shows, or `None` off a regional sheet's disc.
    pub fn dir_at(&self, px: usize, py: usize) -> Option<Vec3> {
        let (w, h) = (self.w, self.h);
        match self.region {
            None => {
                let lon = (px as f32 + 0.5) / w as f32 * 2.0 * PI - PI;
                let lat = PI * 0.5 - (py as f32 + 0.5) / h as f32 * PI;
                Some(lon_lat_to_dir(lon, lat))
            }
            Some(Region {
                centre,
                east,
                north,
                half,
            }) => {
                let x = ((px as f32 + 0.5) / w as f32 * 2.0 - 1.0) * half;
                let y = (1.0 - (py as f32 + 0.5) / h as f32 * 2.0) * half * h as f32 / w as f32;
                let r2 = x * x + y * y;
                (r2 < 1.0).then(|| (centre * (1.0 - r2).sqrt() + east * x + north * y).normalize())
            }
        }
    }

    /// A direction's position on the sheet, in pixel coordinates (pixel
    /// centres at `.0`), or `None` if it is off a regional sheet's disc.
    pub fn project(&self, dir: Vec3) -> Option<(f32, f32)> {
        let (w, h) = (self.w, self.h);
        match self.region {
            None => {
                let lat = dir.y.clamp(-1.0, 1.0).asin();
                let lon = dir.z.atan2(dir.x);
                Some((
                    (lon + PI) / (2.0 * PI) * w as f32 - 0.5,
                    (PI * 0.5 - lat) / PI * h as f32 - 0.5,
                ))
            }
            Some(Region {
                centre,
                east,
                north,
                half,
            }) => {
                if dir.dot(centre) <= 0.0 {
                    return None;
                }
                let (x, y) = (dir.dot(east), dir.dot(north));
                let aspect = h as f32 / w as f32;
                Some((
                    (x / half + 1.0) * 0.5 * w as f32 - 0.5,
                    (1.0 - y / (half * aspect)) * 0.5 * h as f32 - 0.5,
                ))
            }
        }
    }
}

/// Pixel grid geometry: which neighbours exist and how far apart they are.
pub struct Grid {
    pub w: usize,
    pub h: usize,
    pub global: bool,
    /// Angular spacing of one pixel, radians, before the latitude correction.
    pub step: f32,
}

impl Grid {
    pub fn neighbour(&self, i: usize, dx: i32, dy: i32) -> Option<usize> {
        let (x, y) = ((i % self.w) as i32, (i / self.w) as i32);
        let ny = y + dy;
        if ny < 0 || ny >= self.h as i32 {
            return None;
        }
        let nx = x + dx;
        let nx = if self.global {
            nx.rem_euclid(self.w as i32)
        } else if nx < 0 || nx >= self.w as i32 {
            return None;
        } else {
            nx
        };
        Some(ny as usize * self.w + nx as usize)
    }

    pub fn clamped(&self, i: usize, dx: i32, dy: i32) -> usize {
        self.neighbour(i, dx, dy).unwrap_or(i)
    }

    /// How much narrower a pixel is east-west than its nominal step at a
    /// direction: the cosine of latitude on the global sheet, 1 on a region.
    pub fn lat_scale(&self, dir: Vec3) -> f32 {
        if self.global {
            dir.y.asin().cos().max(0.05)
        } else {
            1.0
        }
    }
}

/// One pixel's worth of sampled world.
#[derive(Clone, Copy)]
pub struct Cell {
    pub dir: Vec3,
    pub valid: bool,
    /// Final elevation, generator units, with craters and fine relief.
    pub elevation: f32,
    /// The land form without fine relief: what the cel style terraces, so
    /// cliff lines follow the land rather than the noise.
    pub form: f32,
    /// Signed land score: positive on land, negative at sea, zero on the coast.
    pub land: f32,
    pub moisture: f32,
    pub vegetation: f32,
    pub roughness: f32,
    /// Surface temperature from the climate simulation, Kelvin.
    pub temperature: f32,
    /// What grows (or does not) here; [`Biome::Ocean`] at sea.
    pub biome: Biome,
    /// The chart's moment of the year: how much seasonal snow lies, 0 to 1
    /// (the ice sheet is the frozen zone, apart from this).
    pub snow: f32,
    /// The chart's moment of the year: how far the plants are in leaf, 0
    /// (bare or dry-season brown) to 1 (summer green). A chart with no
    /// moment draws 0 snow and full leaf.
    pub leaf: f32,
}

/// The ground a map draws: the generator's Whittaker biomes
/// (`planet_gen::biome::Biome`), restated so this crate reads a world without
/// the generator, and grouped by kind so a style matches on the kind
/// (`Biome::Desert(_)`) and the compiler holds it to every kind. A reader
/// converts at its edge; the map example does so with an exhaustive match,
/// so a biome the generator adds fails to compile there rather than drawing
/// as something else.
///
/// Properties that cut across the kinds (which grow trees) are data on the
/// biome, from one exhaustive match each: [`Biome::tree_cover`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Biome {
    Ocean,
    Desert(Desert),
    Grassland(Grassland),
    Forest(Forest),
    Polar(Polar),
    Wetland,
    Alpine,
    Volcanic,
    Beach,
}

/// Desert: the red painting's ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Desert {
    Hot,
    Cold,
}

/// Open grassland.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grassland {
    Steppe,
    Savanna,
}

/// Forest, by climate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Forest {
    TropicalRain,
    TropicalDry,
    Temperate,
    TemperateRain,
    Boreal,
}

/// Ground too cold for forest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Polar {
    Tundra,
    Ice,
}

/// How trees stand on a biome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeCover {
    /// None: desert, tundra, ice, a volcano's flanks, the sea.
    None,
    /// Lone trees, never a forest: steppe, savanna, wetland.
    Sparse,
    /// Forest where it grows thick enough.
    Forest,
}

impl Biome {
    pub fn tree_cover(self) -> TreeCover {
        match self {
            Biome::Ocean | Biome::Desert(_) | Biome::Polar(_) | Biome::Volcanic => TreeCover::None,
            Biome::Grassland(_) | Biome::Wetland => TreeCover::Sparse,
            Biome::Forest(_) | Biome::Alpine | Biome::Beach => TreeCover::Forest,
        }
    }

    /// The share of what grows here that keeps its leaves all year, 0 to 1:
    /// rainforest and the conifers of the boreal and alpine forests stay
    /// green; grassland and broadleaf forest turn with the season.
    pub fn evergreen(self) -> f32 {
        match self {
            Biome::Forest(Forest::TropicalRain) => 1.0,
            Biome::Forest(Forest::Boreal) | Biome::Alpine => 0.8,
            Biome::Forest(Forest::TemperateRain) => 0.6,
            Biome::Forest(Forest::TropicalDry) | Biome::Wetland => 0.4,
            Biome::Forest(Forest::Temperate) => 0.2,
            Biome::Grassland(_) | Biome::Polar(_) | Biome::Beach => 0.0,
            // Nothing there to turn: drawn as it always is.
            Biome::Ocean | Biome::Desert(_) | Biome::Volcanic => 1.0,
        }
    }
}

/// Separable box blur over the grid, wrapping in longitude on a global sheet.
pub fn box_blur(values: &[f32], grid: &Grid, radius: i32) -> Vec<f32> {
    let pass = |input: &[f32], horizontal: bool| -> Vec<f32> {
        (0..input.len())
            .into_par_iter()
            .map(|i| {
                let mut sum = 0.0;
                let mut count = 0.0;
                for k in -radius..=radius {
                    let (dx, dy) = if horizontal { (k, 0) } else { (0, k) };
                    if let Some(j) = grid.neighbour(i, dx, dy) {
                        sum += input[j];
                        count += 1.0;
                    }
                }
                sum / count
            })
            .collect()
    };
    pass(&pass(values, true), false)
}

/// Label 4-connected components of cells where `member` is true. Returns
/// each cell's component id (`u32::MAX` outside) and each component's size.
pub fn components(grid: &Grid, member: &[bool]) -> (Vec<u32>, Vec<usize>) {
    let mut label = vec![u32::MAX; member.len()];
    let mut sizes = Vec::new();
    let mut stack = Vec::new();
    for start in 0..member.len() {
        if !member[start] || label[start] != u32::MAX {
            continue;
        }
        let id = sizes.len() as u32;
        let mut size = 0;
        label[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            size += 1;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if let Some(j) = grid.neighbour(i, dx, dy)
                    && member[j]
                    && label[j] == u32::MAX
                {
                    label[j] = id;
                    stack.push(j);
                }
            }
        }
        sizes.push(size);
    }
    (label, sizes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_project_back_to_themselves() {
        for sheet in [
            Sheet::global(400, 200),
            Sheet::regional(300, 300, -90.0, -8.0, 40.0),
        ] {
            for (px, py) in [(10, 10), (150, 100), (299, 150), (5, 120)] {
                if px >= sheet.w || py >= sheet.h {
                    continue;
                }
                let Some(dir) = sheet.dir_at(px, py) else {
                    continue;
                };
                let (x, y) = sheet.project(dir).expect("on the sheet");
                assert!(
                    (x - px as f32).abs() < 1e-2 && (y - py as f32).abs() < 1e-2,
                    "({px}, {py}) → ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn components_count_connected_cells() {
        let grid = Sheet::regional(4, 3, 0.0, 0.0, 10.0).grid();
        #[rustfmt::skip]
        let member = [
            true,  true,  false, false,
            false, false, false, true,
            true,  false, false, true,
        ];
        let (label, sizes) = components(&grid, &member);
        let mut sorted = sizes.clone();
        sorted.sort();
        assert_eq!(sorted, vec![1, 2, 2]);
        assert_eq!(label[0], label[1]);
        assert_eq!(label[7], label[11]);
        assert_eq!(label[2], u32::MAX);
    }
}
