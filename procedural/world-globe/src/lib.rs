//! A drawn world as a globe: the geometry and relief a renderer needs, built
//! from the equirectangular sheets `world-cartography` writes
//! (`perps_world_map --out … --height-out …`).
//!
//! - [`sphere`] — the planet's mesh, displaced by the height sheet, with
//!   texture coordinates into the sheets.
//! - [`normal_map`] — the relief's normals at the sheet's own resolution, for
//!   lighting slopes far finer than the mesh.
//!
//! ## The frame
//!
//! Y is up (north). A sheet pixel at `(x, y)` of a `w × h` sheet is the
//! direction [`sheet_dir`]`(x / w, y / h)`: longitude increases to the east
//! *as seen from outside*, so the sheet reads unmirrored on the globe, as it
//! does in the Blender globe. Texture coordinates are the sheet's own:
//! `(0, 0)` is its top-left (north-west) corner, as a texture uploads it.

use std::f32::consts::PI;

pub mod history;

/// An equirectangular sheet of heights, 0..1, row 0 at the north pole.
pub struct Field {
    pub w: usize,
    pub h: usize,
    pub values: Vec<f32>,
}

impl Field {
    /// The value at a sheet coordinate, wrapping east-west and clamping at
    /// the poles.
    pub fn at(&self, x: isize, y: isize) -> f32 {
        let x = x.rem_euclid(self.w as isize) as usize;
        let y = y.clamp(0, self.h as isize - 1) as usize;
        self.values[y * self.w + x]
    }

    /// Bilinear sample at sheet coordinates `u, v` in 0..1 (texel centres at
    /// `(i + 0.5) / w`).
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        let fx = u * self.w as f32 - 0.5;
        let fy = v * self.h as f32 - 0.5;
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as isize, y0 as isize);
        let top = self.at(x0, y0) * (1.0 - tx) + self.at(x0 + 1, y0) * tx;
        let bottom = self.at(x0, y0 + 1) * (1.0 - tx) + self.at(x0 + 1, y0 + 1) * tx;
        top * (1.0 - ty) + bottom * ty
    }
}

/// The unit direction for sheet coordinates `u, v` in 0..1: `u` runs west to
/// east from longitude −180°, `v` north to south.
pub fn sheet_dir(u: f32, v: f32) -> [f32; 3] {
    let lon = (u - 0.5) * 2.0 * PI;
    let lat = (0.5 - v) * PI;
    dir(lon, lat)
}

fn dir(lon: f32, lat: f32) -> [f32; 3] {
    // −Z for +longitude: east lies to the right of north seen from outside.
    [lat.cos() * lon.cos(), lat.sin(), -lat.cos() * lon.sin()]
}

/// Unit east and north at a longitude/latitude: the sheet's +x and −y.
fn tangents(lon: f32, lat: f32) -> ([f32; 3], [f32; 3]) {
    let east = [-lon.sin(), 0.0, -lon.cos()];
    let north = [-lat.sin() * lon.cos(), lat.cos(), lat.sin() * lon.sin()];
    (east, north)
}

/// A triangle mesh: positions, sheet texture coordinates, and indices.
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// A UV sphere of `segments` around and `segments / 2` pole to pole, each
/// vertex pushed out to radius `1 + height × relief`. The seam column is
/// doubled (u = 0 and u = 1) so texture coordinates do not wrap backwards
/// across it; triangles wind counter-clockwise seen from outside.
pub fn sphere(field: &Field, segments: usize, relief: f32) -> Mesh {
    let rings = (segments / 2).max(2);
    let segments = segments.max(3);
    let mut positions = Vec::with_capacity((rings + 1) * (segments + 1));
    let mut uvs = Vec::with_capacity(positions.capacity());
    for j in 0..=rings {
        let v = j as f32 / rings as f32;
        for i in 0..=segments {
            let u = i as f32 / segments as f32;
            let d = sheet_dir(u, v);
            let r = 1.0 + field.sample(u, v) * relief;
            positions.push([d[0] * r, d[1] * r, d[2] * r]);
            uvs.push([u, v]);
        }
    }
    let row = segments as u32 + 1;
    let mut indices = Vec::with_capacity(rings * segments * 6);
    for j in 0..rings as u32 {
        for i in 0..segments as u32 {
            let a = j * row + i; // north-west
            let b = a + 1; // north-east
            let c = a + row; // south-west
            let d = c + 1; // south-east
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    Mesh {
        positions,
        uvs,
        indices,
    }
}

/// The relief's normals, one per sheet texel, encoded `n × 0.5 + 0.5` into
/// RGBA bytes (alpha 255) in the globe's frame.
///
/// Heights are scaled by `relief × gain`: `relief` is the displacement the
/// mesh uses, and `gain` exaggerates slopes for lighting only, since true
/// slopes at globe scale are a few degrees. Distances on the ground shrink
/// toward the poles with the cosine of latitude, clamped so a polar texel
/// does not read as a cliff.
pub fn normal_map(field: &Field, relief: f32, gain: f32) -> Vec<[u8; 4]> {
    let (w, h) = (field.w, field.h);
    let scale = relief * gain;
    let step_north = PI / h as f32;
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        let v = (y as f32 + 0.5) / h as f32;
        let lat = (0.5 - v) * PI;
        let step_east = lat.cos().max(0.02) * 2.0 * PI / w as f32;
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let lon = (u - 0.5) * 2.0 * PI;
            let (xi, yi) = (x as isize, y as isize);
            let dx = (field.at(xi + 1, yi) - field.at(xi - 1, yi)) * 0.5 * scale / step_east;
            // Sheet y runs south; north is −y.
            let dy = (field.at(xi, yi - 1) - field.at(xi, yi + 1)) * 0.5 * scale / step_north;
            let up = dir(lon, lat);
            let (east, north) = tangents(lon, lat);
            let n = normalize([
                up[0] - dx * east[0] - dy * north[0],
                up[1] - dx * east[1] - dy * north[1],
                up[2] - dx * east[2] - dy * north[2],
            ]);
            out.push(encode(n));
        }
    }
    out
}

