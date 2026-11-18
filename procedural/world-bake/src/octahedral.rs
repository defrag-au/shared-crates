//! Octahedral mapping between unit directions and texture coordinates.
//!
//! The same convention as the hodlcroft compositor's `planet_gen::cubemap`
//! (its `oct_uv_to_dir` is private): `u` runs along x, `v` along y, the upper
//! hemisphere (z ≥ 0) is the inner diamond and the lower hemisphere folds into
//! the corners. The compositor's baker carries a test that bakes a known
//! field with planet-gen and reads it back through here, so a drift between
//! the two fails a test rather than mirroring a world.

/// A unit direction for octahedral coordinates in `[0, 1]²`.
pub fn uv_to_dir(u: f32, v: f32) -> [f32; 3] {
    let fx = u * 2.0 - 1.0;
    let fy = v * 2.0 - 1.0;
    let mut nx = fx;
    let mut ny = fy;
    let nz = 1.0 - fx.abs() - fy.abs();
    if nz < 0.0 {
        nx = (1.0 - fy.abs()) * fx.signum();
        ny = (1.0 - fx.abs()) * fy.signum();
    }
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    [nx / len, ny / len, nz / len]
}

/// Octahedral coordinates in `[0, 1]²` for a direction (need not be unit).
pub fn dir_to_uv(dir: [f32; 3]) -> (f32, f32) {
    let [x, y, z] = dir;
    let l1 = x.abs() + y.abs() + z.abs();
    let (mut px, mut py) = (x / l1, y / l1);
    if z < 0.0 {
        let (ox, oy) = (px, py);
        px = (1.0 - oy.abs()) * sign(ox);
        py = (1.0 - ox.abs()) * sign(oy);
    }
    (px * 0.5 + 0.5, py * 0.5 + 0.5)
}

/// `signum` that maps zero to +1, as the fold needs a side for the axes.
fn sign(v: f32) -> f32 {
    if v >= 0.0 { 1.0 } else { -1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_round_trip_through_uv() {
        let steps = 41;
        for i in 0..steps {
            for j in 0..steps {
                let (u, v) = (
                    (i as f32 + 0.5) / steps as f32,
                    (j as f32 + 0.5) / steps as f32,
                );
                let dir = uv_to_dir(u, v);
                let (u2, v2) = dir_to_uv(dir);
                assert!(
                    (u - u2).abs() < 1e-4 && (v - v2).abs() < 1e-4,
                    "({u}, {v}) -> {dir:?} -> ({u2}, {v2})"
                );
            }
        }
    }

    #[test]
    fn poles_and_equator_land_where_the_convention_says() {
        let (u, v) = dir_to_uv([0.0, 0.0, 1.0]);
        assert!(
            (u - 0.5).abs() < 1e-6 && (v - 0.5).abs() < 1e-6,
            "+z is the centre"
        );
        let (u, v) = dir_to_uv([1.0, 0.0, 0.0]);
        assert!(
            (u - 1.0).abs() < 1e-6 && (v - 0.5).abs() < 1e-6,
            "+x is the right edge"
        );
        let (u, v) = dir_to_uv([0.0, 0.0, -1.0]);
        assert!(u == 1.0 && v == 1.0, "-z folds to a corner, got ({u}, {v})");
    }
}
