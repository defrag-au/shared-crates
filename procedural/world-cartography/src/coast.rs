//! The coast as a field: signed distance to the shore, per pixel.
//!
//! Every coast mark — the stroke, the surf band, the land's inner shade,
//! later the waterlines — is drawn as a function of one distance, so the
//! marks taper, fade and merge between neighbouring islands instead of
//! being a one-pixel neighbour test at constant weight (see
//! `reports/Painterly terrain map techniques.md`).
//!
//! Units are equator pixels: a pixel's nominal step, so one unit is the
//! same ground everywhere. Negative on land, positive at sea.
//!
//! Far from the shore the distance is exact Euclidean, by the separable
//! lower-envelope transform of Felzenszwalb & Huttenlocher ("Distance
//! Transforms of Sampled Functions", 2012): rows first with each row's own
//! east-west pixel width (narrower toward the poles), wrapping in longitude
//! on a global sheet; then columns. Within a pixel or two of the shore it
//! comes instead from the continuous land score over its gradient, which
//! places the shore between pixel centres, so strokes drawn from it are
//! anti-aliased rather than stepped.

use rayon::prelude::*;

use glam::Vec3;

use crate::colour::{Colour, rgb, smoothstep};
use crate::noise::{gradient_noise, grain};
use crate::sheet::{Cell, Grid, box_blur};

/// How the coast's marks are drawn. Widths are equator pixels. The values
/// are the research report's starting points tuned by eye on the 4000-wide
/// chart, except the colours, derived as their fields say.
#[derive(Clone, Debug, PartialEq)]
pub struct CoastStyle {
    /// The stroke's colour: the deepest sea band (46, 72, 142) darkened to
    /// about 55%, so the shore is drawn in the water's own hue rather than
    /// the land's black-brown ink (Esri's coastal-halo advice; Datawrapper).
    pub stroke: Colour,
    /// Surf along the shore: the shallowest sea band (132, 214, 214) taken
    /// most of the way to white.
    pub surf: Colour,
    /// Stroke half-width on a shore facing the light and on one facing away
    /// from it: hand-drawn coasts are heavier on the shadow side.
    pub stroke_width: Taper,
    /// Width of the surf band beyond the stroke, and its strongest mix.
    pub surf_width: f32,
    pub surf_strength: f32,
    /// Width of the land's inner shade, its strongest mix, and how far it
    /// darkens the land's colour toward black.
    pub shade_width: f32,
    pub shade_strength: f32,
    pub shade_darken: f32,
}

/// A value that runs from `first` to `last` along something: lit to shadow
/// side, first waterline to last.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Taper {
    pub first: f32,
    pub last: f32,
}

impl Taper {
    /// The value `t` (0..1) of the way along.
    pub fn at(self, t: f32) -> f32 {
        self.first + (self.last - self.first) * t
    }
}

impl Default for CoastStyle {
    fn default() -> Self {
        Self {
            stroke: rgb(25, 40, 78),
            surf: rgb(236, 250, 246),
            stroke_width: Taper {
                first: 0.3,
                last: 1.1,
            },
            surf_width: 4.0,
            surf_strength: 0.75,
            shade_width: 3.5,
            shade_strength: 0.6,
            shade_darken: 0.35,
        }
    }
}

/// How much a coast mark covers a pixel, each 0..1.
pub struct Marks {
    /// The stroke along the shore, on its water side.
    pub stroke: f32,
    /// Surf on the water side.
    pub surf: f32,
    /// The land's inner shade.
    pub shade: f32,
}

/// The coast marks at signed distance `sd` (equator pixels), on a shore
/// `shadow` (0 lit … 1 facing away from the light) — see [`shadow_side`].
pub fn marks(sd: f32, shadow: f32, style: &CoastStyle) -> Marks {
    let half = style.stroke_width.at(shadow);
    // The stroke lies on the water side, from the shore out to twice its
    // half-width (box-filtered coverage of a pixel by that band). Centred on
    // the shore it buried land thinner than itself: a narrow spit drew as a
    // solid blot.
    let stroke = (half + 0.5 - (sd - half).abs()).clamp(0.0, 1.0);
    let surf = if sd > 0.0 {
        (1.0 - smoothstep(2.0 * half, 2.0 * half + style.surf_width, sd)) * style.surf_strength
    } else {
        0.0
    };
    let shade = if sd < 0.0 {
        (1.0 - smoothstep(0.0, style.shade_width, -sd)) * style.shade_strength
    } else {
        0.0
    };
    Marks {
        stroke,
        surf,
        shade,
    }
}

