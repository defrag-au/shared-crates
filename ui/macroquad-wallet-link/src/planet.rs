//! The bodies of the orbit view, baked once into textures: the home planet (a
//! lit, banded, cratered sphere in an atmosphere) and the moon every linked
//! wallet is drawn as (a grey sphere, tinted at draw time).
//!
//! Baked on the CPU into an `Image`, so drawing one is a single textured quad
//! a frame, at any size. The pixel functions are plain arithmetic, tested
//! without a window.

use macroquad::prelude::*;

/// Where the light comes from: up, left and towards the viewer.
const LIGHT: Vec3 = Vec3::new(-0.52, -0.58, 0.63);

/// How far the planet's atmosphere reaches, in planet radii. The texture is
/// this wide, so the sphere fills `1 / ATMOSPHERE` of it.
pub const ATMOSPHERE: f32 = 1.38;

/// The planet's colours, from the theme: never a palette of its own.
#[derive(Clone, Copy, Debug)]
pub struct PlanetLook {
    /// The deep ground between the bands.
    pub ground: Color,
    /// The bright bands, and the atmosphere.
    pub band: Color,
    /// The second band colour, mixed through the first.
    pub band2: Color,
    pub seed: u32,
}

/// A hash of a lattice point, 0..1.
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x27d4_eb2d)
        ^ (y as u32).wrapping_mul(0x1656_67b1)
        ^ seed.wrapping_mul(0x9e37_79b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    (h & 0x00ff_ffff) as f32 / 0x0100_0000 as f32
}