/// Decode a [`normal_map`] texel back to a unit vector.
pub fn decode(texel: [u8; 4]) -> [f32; 3] {
    let f = |b: u8| b as f32 / 255.0 * 2.0 - 1.0;
    normalize([f(texel[0]), f(texel[1]), f(texel[2])])
}

fn encode(n: [f32; 3]) -> [u8; 4] {
    let b = |c: f32| ((c * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8;
    [b(n[0]), b(n[1]), b(n[2]), 255]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12);
    [v[0] / len, v[1] / len, v[2] / len]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn flat(w: usize, h: usize, value: f32) -> Field {
        Field {
            w,
            h,
            values: vec![value; w * h],
        }
    }

    #[test]
    fn the_sheet_reads_unmirrored_from_outside() {
        // Seen from outside, with north up, east must be to the right:
        // right = up × outward... for a viewer looking inward, right is
        // north × outward.
        let centre = sheet_dir(0.5, 0.5);
        let east = sheet_dir(0.51, 0.5);
        let north = sheet_dir(0.5, 0.49);
        let toward_east = [
            east[0] - centre[0],
            east[1] - centre[1],
            east[2] - centre[2],
        ];
        let toward_north = [
            north[0] - centre[0],
            north[1] - centre[1],
            north[2] - centre[2],
        ];
        // A viewer outside looks along −centre; their right is
        // forward × up = (−centre) × north.
        let forward = [-centre[0], -centre[1], -centre[2]];
        let right = cross(forward, toward_north);
        assert!(dot(right, toward_east) > 0.0, "east is to the right");
    }

    #[test]
    fn tangents_match_the_sheet_directions() {
        for &(lon, lat) in &[(0.3f32, 0.2f32), (-2.0, -0.9), (2.9, 1.1)] {
            let (east, north) = tangents(lon, lat);
            let d = dir(lon, lat);
            let eps = 1e-3;
            let de = dir(lon + eps, lat);
            let dn = dir(lon, lat + eps);
            let fe = normalize([de[0] - d[0], de[1] - d[1], de[2] - d[2]]);
            let fn_ = normalize([dn[0] - d[0], dn[1] - d[1], dn[2] - d[2]]);
            assert!(dot(east, fe) > 0.999, "east at {lon},{lat}");
            assert!(dot(north, fn_) > 0.999, "north at {lon},{lat}");
        }
    }

    #[test]
    fn a_sphere_has_its_grid_and_radius() {
        let mesh = sphere(&flat(16, 8, 0.5), 8, 0.1);
        assert_eq!(mesh.positions.len(), 5 * 9);
        assert_eq!(mesh.indices.len(), 4 * 8 * 6);
        for p in &mesh.positions {
            let r = dot(*p, *p).sqrt();
            assert!((r - 1.05).abs() < 1e-5, "radius {r}");
        }
        assert!(
            mesh.indices
                .iter()
                .all(|&i| (i as usize) < mesh.positions.len())
        );
    }

    #[test]
    fn triangles_face_outward() {
        let mesh = sphere(&flat(16, 8, 0.0), 16, 0.0);
        let mut checked = 0;
        for tri in mesh.indices.chunks(3) {
            let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| mesh.positions[i as usize]);
            let n = cross(
                [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
                [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
            );
            if dot(n, n) < 1e-12 {
                continue; // degenerate at a pole
            }
            let centre = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            assert!(dot(n, centre) > 0.0, "a triangle faces inward");
            checked += 1;
        }
        assert!(checked > 100);
    }

    #[test]
    fn flat_ground_points_straight_up() {
        let field = flat(64, 32, 0.4);
        let normals = normal_map(&field, 0.05, 10.0);
        for (i, &texel) in normals.iter().enumerate() {
            let (x, y) = (i % 64, i / 64);
            let up = sheet_dir((x as f32 + 0.5) / 64.0, (y as f32 + 0.5) / 32.0);
            assert!(dot(decode(texel), up) > 0.995, "texel {x},{y}");
        }
    }

    #[test]
    fn ground_rising_to_the_east_faces_west() {
        // Height grows with x: the slope faces west, away from east.
        let (w, h) = (128, 64);
        let values = (0..w * h).map(|i| (i % w) as f32 / w as f32).collect();
        let field = Field { w, h, values };
        let normals = normal_map(&field, 0.05, 10.0);
        let (x, y) = (40, 32);
        let n = decode(normals[y * w + x]);
        let lon = ((x as f32 + 0.5) / w as f32 - 0.5) * 2.0 * PI;
        let lat = (0.5 - (y as f32 + 0.5) / h as f32) * PI;
        let (east, north) = tangents(lon, lat);
        // Rise per unit of ground: one sheet step of height per texel, over
        // a texel's width on the ground at this latitude.
        let rise = (1.0 / w as f32) * 0.05 * 10.0 / (lat.cos() * 2.0 * PI / w as f32);
        let expected = -rise / (1.0 + rise * rise).sqrt();
        assert!(
            (dot(n, east) - expected).abs() < 0.01,
            "tilted west by {expected}: {n:?}"
        );
        assert!(dot(n, north).abs() < 0.01, "not north or south: {n:?}");
        // A bigger gain tilts it further.
        let steeper = decode(normal_map(&field, 0.05, 30.0)[y * w + x]);
        assert!(dot(steeper, east) < dot(n, east));
    }
}