/// How waterlines are drawn. Distances and widths are equator pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct WaterlineStyle {
    /// Distance of the first line from the shore, and the growth of each
    /// gap over the last: `d_n = growth · d_{n−1} + first`. 1.3 is John
    /// Nelson's growth in his Water Line Calculator (10, 23, 39.9…); the
    /// first sits just beyond the surf.
    pub first: f32,
    pub growth: f32,
    /// Opacity of the first line, and its fall per line (the research
    /// report's starting range: 0.6–0.75 per line).
    pub opacity: f32,
    pub fade: f32,
    /// Line half-width, first line to last: lines thin outward.
    pub width: Taper,
    /// Share of each line drawn, first to last, one per line: continuous
    /// near the shore, broken far out (Roberts' "Ripples" style).
    pub duty: Vec<f32>,
    /// Blur of the distance a line follows, as a share of its distance:
    /// outer lines see only the large shapes, smoothing inner corners and
    /// bridging inlet mouths instead of following them in.
    pub blur: f32,
}

impl Default for WaterlineStyle {
    fn default() -> Self {
        Self {
            first: 6.0,
            growth: 1.3,
            opacity: 0.5,
            fade: 0.7,
            width: Taper {
                first: 0.55,
                last: 0.35,
            },
            duty: vec![1.0, 0.85, 0.6, 0.4],
            blur: 0.4,
        }
    }
}

/// Waterlines: lines echoing the shore out to sea, each at a set distance
/// read from its own blurred copy of the coast's distance field.
pub struct Waterlines {
    style: WaterlineStyle,
    /// Per line: its distance from the shore and the field it follows.
    lines: Vec<(f32, Vec<f32>)>,
}

impl Waterlines {
    pub fn new(coast: &[f32], grid: &Grid, style: &WaterlineStyle) -> Self {
        let mut d = style.first;
        let lines = style
            .duty
            .iter()
            .map(|_| {
                let at = d;
                d = style.growth * d + style.first;
                let radius = (at * style.blur).round().max(1.0) as i32;
                // Far from any shore the field is infinite; cap it so the
                // blur averages finite numbers.
                let capped: Vec<f32> = coast
                    .iter()
                    .map(|&v| v.clamp(-at * 4.0, at * 4.0))
                    .collect();
                (at, box_blur(&capped, grid, radius))
            })
            .collect();
        Self {
            style: style.clone(),
            lines,
        }
    }

    /// How strongly waterlines ink cell `i` (direction `dir`), 0..1.
    pub fn ink(&self, i: usize, dir: Vec3) -> f32 {
        let style = &self.style;
        let last = (self.lines.len().max(2) - 1) as f32;
        let mut ink = 0.0f32;
        for (n, (at, field)) in self.lines.iter().enumerate() {
            let half = style.width.at(n as f32 / last);
            // A small wander, so the spacing is not a mechanical offset.
            let warp = gradient_noise(dir * 31.0 + Vec3::splat(n as f32 * 3.7)) * 0.6;
            let cover = (half + 0.5 - (field[i] + warp - at).abs()).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            // Breaks: value noise against the line's duty, soft-edged.
            let duty = style.duty[n];
            let g = grain(dir, 70.0 + n as f32 * 9.0);
            let dash = 1.0 - smoothstep(duty - 0.06, duty + 0.06, g);
            ink = ink.max(cover * dash * style.opacity * style.fade.powi(n as i32));
        }
        ink
    }
}