/// Smooth value noise at `p`, 0..1.
fn noise(p: Vec2, seed: u32) -> f32 {
    let (x0, y0) = (p.x.floor(), p.y.floor());
    let (fx, fy) = (p.x - x0, p.y - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (ix, iy) = (x0 as i32, y0 as i32);
    let a = hash(ix, iy, seed);
    let b = hash(ix + 1, iy, seed);
    let c = hash(ix, iy + 1, seed);
    let d = hash(ix + 1, iy + 1, seed);
    let top = a + (b - a) * sx;
    let bottom = c + (d - c) * sx;
    top + (bottom - top) * sy
}

/// Three octaves of [`noise`], 0..1.
fn fbm(p: Vec2, seed: u32) -> f32 {
    noise(p, seed) * 0.57 + noise(p * 2.1, seed + 1) * 0.29 + noise(p * 4.3, seed + 2) * 0.14
}

/// `a` towards `b` by `t`.
pub(crate) fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The planet's colour at `p`, a point in the texture as −1..1 on each axis
/// (so the sphere's edge is at `1 / ATMOSPHERE`). Premultiplied by nothing:
/// straight alpha.
pub fn planet_pixel(p: Vec2, look: &PlanetLook, px: f32) -> Color {
    let r = p.length() * ATMOSPHERE;
    let edge = smoothstep(1.0, 1.0 - 1.6 * px * ATMOSPHERE, r);
    // Outside: the atmosphere, falling off from the limb.
    let halo = {
        let t = ((r - 1.0) / (ATMOSPHERE - 1.0)).clamp(0.0, 1.0);
        let k = (1.0 - t).powf(2.6) * 0.34;
        Color::new(look.band.r, look.band.g, look.band.b, k)
    };
    if r >= 1.0 {
        return halo;
    }
    let q = p * ATMOSPHERE;
    let z = (1.0 - q.length_squared()).max(0.0).sqrt();
    let n = vec3(q.x, q.y, z);
    let lit = n.dot(LIGHT.normalize()).max(0.0);
    // Bands of latitude, as a gas giant has: wrapped onto the sphere (the
    // surface's own longitude stretches them at the limb), and warped a
    // little by noise so they swirl rather than rule.
    let lon = q.x / (z + 0.25);
    let warp = fbm(vec2(lon * 1.4 + 5.0, q.y * 3.0), look.seed);
    let bands = (q.y * 8.5 + warp * 2.2).sin() * 0.5 + 0.5;
    let streaks = fbm(vec2(lon * 2.5, q.y * 14.0), look.seed + 7);
    let mut c = mix(look.ground, look.band, smoothstep(0.3, 0.92, bands) * 0.72);
    c = mix(c, look.band2, smoothstep(0.55, 0.85, streaks) * 0.42);
    // Craters: dark discs with a lit rim, where the noise peaks.
    let crater = noise(vec2(q.x * 5.0, q.y * 5.0) / (z + 0.6), look.seed + 11);
    if crater > 0.78 {
        let k = smoothstep(0.78, 0.86, crater);
        c = mix(c, look.ground, k * 0.55);
    }
    // Light: a soft terminator, a little ambient, and the limb catching the
    // atmosphere's glow.
    let shade = 0.16 + 0.84 * smoothstep(0.0, 0.85, lit);
    let rim = (1.0 - z).powi(3);
    let lit_c = Color::new(c.r * shade, c.g * shade, c.b * shade, 1.0);
    let body = mix(lit_c, look.band, rim * 0.6);
    // The edge pixels blend into the halo rather than into nothing.
    mix(halo, body, edge)
}

/// A moon's grey at `p` (−1..1, the sphere filling the texture), tinted when
/// drawn.
pub fn moon_pixel(p: Vec2, seed: u32, px: f32) -> Color {
    let r = p.length();
    let edge = smoothstep(1.0, 1.0 - 1.6 * px, r);
    if r >= 1.0 {
        return Color::new(1.0, 1.0, 1.0, 0.0);
    }
    let z = (1.0 - r * r).max(0.0).sqrt();
    let n = vec3(p.x, p.y, z);
    let lit = n.dot(LIGHT.normalize()).max(0.0);
    let ground = 0.78 + 0.22 * fbm(p * 3.0, seed);
    let crater = noise(p * 4.0 / (z + 0.6), seed + 3);
    let pit = if crater > 0.74 {
        1.0 - smoothstep(0.74, 0.84, crater) * 0.3
    } else {
        1.0
    };
    let shade = (0.22 + 0.78 * smoothstep(0.0, 0.9, lit)) * ground * pit;
    let rim = (1.0 - z).powi(4) * 0.35;
    let v = (shade + rim).min(1.0);
    Color::new(v, v, v, edge)
}

/// Bake `size`×`size` pixels of `f` into a texture, linear-filtered.
fn bake(size: u16, f: impl Fn(Vec2, f32) -> Color) -> Texture2D {
    let mut image = Image::gen_image_color(size, size, Color::new(0.0, 0.0, 0.0, 0.0));
    let px = 2.0 / f32::from(size);
    for y in 0..u32::from(size) {
        for x in 0..u32::from(size) {
            let p = vec2((x as f32 + 0.5) * px - 1.0, (y as f32 + 0.5) * px - 1.0);
            image.set_pixel(x, y, f(p, px));
        }
    }
    let texture = Texture2D::from_image(&image);
    texture.set_filter(FilterMode::Linear);
    texture
}

/// The home planet, atmosphere included.
pub fn bake_planet(look: &PlanetLook, size: u16) -> Texture2D {
    bake(size, |p, px| planet_pixel(p, look, px))
}

/// The moon every wallet is drawn as, in greys.
pub fn bake_moon(seed: u32, size: u16) -> Texture2D {
    bake(size, |p, px| moon_pixel(p, seed, px))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look() -> PlanetLook {
        PlanetLook {
            ground: Color::new(0.05, 0.12, 0.10, 1.0),
            band: Color::new(0.6, 0.9, 0.3, 1.0),
            band2: Color::new(0.3, 0.8, 0.8, 1.0),
            seed: 7,
        }
    }

    #[test]
    fn the_sphere_is_solid_and_the_corners_are_empty() {
        let l = look();
        assert_eq!(planet_pixel(Vec2::ZERO, &l, 0.01).a, 1.0);
        assert!(planet_pixel(vec2(1.0, 1.0), &l, 0.01).a < 0.01);
        assert_eq!(moon_pixel(Vec2::ZERO, 3, 0.01).a, 1.0);
        assert_eq!(moon_pixel(vec2(0.99, 0.99), 3, 0.01).a, 0.0);
    }

    /// Lit from the upper left: that side of the sphere is brighter than the
    /// lower right, which is what reads as a ball rather than a disc.
    #[test]
    fn the_lit_side_is_brighter() {
        let l = look();
        let lum = |c: Color| c.r + c.g + c.b;
        let s = 0.4 / ATMOSPHERE;
        assert!(
            lum(planet_pixel(vec2(-s, -s), &l, 0.01)) > lum(planet_pixel(vec2(s, s), &l, 0.01))
        );
        assert!(
            lum(moon_pixel(vec2(-0.4, -0.4), 3, 0.01)) > lum(moon_pixel(vec2(0.4, 0.4), 3, 0.01))
        );
    }

    #[test]
    fn the_atmosphere_fades_outwards() {
        let l = look();
        let just_out = planet_pixel(vec2(1.05 / ATMOSPHERE, 0.0), &l, 0.01).a;
        let far_out = planet_pixel(vec2(1.3 / ATMOSPHERE, 0.0), &l, 0.01).a;
        assert!(just_out > far_out && far_out >= 0.0);
    }
}
