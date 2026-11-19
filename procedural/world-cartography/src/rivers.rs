//! Rivers as an anti-aliased coverage mask (0..1 per pixel).
//!
//! [`strokes`] draws rivers traced on the bake's detail layer
//! (`world_baker::detail::Drainage::rivers`, into `world_bake::River`) as
//! smoothed, tapered strokes.
//! [`raster`] routes water over the sheet's own pixels instead, for sheets
//! drawn without a detail layer.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use glam::Vec3;
use world_bake::River;

use crate::sheet::{Cell, Grid, Sheet};

/// Rain-weighted catchment (detail cells) at which a channel is drawn on the
/// global sheet: chosen to match the density of the earlier per-pixel river
/// pass (260 px of catchment at the 2000-wide reference ≈ 850 detail cells).
pub const MIN_FLOW: f32 = 850.0;

/// How river strokes are sized.
pub struct Stroke {
    pub min_flow: f32,
    /// Width multiplier for the sheet's scale on the ground.
    pub px_scale: f32,
}

/// Chaikin corner-cutting: rounds a polyline's 45° steps into curves. Ends
/// stay put so mouths and junctions still meet. Each point carries a flow.
pub fn chaikin(points: &[(f32, f32, f32)], rounds: usize) -> Vec<(f32, f32, f32)> {
    let mut line = points.to_vec();
    for _ in 0..rounds {
        if line.len() < 3 {
            break;
        }
        let mut next = Vec::with_capacity(line.len() * 2);
        next.push(line[0]);
        for pair in line.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let at = |t: f32| {
                (
                    a.0 + (b.0 - a.0) * t,
                    a.1 + (b.1 - a.1) * t,
                    a.2 + (b.2 - a.2) * t,
                )
            };
            next.push(at(0.25));
            next.push(at(0.75));
        }
        next.push(*line.last().unwrap());
        line = next;
    }
    line
}

/// Rasterise traced rivers into an anti-aliased coverage mask: each is
/// projected, broken where it leaves the sheet or crosses the seam, smoothed,
/// and stroked as tapered capsules whose width grows with the log of flow.
/// Rivers stop where `frozen` says the ground is ice, so ice reads as a sheet.
pub fn strokes(
    rivers: &[River],
    sheet: &Sheet,
    frozen: &dyn Fn([f32; 3]) -> bool,
    stroke: &Stroke,
) -> Vec<f32> {
    let (w, h) = (sheet.w, sheet.h);
    let wrap = sheet.region.is_none();
    let mut coverage = vec![0.0f32; w * h];
    let radius = |flow: f32| {
        let q = (flow / stroke.min_flow).max(1.0);
        (0.45 + 0.38 * q.log2()).min(2.6) * stroke.px_scale
    };
    let mut paint = |line: &[(f32, f32, f32)]| {
        for pair in chaikin(line, 2).windows(2) {
            let ((x0, y0, f0), (x1, y1, f1)) = (pair[0], pair[1]);
            let (r0, r1) = (radius(f0), radius(f1));
            let reach = r0.max(r1) + 1.0;
            let (xa, xb) = (x0.min(x1) - reach, x0.max(x1) + reach);
            let (ya, yb) = (y0.min(y1) - reach, y0.max(y1) + reach);
            let (dx, dy) = (x1 - x0, y1 - y0);
            let len2 = (dx * dx + dy * dy).max(1e-6);
            for py in (ya.floor().max(0.0) as usize)..=(yb.ceil().min(h as f32 - 1.0) as usize) {
                for px in (xa.floor().max(0.0) as usize)..=(xb.ceil().min(w as f32 - 1.0) as usize)
                {
                    let (qx, qy) = (px as f32 - x0, py as f32 - y0);
                    let t = ((qx * dx + qy * dy) / len2).clamp(0.0, 1.0);
                    let (ex, ey) = (qx - dx * t, qy - dy * t);
                    let r = r0 + (r1 - r0) * t;
                    let cover = (r - (ex * ex + ey * ey).sqrt() + 0.5).clamp(0.0, 1.0);
                    let cell = &mut coverage[py * w + px];
                    *cell = cell.max(cover);
                }
            }
        }
    };
    for river in rivers {
        let mut line: Vec<(f32, f32, f32)> = Vec::new();
        for (&d, &flow) in river.points.iter().zip(&river.flow) {
            // Rivers stop at the ice edge: across a flat sheet they read as
            // borders, not water.
            let point = if frozen(d) {
                None
            } else {
                sheet.project(Vec3::from_array(d))
            };
            match point {
                Some((x, y)) => {
                    let seam = wrap
                        && line
                            .last()
                            .is_some_and(|&(lx, _, _)| (x - lx).abs() > w as f32 * 0.5);
                    if seam {
                        paint(&line);
                        line.clear();
                    }
                    line.push((x, y, flow));
                }
                None => {
                    paint(&line);
                    line.clear();
                }
            }
        }
        paint(&line);
    }
    coverage
}

