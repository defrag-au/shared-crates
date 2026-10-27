//! Tiny vec/mat — no glam, so backends don't fight over a glam major.

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const ONE: Self = Self {
        x: 1.0,
        y: 1.0,
        z: 1.0,
    };
    pub const Y: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };

    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn from_array(a: [f32; 3]) -> Self {
        Self {
            x: a[0],
            y: a[1],
            z: a[2],
        }
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Self) -> Self {
        Self {
            x: self.y * o.z - self.z * o.y,
            y: self.z * o.x - self.x * o.z,
            z: self.x * o.y - self.y * o.x,
        }
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn normalized(self) -> Self {
        let l = self.length();
        if l < 1e-8 {
            Self::ZERO
        } else {
            Self {
                x: self.x / l,
                y: self.y / l,
                z: self.z / l,
            }
        }
    }

    pub fn is_near(self, o: Self, eps: f32) -> bool {
        (self.x - o.x).abs() <= eps && (self.y - o.y).abs() <= eps && (self.z - o.z).abs() <= eps
    }
}

impl Default for Vec3 {
    fn default() -> Self {
        Self::ZERO
    }
}

impl std::ops::Add for Vec3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl std::ops::Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}

/// Column-major 4×4.
pub type Mat4 = [[f32; 4]; 4];

pub fn mat4_ident() -> Mat4 {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub fn mat4_mul(a: Mat4, b: Mat4) -> Mat4 {
    let mut r = [[0.0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            r[i][j] = a[0][j] * b[i][0] + a[1][j] * b[i][1] + a[2][j] * b[i][2] + a[3][j] * b[i][3];
        }
    }
    r
}

pub fn transform_point(m: Mat4, p: Vec3) -> Vec3 {
    Vec3 {
        x: m[0][0] * p.x + m[1][0] * p.y + m[2][0] * p.z + m[3][0],
        y: m[0][1] * p.x + m[1][1] * p.y + m[2][1] * p.z + m[3][1],
        z: m[0][2] * p.x + m[1][2] * p.y + m[2][2] * p.z + m[3][2],
    }
}

pub fn transform_dir(m: Mat4, p: Vec3) -> Vec3 {
    Vec3 {
        x: m[0][0] * p.x + m[1][0] * p.y + m[2][0] * p.z,
        y: m[0][1] * p.x + m[1][1] * p.y + m[2][1] * p.z,
        z: m[0][2] * p.x + m[1][2] * p.y + m[2][2] * p.z,
    }
    .normalized()
}

/// A matrix's normal transform — the inverse-transpose of its 3×3 linear
/// part — inverted once, then applied to as many normals as share the
/// matrix. Inverting per vertex was most of flatten's cost on a posed scene.
#[derive(Clone, Copy, Debug)]
pub enum NormalXf {
    /// The inverse of `m` (its transpose is applied).
    Inverse(Mat4),
    /// `m` was singular: fall back to its linear part, as before.
    Linear(Mat4),
}

impl NormalXf {
    pub fn of(m: Mat4) -> Self {
        match mat4_inverse(m) {
            Some(inv) => NormalXf::Inverse(inv),
            None => NormalXf::Linear(m),
        }
    }

    pub fn apply(&self, n: Vec3) -> Vec3 {
        match self {
            NormalXf::Inverse(inv) => Vec3 {
                x: inv[0][0] * n.x + inv[0][1] * n.y + inv[0][2] * n.z,
                y: inv[1][0] * n.x + inv[1][1] * n.y + inv[1][2] * n.z,
                z: inv[2][0] * n.x + inv[2][1] * n.y + inv[2][2] * n.z,
            }
            .normalized(),
            NormalXf::Linear(m) => transform_dir(*m, n),
        }
    }
}

/// T * Rz(ry) * Ry(ry) * Rx(rx) * S. Euler in **radians**.
pub fn mat4_trs(t: Vec3, r_rad: Vec3, s: Vec3) -> Mat4 {
    let (sx, sy, sz) = (s.x, s.y, s.z);
    let (cx, sx_) = (r_rad.x.cos(), r_rad.x.sin());
    let (cy, sy_) = (r_rad.y.cos(), r_rad.y.sin());
    let (cz, sz_) = (r_rad.z.cos(), r_rad.z.sin());
    // R = Rz * Ry * Rx
    let r00 = cz * cy;
    let r01 = cz * sy_ * sx_ - sz_ * cx;
    let r02 = cz * sy_ * cx + sz_ * sx_;
    let r10 = sz_ * cy;
    let r11 = sz_ * sy_ * sx_ + cz * cx;
    let r12 = sz_ * sy_ * cx - cz * sx_;
    let r20 = -sy_;
    let r21 = cy * sx_;
    let r22 = cy * cx;
    [
        [r00 * sx, r10 * sx, r20 * sx, 0.0],
        [r01 * sy, r11 * sy, r21 * sy, 0.0],
        [r02 * sz, r12 * sz, r22 * sz, 0.0],
        [t.x, t.y, t.z, 1.0],
    ]
}

pub fn mat4_look_at(eye: Vec3, at: Vec3, up: Vec3) -> Mat4 {
    let f = Vec3::new(at.x - eye.x, at.y - eye.y, at.z - eye.z).normalized();
    let s = f.cross(up.normalized()).normalized();
    let u = s.cross(f);
    [
        [s.x, u.x, -f.x, 0.0],
        [s.y, u.y, -f.y, 0.0],
        [s.z, u.z, -f.z, 0.0],
        [-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
    ]
}

pub fn transform_vec4(m: Mat4, v: [f32; 4]) -> [f32; 4] {
    [
        m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2] + m[3][0] * v[3],
        m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2] + m[3][1] * v[3],
        m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2] + m[3][2] * v[3],
        m[0][3] * v[0] + m[1][3] * v[1] + m[2][3] * v[2] + m[3][3] * v[3],
    ]
}

