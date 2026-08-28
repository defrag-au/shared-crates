//! Small, pure spline/curve math.
//!
//! No I/O, no runtime dependencies, generic over dimension — so the same code
//! works for 2D screen/UV points, 3D positions, colours, etc., and can be
//! shared across renderers (and later extracted to shared-crates).

/// Centripetal-style uniform Catmull-Rom interpolation of a single segment.
///
/// Returns the point on the curve between `p1` and `p2` at `t` in `0..=1`,
/// using `p0` and `p3` as the surrounding tangent controls. At `t == 0` the
/// result is exactly `p1`; at `t == 1` it is exactly `p2`, so passing a grid of
/// control points reproduces them at the segment boundaries.
pub fn catmull_rom<const N: usize>(
    p0: [f32; N],
    p1: [f32; N],
    p2: [f32; N],
    p3: [f32; N],
    t: f32,
) -> [f32; N] {
    let t2 = t * t;
    let t3 = t2 * t;
    let mut out = [0.0; N];
    for k in 0..N {
        out[k] = 0.5
            * (2.0 * p1[k]
                + (-p0[k] + p2[k]) * t
                + (2.0 * p0[k] - 5.0 * p1[k] + 4.0 * p2[k] - p3[k]) * t2
                + (-p0[k] + 3.0 * p1[k] - 3.0 * p2[k] + p3[k]) * t3);
    }
    out
}

/// Evaluate a tensor-product Catmull-Rom surface over a `rows`×`cols` control
/// grid sampled via `get(row, col)`.
///
/// `(i, j)` is the cell whose lower corner the sample sits in and `(t, s)` are
/// the local parameters within that cell (`t` down rows, `s` across columns),
/// each in `0..=1`. Phantom control points beyond the grid edges are clamped to
/// the edge nodes, which limits overshoot at the borders. The four control nodes
/// of a cell are reproduced exactly at its corners.
pub fn surface_point<const N: usize, F: Fn(usize, usize) -> [f32; N]>(
    get: &F,
    rows: usize,
    cols: usize,
    i: usize,
    j: usize,
    t: f32,
    s: f32,
) -> [f32; N] {
    let mut row_pts = [[0.0f32; N]; 4];
    for (k, dr) in [-1i32, 0, 1, 2].into_iter().enumerate() {
        let r = clamp_idx(i as i32 + dr, rows);
        let p0 = get(r, clamp_idx(j as i32 - 1, cols));
        let p1 = get(r, clamp_idx(j as i32, cols));
        let p2 = get(r, clamp_idx(j as i32 + 1, cols));
        let p3 = get(r, clamp_idx(j as i32 + 2, cols));
        row_pts[k] = catmull_rom(p0, p1, p2, p3, s);
    }
    catmull_rom(row_pts[0], row_pts[1], row_pts[2], row_pts[3], t)
}

/// Clamp a (possibly negative) index into `0..n`.
pub fn clamp_idx(i: i32, n: usize) -> usize {
    i.clamp(0, n as i32 - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4
    }

    #[test]
    fn endpoints_are_exact() {
        let p0 = [0.0, 0.0];
        let p1 = [1.0, 2.0];
        let p2 = [3.0, 5.0];
        let p3 = [4.0, 4.0];
        assert!(approx(catmull_rom(p0, p1, p2, p3, 0.0), p1));
        assert!(approx(catmull_rom(p0, p1, p2, p3, 1.0), p2));
    }

    #[test]
    fn collinear_controls_stay_on_the_line() {
        // Evenly spaced collinear points: the curve is the straight line.
        let p = |x: f32| [x, 2.0 * x];
        let got = catmull_rom(p(0.0), p(1.0), p(2.0), p(3.0), 0.5);
        assert!(approx(got, p(1.5)));
    }

    #[test]
    fn surface_reproduces_control_nodes() {
        // 3x3 grid; sampling a cell corner returns that control node exactly.
        let grid = |r: usize, c: usize| [c as f32 * 10.0, r as f32 * 7.0];
        let got = surface_point(&grid, 3, 3, 1, 1, 0.0, 0.0);
        assert!(approx(got, grid(1, 1)));
        let got = surface_point(&grid, 3, 3, 1, 1, 1.0, 1.0);
        assert!(approx(got, grid(2, 2)));
    }

    #[test]
    fn works_in_three_dimensions() {
        let p0 = [0.0, 0.0, 0.0];
        let p1 = [1.0, 1.0, 1.0];
        let p2 = [2.0, 2.0, 2.0];
        let p3 = [3.0, 3.0, 3.0];
        let got = catmull_rom(p0, p1, p2, p3, 0.5);
        assert!((got[0] - 1.5).abs() < 1e-4);
        assert!((got[2] - 1.5).abs() < 1e-4);
    }
}