/// Rivers routed on the pixel grid, for sheets drawn without a detail layer.
/// `zoom` is the sheet's scale relative to a 2000-wide global sheet.
pub fn raster(cells: &[Cell], grid: &Grid, zoom: f32, threshold: f32) -> Vec<f32> {
    let accumulation = flow_accumulation(cells, grid);
    // Thresholds are in pixels of catchment, so scale them by how much of
    // the planet one pixel covers: the same rivers at any width or zoom.
    let river_start = 260.0 * zoom.powi(2) * threshold;
    let mut river = vec![0.0_f32; cells.len()];
    for (i, &acc) in accumulation.iter().enumerate() {
        if acc < river_start {
            continue;
        }
        let strength = ((acc / river_start).log2() * 0.45 + 0.55).min(2.6) * zoom.powf(0.6);
        let reach = strength.ceil() as i32;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let d = ((dx * dx + dy * dy) as f32).sqrt();
                let cover = (strength * 0.5 + 0.5 - d).clamp(0.0, 1.0);
                if cover > 0.0
                    && let Some(j) = grid.neighbour(i, dx, dy)
                {
                    let alpha = cover * (0.55 + 0.45 * (strength / 2.6).min(1.0));
                    river[j] = river[j].max(alpha);
                }
            }
        }
    }
    river
}

/// A height with a total order, for the priority-flood heap. Equality and
/// both orderings come from `total_cmp`, so they agree.
#[derive(Clone, Copy)]
struct Height(f32);
impl PartialEq for Height {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for Height {}
impl PartialOrd for Height {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Height {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// Rain routed downhill to the sea. Depressions are filled (priority-flood),
/// so every land cell drains; each cell's receiver is the cell the flood
/// reached it from. Returns accumulated catchment per cell, in units of one
/// equatorial pixel of average rain.
fn flow_accumulation(cells: &[Cell], grid: &Grid) -> Vec<f32> {
    let n = cells.len();
    let is_land = |i: usize| cells[i].valid && cells[i].land > 0.0;
    let mut filled: Vec<f32> = cells.iter().map(|c| c.elevation).collect();
    let mut receiver = vec![u32::MAX; n];
    let mut done = vec![false; n];
    let mut heap = BinaryHeap::new();
    const NEIGHBOURS: [(i32, i32); 8] = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ];

    for i in 0..n {
        if !is_land(i) {
            done[i] = true;
            continue;
        }
        let outlet = NEIGHBOURS
            .iter()
            .filter_map(|&(dx, dy)| grid.neighbour(i, dx, dy))
            .find(|&j| !is_land(j));
        if let Some(j) = outlet {
            receiver[i] = j as u32;
            done[i] = true;
            heap.push(Reverse((Height(filled[i]), i as u32)));
        }
    }

    let mut order = Vec::with_capacity(n);
    while let Some(Reverse((Height(level), i))) = heap.pop() {
        let i = i as usize;
        order.push(i);
        for &(dx, dy) in &NEIGHBOURS {
            if let Some(j) = grid.neighbour(i, dx, dy)
                && !done[j]
            {
                done[j] = true;
                receiver[j] = i as u32;
                filled[j] = filled[j].max(level + 1e-7);
                heap.push(Reverse((Height(filled[j]), j as u32)));
            }
        }
    }

    let mut accumulation: Vec<f32> = cells
        .iter()
        .map(|c| {
            if !c.valid {
                return 0.0;
            }
            let area = if grid.global {
                c.dir.y.asin().cos()
            } else {
                1.0
            };
            area * (0.25 + c.moisture.clamp(0.0, 1.0) * 1.5)
        })
        .collect();
    for &i in order.iter().rev() {
        let r = receiver[i] as usize;
        if r < n && is_land(r) {
            accumulation[r] += accumulation[i];
        }
    }
    accumulation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaikin_keeps_ends_and_rounds_corners() {
        let line = [(0.0, 0.0, 1.0), (10.0, 0.0, 2.0), (10.0, 10.0, 3.0)];
        let round = chaikin(&line, 2);
        assert_eq!(round.first(), line.first());
        assert_eq!(round.last(), line.last());
        assert!(round.len() > line.len());
        // The corner itself is cut away.
        assert!(!round.iter().any(|&(x, y, _)| x == 10.0 && y == 0.0));
    }

    #[test]
    fn a_stroke_widens_with_flow() {
        let sheet = Sheet::regional(200, 200, 0.0, 0.0, 20.0);
        let along = |lat: f32| crate::sheet::lon_lat_to_dir(0.0, lat.to_radians()).to_array();
        let river = River {
            points: (0..40).map(|k| along(-8.0 + k as f32 * 0.4)).collect(),
            flow: (0..40).map(|k| 850.0 * 2f32.powi(k / 5)).collect(),
        };
        let cover = strokes(
            &[river],
            &sheet,
            &|_| false,
            &Stroke {
                min_flow: 850.0,
                px_scale: 1.0,
            },
        );
        let width_at = |row: usize| cover[row * 200..(row + 1) * 200].iter().sum::<f32>();
        // Flow grows northwards, and north is up the sheet (smaller rows).
        let (upstream, downstream) = (width_at(170), width_at(40));
        assert!(upstream > 0.0, "the stroke reaches the upstream row");
        assert!(
            downstream > upstream * 1.5,
            "widths {upstream} → {downstream}"
        );
    }
}
