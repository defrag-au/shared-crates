//! Points on the unit sphere: an even spread of them, and a spatial index for
//! finding the ones near a point.

use glam::Vec3;

/// `n` points spread evenly over the unit sphere (a Fibonacci spiral): every
/// point has about the same area, `4π / n`, around it.
pub fn fibonacci(n: usize) -> Vec<Vec3> {
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    (0..n)
        .map(|i| {
            let y = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
            let r = (1.0 - y * y).max(0.0).sqrt();
            let theta = golden * i as f32;
            Vec3::new(r * theta.cos(), y, r * theta.sin())
        })
        .collect()
}

/// The typical distance (chord ≈ angle) between neighbours of `n` evenly
/// spread points.
pub fn spacing(n: usize) -> f32 {
    (4.0 * std::f32::consts::PI / n as f32).sqrt()
}

/// A uniform 3D grid over the cube around the unit sphere, bucketing point
/// indices for radius queries.
pub struct Index {
    cell: f32,
    dim: usize,
    /// `starts[c]..starts[c + 1]` indexes `items` for cell `c`.
    starts: Vec<u32>,
    items: Vec<u32>,
}

impl Index {
    /// Index `points` (on or near the unit sphere) with cells `cell` wide.
    pub fn new(points: &[Vec3], cell: f32) -> Self {
        let dim = (2.2 / cell).ceil() as usize + 1;
        let index_of = |p: Vec3| cell_of(p, cell, dim);
        let mut counts = vec![0u32; dim * dim * dim + 1];
        for &p in points {
            counts[index_of(p) + 1] += 1;
        }
        for c in 1..counts.len() {
            counts[c] += counts[c - 1];
        }
        let starts = counts.clone();
        let mut next = counts;
        let mut items = vec![0u32; points.len()];
        for (i, &p) in points.iter().enumerate() {
            let c = index_of(p);
            items[next[c] as usize] = i as u32;
            next[c] += 1;
        }
        Self {
            cell,
            dim,
            starts,
            items,
        }
    }

    /// Call `f` with the index of every point that might be within `radius`
    /// of `p` (every one that is, and some that are not: the caller measures).
    pub fn near(&self, p: Vec3, radius: f32, mut f: impl FnMut(usize)) {
        let lo = |v: f32| (((v - radius + 1.1) / self.cell).floor().max(0.0)) as usize;
        let hi = |v: f32| (((v + radius + 1.1) / self.cell).floor() as usize).min(self.dim - 1);
        for z in lo(p.z)..=hi(p.z) {
            for y in lo(p.y)..=hi(p.y) {
                for x in lo(p.x)..=hi(p.x) {
                    let c = (z * self.dim + y) * self.dim + x;
                    for &i in &self.items[self.starts[c] as usize..self.starts[c + 1] as usize] {
                        f(i as usize);
                    }
                }
            }
        }
    }

    /// The nearest point to `p` within `radius`, if any.
    pub fn nearest(&self, points: &[Vec3], p: Vec3, radius: f32) -> Option<usize> {
        let mut best = None;
        let mut best_d = radius * radius;
        self.near(p, radius, |i| {
            let d = points[i].distance_squared(p);
            if d <= best_d {
                best_d = d;
                best = Some(i);
            }
        });
        best
    }
}

fn cell_of(p: Vec3, cell: f32, dim: usize) -> usize {
    let c = |v: f32| (((v + 1.1) / cell).floor().max(0.0) as usize).min(dim - 1);
    (c(p.z) * dim + c(p.y)) * dim + c(p.x)
}

/// Each point's `k` nearest others (fewer if the index finds fewer within
/// `radius`), nearest first.
pub fn neighbours(points: &[Vec3], index: &Index, k: usize, radius: f32) -> Vec<Vec<u32>> {
    points
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let mut near: Vec<(f32, u32)> = Vec::new();
            index.near(p, radius, |j| {
                if j != i {
                    let d = points[j].distance_squared(p);
                    if d <= radius * radius {
                        near.push((d, j as u32));
                    }
                }
            });
            near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            near.into_iter().take(k).map(|(_, j)| j).collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fibonacci_points_are_on_the_sphere_and_evenly_spread() {
        let points = fibonacci(4000);
        assert!(points.iter().all(|p| (p.length() - 1.0).abs() < 1e-5));
        // Every point's nearest neighbour is about one spacing away.
        let index = Index::new(&points, spacing(4000) * 2.0);
        let s = spacing(4000);
        for (i, &p) in points.iter().enumerate() {
            let mut best = f32::MAX;
            index.near(p, s * 2.0, |j| {
                if j != i {
                    best = best.min(points[j].distance(p));
                }
            });
            assert!(
                (0.5 * s..1.5 * s).contains(&best),
                "point {i}: nearest {best} vs {s}"
            );
        }
    }

    #[test]
    fn the_index_finds_exactly_the_brute_force_nearest() {
        let points = fibonacci(3000);
        let index = Index::new(&points, spacing(3000) * 2.0);
        for k in 0..200 {
            let a = k as f32 * 0.37;
            let q = Vec3::new(a.cos(), (a * 0.7).sin(), a.sin()).normalize();
            let brute = (0..points.len())
                .min_by(|&i, &j| points[i].distance(q).total_cmp(&points[j].distance(q)))
                .unwrap();
            assert_eq!(index.nearest(&points, q, spacing(3000) * 2.0), Some(brute));
        }
    }
}
