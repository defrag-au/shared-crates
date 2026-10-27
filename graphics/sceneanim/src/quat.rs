//! Minimal quat so Linear rotation is slerp, not euler lerp.

use scenegraph::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Quat {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Quat {
    pub const IDENTITY: Self = Self {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Matches scenegraph `mat4_trs` euler: Rz * Ry * Rx.
    pub fn from_euler_xyz(r: Vec3) -> Self {
        let (cx, sx) = ((r.x * 0.5).cos(), (r.x * 0.5).sin());
        let (cy, sy) = ((r.y * 0.5).cos(), (r.y * 0.5).sin());
        let (cz, sz) = ((r.z * 0.5).cos(), (r.z * 0.5).sin());
        Self {
            w: cz * cy * cx + sz * sy * sx,
            x: cz * cy * sx - sz * sy * cx,
            y: cz * sy * cx + sz * cy * sx,
            z: sz * cy * cx - cz * sy * sx,
        }
    }

    pub fn to_euler_xyz(self) -> Vec3 {
        let Self { w, x, y, z } = self;
        let sinp = 2.0 * (w * y - z * x);
        let pitch = if sinp.abs() >= 1.0 {
            std::f32::consts::FRAC_PI_2.copysign(sinp)
        } else {
            sinp.asin()
        };
        Vec3::new(
            (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y)),
            pitch,
            (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z)),
        )
    }

    pub fn dot(self, o: Self) -> f32 {
        self.w * o.w + self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn normalized(self) -> Self {
        let l = self.dot(self).sqrt();
        if l < 1e-8 {
            Self::IDENTITY
        } else {
            Self {
                w: self.w / l,
                x: self.x / l,
                y: self.y / l,
                z: self.z / l,
            }
        }
    }

    pub fn slerp(self, mut other: Self, t: f32) -> Self {
        let a = self.normalized();
        other = other.normalized();
        let mut d = a.dot(other);
        if d < 0.0 {
            other = Self {
                w: -other.w,
                x: -other.x,
                y: -other.y,
                z: -other.z,
            };
            d = -d;
        }
        if d > 0.9995 {
            return Self {
                w: a.w + t * (other.w - a.w),
                x: a.x + t * (other.x - a.x),
                y: a.y + t * (other.y - a.y),
                z: a.z + t * (other.z - a.z),
            }
            .normalized();
        }
        let th = d.clamp(-1.0, 1.0).acos();
        let s = th.sin();
        let wa = ((1.0 - t) * th).sin() / s;
        let wb = (t * th).sin() / s;
        Self {
            w: wa * a.w + wb * other.w,
            x: wa * a.x + wb * other.x,
            y: wa * a.y + wb * other.y,
            z: wa * a.z + wb * other.z,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euler_roundtrip_yaw() {
        // Avoid ±π/2 pitch — XYZ extraction is singular there.
        let r = Vec3::new(0.2, 1.0, -0.3);
        let back = Quat::from_euler_xyz(r).to_euler_xyz();
        assert!(back.is_near(r, 1e-4), "{back:?}");
    }

    #[test]
    fn slerp_ends() {
        let a = Quat::from_euler_xyz(Vec3::ZERO);
        let b = Quat::from_euler_xyz(Vec3::new(0.0, 1.0, 0.0));
        let z = a.slerp(b, 0.0).to_euler_xyz();
        let o = a.slerp(b, 1.0).to_euler_xyz();
        assert!(z.is_near(Vec3::ZERO, 1e-4), "{z:?}");
        assert!(o.is_near(Vec3::new(0.0, 1.0, 0.0), 1e-4), "{o:?}");
    }
}