/// How squarely the shore at cell `i` faces away from the upper-left light,
/// 0..1: the seaward normal (the distance field's gradient) against the
/// light's travel, down and to the right on the sheet. `lat_scale` is the
/// cell's east-west pixel width ([`Grid::lat_scale`]).
pub fn shadow_side(coast: &[f32], i: usize, grid: &Grid, lat_scale: f32) -> f32 {
    let at = |dx: i32, dy: i32| coast[grid.clamped(i, dx, dy)];
    let gx = (at(1, 0) - at(-1, 0)) / (2.0 * lat_scale);
    let gy = (at(0, 1) - at(0, -1)) / 2.0;
    let len = (gx * gx + gy * gy).sqrt();
    if len < 1e-6 {
        return 0.0;
    }
    ((gx + gy) / (len * std::f32::consts::SQRT_2)).clamp(0.0, 1.0)
}

/// Distance from pixel centres beyond which the land score's linear estimate
/// is not trusted, equator pixels.
const LINEAR_REACH: f32 = 1.5;

/// Signed distance to the shore per cell, equator pixels: negative on land,
/// positive at sea. `is_land` says which cells are land; `cells` give the
/// continuous land score (zero on the shore) and each row's latitude.
pub fn signed_distance(is_land: &[bool], cells: &[Cell], grid: &Grid) -> Vec<f32> {
    let row_scale: Vec<f64> = (0..grid.h)
        .map(|y| grid.lat_scale(cells[y * grid.w].dir) as f64)
        .collect();
    let to_sea = distance_to(&|i| !is_land[i], grid, &row_scale);
    let to_land = distance_to(&|i| is_land[i], grid, &row_scale);
    (0..cells.len())
        .into_par_iter()
        .map(|i| {
            // Pixel-centre distance to the nearest centre of the other kind;
            // the shore lies about half a pixel short of it.
            let coarse = if is_land[i] {
                -(to_sea[i] - 0.5)
            } else {
                to_land[i] - 0.5
            };
            if coarse.abs() > LINEAR_REACH + 1.0 {
                return coarse;
            }
            // The land score is linear enough near the shore that its value
            // over its gradient is the distance to its zero.
            let x = |dx: i32| cells[grid.clamped(i, dx, 0)].land;
            let y = |dy: i32| cells[grid.clamped(i, 0, dy)].land;
            let sx = row_scale[i / grid.w] as f32;
            let gx = (x(1) - x(-1)) / (2.0 * sx);
            let gy = (y(1) - y(-1)) / 2.0;
            let g = (gx * gx + gy * gy).sqrt();
            if g < 1e-9 {
                return coarse;
            }
            let linear = -cells[i].land / g;
            // Keep the side the mask says: a crater lake or a clamped coast
            // can disagree with the score's sign.
            let linear = if is_land[i] {
                linear.min(-1e-3)
            } else {
                linear.max(1e-3)
            };
            if linear.abs() < LINEAR_REACH {
                linear
            } else {
                coarse
            }
        })
        .collect()
}

/// Euclidean distance from each cell to the nearest cell where `site` holds,
/// equator pixels (`f32::INFINITY` if there is none).
fn distance_to(site: &(dyn Fn(usize) -> bool + Sync), grid: &Grid, row_scale: &[f64]) -> Vec<f32> {
    let (w, h) = (grid.w, grid.h);
    const FAR: f64 = 1e20;
    // Rows: squared distance along the row, with the row's pixel width.
    let rows: Vec<Vec<f64>> = (0..h)
        .into_par_iter()
        .map(|y| {
            let s = row_scale[y];
            if grid.global {
                // Three copies of the row, so the envelope sees across the
                // date line; the middle copy is the answer.
                let f: Vec<f64> = (0..3 * w)
                    .map(|k| if site(y * w + k % w) { 0.0 } else { FAR })
                    .collect();
                let pos: Vec<f64> = (0..3 * w).map(|k| k as f64 * s).collect();
                envelope(&f, &pos)[w..2 * w].to_vec()
            } else {
                let f: Vec<f64> = (0..w)
                    .map(|x| if site(y * w + x) { 0.0 } else { FAR })
                    .collect();
                let pos: Vec<f64> = (0..w).map(|x| x as f64 * s).collect();
                envelope(&f, &pos)
            }
        })
        .collect();
    // Columns: one unit per row.
    let pos: Vec<f64> = (0..h).map(|y| y as f64).collect();
    let columns: Vec<Vec<f64>> = (0..w)
        .into_par_iter()
        .map(|x| {
            let f: Vec<f64> = (0..h).map(|y| rows[y][x]).collect();
            envelope(&f, &pos)
        })
        .collect();
    let mut out = vec![0.0f32; w * h];
    for (x, column) in columns.iter().enumerate() {
        for (y, &d2) in column.iter().enumerate() {
            out[y * w + x] = if d2 >= FAR * 0.5 {
                f32::INFINITY
            } else {
                d2.sqrt() as f32
            };
        }
    }
    out
}

