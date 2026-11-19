//! Noise on the sphere, for texture the bake does not carry.

use std::f32::consts::PI;

use glam::Vec3;

/// Hash of a point to 0..1.
pub fn hash(p: Vec3) -> f32 {
    let h = (p.x * 127.1 + p.y * 311.7 + p.z * 74.7).sin() * 43_758.547;
    h - h.floor()
}

/// Value noise, 0..1.
pub fn grain(dir: Vec3, scale: f32) -> f32 {
    let p = dir * scale;
    let cell = p.floor();
    let f = p - cell;
    let f = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let mut total = 0.0;
    for corner in 0..8 {
        let o = Vec3::new(
            (corner & 1) as f32,
            ((corner >> 1) & 1) as f32,
            ((corner >> 2) & 1) as f32,
        );
        let w = (if o.x > 0.5 { f.x } else { 1.0 - f.x })
            * (if o.y > 0.5 { f.y } else { 1.0 - f.y })
            * (if o.z > 0.5 { f.z } else { 1.0 - f.z });
        total += hash(cell + o) * w;
    }
    total
}

/// Gradient (Perlin-style) noise in roughly [-1, 1]. Unlike the value noise in
/// [`grain`], it has no visible lattice, so ridges built from it do not read
/// as blocks.
pub fn gradient_noise(p: Vec3) -> f32 {
    let cell = p.floor();
    let f = p - cell;
    let fade = f * f * f * (f * (f * 6.0 - Vec3::splat(15.0)) + Vec3::splat(10.0));
    let corner = |o: Vec3| {
        let h = hash(cell + o);
        let theta = h * 2.0 * PI;
        let z = hash(cell + o + Vec3::splat(17.3)) * 2.0 - 1.0;
        let r = (1.0 - z * z).sqrt();
        Vec3::new(r * theta.cos(), r * theta.sin(), z).dot(f - o)
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(corner(Vec3::ZERO), corner(Vec3::X), fade.x);
    let x10 = lerp(corner(Vec3::Y), corner(Vec3::new(1.0, 1.0, 0.0)), fade.x);
    let x01 = lerp(corner(Vec3::Z), corner(Vec3::new(1.0, 0.0, 1.0)), fade.x);
    let x11 = lerp(corner(Vec3::new(0.0, 1.0, 1.0)), corner(Vec3::ONE), fade.x);
    lerp(lerp(x00, x10, fade.y), lerp(x01, x11, fade.y), fade.z) * 1.6
}

/// Regional climate anomalies the chart adds to the bake's climate: Kelvin,
/// and a precipitation offset (the bake's 0–1 scale). Smooth, continental in
/// scale (two octaves, the coarse one a few thousand km across on an Earth):
/// typically ~3.6 K and ~0.06, ten Kelvin or so at the extremes.
///
/// A generated climate is mostly a function of latitude, so its isotherms,
/// and the ice edge and biome borders drawn from them, run as straight belts
/// round the globe. Added to it, an anomaly bends each isotherm by the
/// anomaly over the local gradient: widely across gentle latitude gradients,
/// barely against a mountain range, where cold sits where the height is.
/// Every frost and biome decision reads the same anomaly, so the ice edge,
/// tundra, forest and rivers stay consistent.
pub fn climate_anomaly(dir: Vec3) -> (f32, f32) {
    // Three octaves: continental swings, regional ones, and a fine wander
    // so a border crossing a smooth gradient does not run straight for a
    // thousand km.
    let field = |seed: f32| {
        gradient_noise(dir * 2.3 + Vec3::splat(seed)) * 0.6
            + gradient_noise(dir * 6.1 + Vec3::splat(seed + 5.3)) * 0.28
            + gradient_noise(dir * 15.7 + Vec3::splat(seed + 11.9)) * 0.17
    };
    // The field's standard deviation is ~0.2 (measured; see the test), so
    // these give ~3.6 K and ~0.06 typical.
    (field(4.1) * 18.0, field(21.9) * 0.3)
}

/// Ridged multi-octave noise in [0, 1]: sharp crests, soft valleys. Each
/// octave is rotated so no octave's grid lines up with another's.
pub fn ridged(dir: Vec3) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 0.5;
    let mut scale = 40.0;
    let mut norm = 0.0;
    let mut weight = 1.0;
    for octave in 0..7 {
        let axis = Vec3::new(0.3 + octave as f32, 1.0, 0.7 - octave as f32 * 0.4).normalize();
        let rotated = glam::Quat::from_axis_angle(axis, 0.9 + octave as f32 * 1.3) * dir;
        let n = 1.0 - gradient_noise(rotated * scale).abs();
        let n = n * n * weight;
        // Higher octaves only roughen what the lower ones already raised.
        weight = (n * 1.8).clamp(0.0, 1.0);
        total += n * amplitude;
        norm += amplitude;
        amplitude *= 0.52;
        scale *= 2.07;
    }
    total / norm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn climate_anomalies_are_regional_in_size() {
        // Typical (standard deviation) anomalies of ~3–4 K and ~0.06: Europe
        // runs ~10 K warmer than Labrador at the same latitude, so a few
        // Kelvin typical, ten or so at the extremes.
        let n = 20_000;
        let golden = PI * (3.0 - 5f32.sqrt());
        let (mut t, mut p) = (Vec::new(), Vec::new());
        for k in 0..n {
            let y = 1.0 - 2.0 * (k as f32 + 0.5) / n as f32;
            let r = (1.0 - y * y).sqrt();
            let a = golden * k as f32;
            let (dt, dp) = climate_anomaly(Vec3::new(r * a.cos(), y, r * a.sin()));
            t.push(dt);
            p.push(dp);
        }
        let sd = |v: &[f32]| {
            let mean = v.iter().sum::<f32>() / v.len() as f32;
            (v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
        };
        let (st, sp) = (sd(&t), sd(&p));
        println!("anomaly sd: {st:.2} K, {sp:.3} precipitation");
        assert!((3.0..4.5).contains(&st), "temperature anomaly sd {st} K");
        assert!((0.04..0.08).contains(&sp), "precipitation anomaly sd {sp}");
    }
}
