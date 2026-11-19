//! Seeded randomness, the same on every platform: a SplitMix64 stream for
//! choices, and lattice gradient noise for shapes.

use glam::Vec3;

/// SplitMix64.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `lo..hi`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        let unit = (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32;
        lo + (hi - lo) * unit
    }

    /// A uniformly random unit vector.
    pub fn direction(&mut self) -> Vec3 {
        let z = self.range(-1.0, 1.0);
        let a = self.range(0.0, std::f32::consts::TAU);
        let r = (1.0 - z * z).max(0.0).sqrt();
        Vec3::new(r * a.cos(), z, r * a.sin())
    }
}

pub(crate) fn hash(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    let mut h = seed ^ 0x9e37_79b9;
    for v in [x, y, z] {
        h ^= v as u32;
        h = h.wrapping_mul(0x85eb_ca6b);
        h ^= h >> 13;
        h = h.wrapping_mul(0xc2b2_ae35);
        h ^= h >> 16;
    }
    h
}

const GRADIENTS: [[f32; 3]; 12] = [
    [1.0, 1.0, 0.0],
    [-1.0, 1.0, 0.0],
    [1.0, -1.0, 0.0],
    [-1.0, -1.0, 0.0],
    [1.0, 0.0, 1.0],
    [-1.0, 0.0, 1.0],
    [1.0, 0.0, -1.0],
    [-1.0, 0.0, -1.0],
    [0.0, 1.0, 1.0],
    [0.0, -1.0, 1.0],
    [0.0, 1.0, -1.0],
    [0.0, -1.0, -1.0],
];

/// Improved Perlin noise, roughly −1..1.
pub fn perlin(p: Vec3, seed: u32) -> f32 {
    let cell = p.floor();
    let f = p - cell;
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (u, v, w) = (fade(f.x), fade(f.y), fade(f.z));
    let (cx, cy, cz) = (cell.x as i32, cell.y as i32, cell.z as i32);
    let corner = |dx: i32, dy: i32, dz: i32| {
        let g = GRADIENTS[(hash(cx + dx, cy + dy, cz + dz, seed) % 12) as usize];
        g[0] * (f.x - dx as f32) + g[1] * (f.y - dy as f32) + g[2] * (f.z - dz as f32)
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), u);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), u);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), u);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), u);
    lerp(lerp(x00, x10, v), lerp(x01, x11, v), w)
}

/// Multi-octave noise, roughly −1..1, starting at `frequency`.
pub fn fbm(p: Vec3, frequency: f32, octaves: u32, seed: u32) -> f32 {
    let (mut total, mut norm, mut amplitude, mut f) = (0.0, 0.0, 0.5, frequency);
    for o in 0..octaves {
        total += perlin(p * f, seed.wrapping_add(o * 104_729)) * amplitude;
        norm += amplitude;
        amplitude *= 0.5;
        f *= 2.0;
    }
    total / norm
}