/// The 1-D squared distance transform: for each sample `q`,
/// `min_p (pos[q] − pos[p])² + f[p]`, by the lower envelope of the parabolas
/// rooted at each `p` (Felzenszwalb & Huttenlocher's Algorithm 1, with
/// positions in place of indices so a row's pixels can have a width).
fn envelope(f: &[f64], pos: &[f64]) -> Vec<f64> {
    let n = f.len();
    let mut v = vec![0usize; n]; // parabola roots in the envelope
    let mut z = vec![0.0f64; n + 1]; // boundaries between them
    let mut k = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    // Where the parabolas rooted at p and q meet.
    let meet = |q: usize, p: usize| {
        ((f[q] + pos[q] * pos[q]) - (f[p] + pos[p] * pos[p])) / (2.0 * (pos[q] - pos[p]))
    };
    for q in 1..n {
        let mut s = meet(q, v[k]);
        while s <= z[k] {
            k -= 1;
            s = meet(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    let mut out = vec![0.0; n];
    k = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while z[k + 1] < pos[q] {
            k += 1;
        }
        let d = pos[q] - pos[v[k]];
        *o = d * d + f[v[k]];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The brute-force answer, for comparison.
    fn brute(f: &[f64], pos: &[f64]) -> Vec<f64> {
        (0..f.len())
            .map(|q| {
                (0..f.len())
                    .map(|p| (pos[q] - pos[p]).powi(2) + f[p])
                    .fold(f64::INFINITY, f64::min)
            })
            .collect()
    }

    #[test]
    fn the_envelope_is_the_brute_force_minimum() {
        let far = 1e20;
        let f = vec![far, 0.0, far, far, far, 0.0, far, far, 2.0, far];
        for s in [1.0, 0.37] {
            let pos: Vec<f64> = (0..f.len()).map(|x| x as f64 * s).collect();
            let got = envelope(&f, &pos);
            let want = brute(&f, &pos);
            for (g, w) in got.iter().zip(&want) {
                assert!((g - w).abs() < 1e-9, "scale {s}: {got:?} vs {want:?}");
            }
        }
    }

    #[test]
    fn distance_wraps_across_the_date_line_and_narrows_with_latitude() {
        use crate::sheet::Sheet;
        let sheet = Sheet::global(80, 40);
        let grid = sheet.grid();
        // One land pixel at the west edge of the equator row and one at the
        // west edge of a row at 60° north.
        let mut land = vec![false; grid.w * grid.h];
        let eq = grid.h / 2;
        land[eq * grid.w] = true;
        let row_scale: Vec<f64> = (0..grid.h)
            .map(|y| {
                let dir = sheet.dir_at(0, y).expect("on the sheet");
                grid.lat_scale(dir) as f64
            })
            .collect();
        let d = distance_to(&|i| land[i], &grid, &row_scale);
        // The pixel at the east edge is one pixel away, across the wrap.
        assert!((d[eq * grid.w + grid.w - 1] - row_scale[eq] as f32).abs() < 1e-4);
        // Along a high row, a pixel is narrower: the same pixel count is less
        // ground.
        let hi = 6;
        let mut land_hi = vec![false; grid.w * grid.h];
        land_hi[hi * grid.w] = true;
        let d_hi = distance_to(&|i| land_hi[i], &grid, &row_scale);
        assert!(d_hi[hi * grid.w + 5] < d[eq * grid.w + 5]);
        assert!((d_hi[hi * grid.w + 5] - 5.0 * row_scale[hi] as f32).abs() < 1e-3);
    }
}