#[allow(clippy::needless_range_loop)]
pub fn mat4_inverse(m: Mat4) -> Option<Mat4> {
    let mut a = [[0.0f32; 8]; 4];
    for r in 0..4 {
        for c in 0..4 {
            a[r][c] = m[c][r];
            a[r][c + 4] = if r == c { 1.0 } else { 0.0 };
        }
    }
    for i in 0..4 {
        let mut piv = i;
        let mut best = a[i][i].abs();
        for r in i + 1..4 {
            let v = a[r][i].abs();
            if v > best {
                best = v;
                piv = r;
            }
        }
        if best < 1e-8 {
            return None;
        }
        if piv != i {
            a.swap(i, piv);
        }
        let div = a[i][i];
        for c in a[i].iter_mut() {
            *c /= div;
        }
        for r in 0..4 {
            if r == i {
                continue;
            }
            let f = a[r][i];
            for c in 0..8 {
                a[r][c] -= f * a[i][c];
            }
        }
    }
    let mut inv = mat4_ident();
    for r in 0..4 {
        for c in 0..4 {
            inv[c][r] = a[r][c + 4];
        }
    }
    Some(inv)
}

/// Unproject NDC (OpenGL, z ∈ [-1, 1]) through an inverse view-projection.
pub fn unproject(inv_view_proj: Mat4, ndc: Vec3) -> Option<Vec3> {
    let e = transform_vec4(inv_view_proj, [ndc.x, ndc.y, ndc.z, 1.0]);
    if e[3].abs() < 1e-8 {
        return None;
    }
    Some(Vec3::new(e[0] / e[3], e[1] / e[3], e[2] / e[3]))
}

/// Barycentric hit. `u` weights `v1`, `v` weights `v2`, `1-u-v` weights `v0`.
/// `front` is the +normal / winding side (`dir · n < 0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriHit {
    pub t: f32,
    pub u: f32,
    pub v: f32,
    pub front: bool,
}

/// Two-sided Möller–Trumbore. `dir` should be unit.
pub fn ray_triangle(orig: Vec3, dir: Vec3, v0: Vec3, v1: Vec3, v2: Vec3) -> Option<TriHit> {
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    let h = dir.cross(edge2);
    let a = edge1.dot(h);
    if a.abs() < 1e-8 {
        return None;
    }
    let f = 1.0 / a;
    let s = orig - v0;
    let u = f * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(edge1);
    let v = f * dir.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = f * edge2.dot(q);
    if t > 1e-4 {
        Some(TriHit {
            t,
            u,
            v,
            front: a > 0.0,
        })
    } else {
        None
    }
}

pub fn lerp_uv(uv0: [f32; 2], uv1: [f32; 2], uv2: [f32; 2], u: f32, v: f32) -> [f32; 2] {
    let w = 1.0 - u - v;
    [
        w * uv0[0] + u * uv1[0] + v * uv2[0],
        w * uv0[1] + u * uv1[1] + v * uv2[1],
    ]
}

pub(crate) fn compact_f(v: f32) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn mat4_perspective(fov_rad: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_rad * 0.5).tan();
    let nf = 1.0 / (near - far);
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, (far + near) * nf, -1.0],
        [0.0, 0.0, 2.0 * far * near * nf, 0.0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_offset_composes() {
        let parent = mat4_trs(Vec3::new(1.0, 0.0, 0.0), Vec3::ZERO, Vec3::ONE);
        let child = mat4_trs(Vec3::new(0.0, 2.0, 0.0), Vec3::ZERO, Vec3::ONE);
        let w = mat4_mul(parent, child);
        let p = transform_point(w, Vec3::ZERO);
        assert!(p.is_near(Vec3::new(1.0, 2.0, 0.0), 1e-5), "{p:?}");
    }

    #[test]
    fn inverse_undoes_trs() {
        let m = mat4_trs(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(0.2, -0.4, 0.1),
            Vec3::new(2.0, 0.5, 1.5),
        );
        let inv = mat4_inverse(m).unwrap();
        let p = transform_point(mat4_mul(inv, m), Vec3::new(0.3, -1.0, 2.0));
        assert!(p.is_near(Vec3::new(0.3, -1.0, 2.0), 1e-4), "{p:?}");
    }

    #[test]
    fn ray_hits_unit_triangle() {
        let hit = ray_triangle(
            Vec3::new(0.0, 0.0, 2.0),
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        )
        .unwrap();
        assert!((hit.t - 2.0).abs() < 1e-4, "{}", hit.t);
        assert!(hit.front);
        assert!(ray_triangle(
            Vec3::new(3.0, 0.0, 2.0),
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        )
        .is_none());
    }

    #[test]
    fn inverse_transpose_fixes_scaled_normals() {
        let m = mat4_trs(
            Vec3::ZERO,
            Vec3::new(0.0, std::f32::consts::FRAC_PI_4, 0.0),
            Vec3::new(2.0, 1.0, 1.0),
        );
        let n = NormalXf::of(m).apply(Vec3::new(1.0, 0.0, 1.0));
        let wrong = transform_dir(m, Vec3::new(1.0, 0.0, 1.0));
        let expect = Vec3::new(0.9487, 0.0, 0.3162);
        assert!(n.is_near(expect, 2e-3), "{n:?}");
        assert!(
            (n.z - wrong.z).abs() > 0.4,
            "dir vs normal should disagree, n={n:?} dir={wrong:?}"
        );
    }
}
