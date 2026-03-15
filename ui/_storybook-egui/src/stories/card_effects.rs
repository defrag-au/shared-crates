//! Card visual effects — swappable overlay implementations for NFT shapes.
//!
//! Each effect implements `CardEffect`, computing per-vertex overlay colours
//! from normalised UV coordinates and mouse position.  Effects are stateless
//! per-frame and purely mathematical — no textures or fragment shaders.

use egui::Color32;
use std::f32::consts::TAU;

// ============================================================================
// Trait + vertex input
// ============================================================================

/// Per-vertex input for effect computation.
pub struct EffectVertex {
    pub norm_u: f32,
    pub norm_v: f32,
}

/// Trait for card visual effects (holographic, foil, iridescent, etc).
pub trait CardEffect {
    /// Human-readable name for the UI selector.
    #[allow(dead_code)]
    fn name(&self) -> &str;

    /// Compute overlay colour for each vertex given normalised positions
    /// and the current mouse position (also normalised 0..1).
    fn compute_colors(&self, vertices: &[EffectVertex], mouse_u: f32, mouse_v: f32)
        -> Vec<Color32>;
}

/// Available effects for the UI selector.
pub const EFFECT_NAMES: &[&str] = &[
    "Streak Holo",
    "Thin Film",
    "Diffraction",
    "Glitter",
    "Brushed Metal",
    "Aurora",
    "Prismatic",
];

// ============================================================================
// Colour utilities (used by StreakHolo)
// ============================================================================

fn hue_to_rgb(hue: f32) -> Color32 {
    let h = ((hue % 360.0) + 360.0) % 360.0 / 60.0;
    let i = h.floor() as u8;
    let f = h - h.floor();
    let q = (255.0 * (1.0 - f)) as u8;
    let t = (255.0 * f) as u8;
    match i {
        0 => Color32::from_rgb(255, t, 0),
        1 => Color32::from_rgb(q, 255, 0),
        2 => Color32::from_rgb(0, 255, t),
        3 => Color32::from_rgb(0, q, 255),
        4 => Color32::from_rgb(t, 0, 255),
        _ => Color32::from_rgb(255, 0, q),
    }
}

/// Compute holo vertex colour from normalised position and light direction.
#[allow(clippy::too_many_arguments)]
fn holo_color(
    u: f32,
    v: f32,
    mouse_u: f32,
    mouse_v: f32,
    hue_range: f32,
    shimmer_width: f32,
    shimmer_intensity: f32,
    overlay_opacity: f32,
) -> Color32 {
    let light_x = mouse_u - 0.5;
    let light_y = mouse_v - 0.5;
    let light_len = (light_x * light_x + light_y * light_y).sqrt().max(0.001);
    let light_dx = light_x / light_len;
    let light_dy = light_y / light_len;

    let dx = u - mouse_u;
    let dy = v - mouse_v;
    let dot = dx * light_dx + dy * light_dy;
    let streak_dist = dot.abs();
    let streak = (-streak_dist * streak_dist / (shimmer_width * shimmer_width * 0.5)).exp()
        * shimmer_intensity;
    let cross = dx * light_dy - dy * light_dx;
    let hue = 200.0 + cross * hue_range * 2.0;

    let edge_u = (0.5 - (u - 0.5).abs()) * 2.0;
    let edge_v = (0.5 - (v - 0.5).abs()) * 2.0;
    let fresnel = 1.0 - edge_u.min(edge_v).clamp(0.0, 1.0);
    let fresnel_boost = fresnel * fresnel * overlay_opacity * 0.5;
    let intensity = (streak + fresnel_boost).clamp(0.0, 1.0);

    let rainbow = hue_to_rgb(hue);
    let alpha = (intensity * 255.0) as u8;
    Color32::from_rgba_premultiplied(
        (rainbow.r() as f32 * intensity) as u8,
        (rainbow.g() as f32 * intensity) as u8,
        (rainbow.b() as f32 * intensity) as u8,
        alpha,
    )
}

// ============================================================================
// Hash utilities (used by Glitter, PrismaticDispersion)
// ============================================================================

/// Simple deterministic hash for 2D → f32.
fn hash1(x: f32, y: f32) -> f32 {
    let p = x * 12.9898 + y * 4.1414;
    (p.sin() * 43_758.547).fract().abs()
}

/// Simple deterministic hash for 2D → (f32, f32).
fn hash2(x: f32, y: f32) -> (f32, f32) {
    let p1 = x * 127.1 + y * 311.7;
    let p2 = x * 269.5 + y * 183.3;
    (
        (p1.sin() * 43_758.547).fract().abs(),
        (p2.sin() * 43_758.547).fract().abs(),
    )
}

// ============================================================================
// Spectral utilities (used by DiffractionGrating)
// ============================================================================

/// Zucconi6 spectral approximation (wavelength 400..700nm → RGB).
fn spectral_zucconi6(w: f32) -> [f32; 3] {
    let x = ((w - 400.0) / 300.0).clamp(0.0, 1.0);

    fn bump(x: f32, center: f32, width: f32) -> f32 {
        let t = (x - center) * width;
        (1.0 - t * t).max(0.0)
    }

    let r = bump(x, 0.6955, 3.5459) + bump(x, 0.1175, 3.9031);
    let g = bump(x, 0.4923, 2.9323) + bump(x, 0.8676, 3.2118);
    let b = bump(x, 0.2770, 2.4159) + bump(x, 0.6608, 3.9659);

    [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)]
}

// ============================================================================
// 1. Streak Holo
// ============================================================================

/// Specular streak + iridescence + Fresnel edge glow (original effect).
#[derive(Clone, Copy)]
pub struct StreakHolo {
    pub hue_range: f32,
    pub shimmer_width: f32,
    pub shimmer_intensity: f32,
    pub overlay_opacity: f32,
}

impl CardEffect for StreakHolo {
    fn name(&self) -> &str {
        "Streak Holo"
    }

    fn compute_colors(
        &self,
        vertices: &[EffectVertex],
        mouse_u: f32,
        mouse_v: f32,
    ) -> Vec<Color32> {
        vertices
            .iter()
            .map(|v| {
                holo_color(
                    v.norm_u,
                    v.norm_v,
                    mouse_u,
                    mouse_v,
                    self.hue_range,
                    self.shimmer_width,
                    self.shimmer_intensity,
                    self.overlay_opacity,
                )
            })
            .collect()
    }
}

// ============================================================================
// 2. Thin Film Iridescence
// ============================================================================

/// Thin-film interference — oil-on-water / soap bubble rainbow.
/// Uses sine-wave RGB channels at different wavelengths modulated by
/// Fresnel–Schlick approximation for view-angle dependent intensity.
#[derive(Clone, Copy)]
pub struct ThinFilmIridescence {
    pub iri_min: f32,       // minimum film thickness (nm)
    pub iri_range: f32,     // thickness variation range (nm)
    pub fresnel_power: f32, // Schlick exponent (higher = more edge-only)
    pub intensity: f32,     // overall effect strength
}

impl CardEffect for ThinFilmIridescence {
    fn name(&self) -> &str {
        "Thin Film"
    }

    fn compute_colors(
        &self,
        vertices: &[EffectVertex],
        mouse_u: f32,
        mouse_v: f32,
    ) -> Vec<Color32> {
        vertices
            .iter()
            .map(|v| {
                let du = v.norm_u - mouse_u;
                let dv = v.norm_v - mouse_v;
                let dist = (du * du + dv * dv).sqrt();
                let cos_theta = (1.0 - dist * 1.5).clamp(0.0, 1.0);

                let thickness = self.iri_min + self.iri_range * (1.0 - cos_theta);

                let pi2 = std::f32::consts::TAU;
                let r = (thickness * pi2 / 650.0).sin() * 0.5 + 0.5;
                let g = (thickness * pi2 / 510.0).sin() * 0.5 + 0.5;
                let b = (thickness * pi2 / 440.0).sin() * 0.5 + 0.5;

                let fresnel = (1.0 - cos_theta).powf(self.fresnel_power);
                let alpha = (fresnel * self.intensity).clamp(0.0, 1.0);

                Color32::from_rgba_premultiplied(
                    (r * alpha * 255.0) as u8,
                    (g * alpha * 255.0) as u8,
                    (b * alpha * 255.0) as u8,
                    (alpha * 255.0) as u8,
                )
            })
            .collect()
    }
}

// ============================================================================
// 3. Diffraction Grating
// ============================================================================

/// Diffraction grating — sharp rainbow bands like a CD/DVD holographic sticker.
/// Uses the grating equation with multiple diffraction orders summed via the
/// Zucconi spectral approximation for physically-accurate rainbow colours.
#[derive(Clone, Copy)]
pub struct DiffractionGrating {
    pub grating_spacing: f32, // controls rainbow density (800..3000)
    pub grating_angle: f32,   // tangent rotation in radians
    pub max_orders: u32,      // diffraction orders to sum (1..8)
    pub intensity: f32,       // overall brightness
}

impl CardEffect for DiffractionGrating {
    fn name(&self) -> &str {
        "Diffraction"
    }

    fn compute_colors(
        &self,
        vertices: &[EffectVertex],
        mouse_u: f32,
        mouse_v: f32,
    ) -> Vec<Color32> {
        let (tang_cos, tang_sin) = (self.grating_angle.cos(), self.grating_angle.sin());

        vertices
            .iter()
            .map(|v| {
                let lx = mouse_u - v.norm_u;
                let ly = mouse_v - v.norm_v;
                let l_len = (lx * lx + ly * ly).sqrt().max(0.001);
                let l_dx = lx / l_len;
                let l_dy = ly / l_len;

                let vx = 0.5 - v.norm_u;
                let vy = 0.5 - v.norm_v;
                let v_len = (vx * vx + vy * vy).sqrt().max(0.001);
                let v_dx = vx / v_len;
                let v_dy = vy / v_len;

                let dot_l_t = l_dx * tang_cos + l_dy * tang_sin;
                let dot_v_t = v_dx * tang_cos + v_dy * tang_sin;
                let u_param = (dot_l_t - dot_v_t).abs();

                let mut r = 0.0_f32;
                let mut g = 0.0_f32;
                let mut b = 0.0_f32;
                let orders = self.max_orders.max(1);
                for n in 1..=orders {
                    let wavelength = u_param * self.grating_spacing / n as f32;
                    if (400.0..=700.0).contains(&wavelength) {
                        let rgb = spectral_zucconi6(wavelength);
                        r += rgb[0];
                        g += rgb[1];
                        b += rgb[2];
                    }
                }
                let scale = self.intensity / orders as f32;
                r *= scale;
                g *= scale;
                b *= scale;

                let alpha = (r + g + b).clamp(0.0, 1.0) / 3.0 * self.intensity;
                Color32::from_rgba_premultiplied(
                    (r.clamp(0.0, 1.0) * 255.0) as u8,
                    (g.clamp(0.0, 1.0) * 255.0) as u8,
                    (b.clamp(0.0, 1.0) * 255.0) as u8,
                    (alpha.clamp(0.0, 1.0) * 255.0) as u8,
                )
            })
            .collect()
    }
}

// ============================================================================
// 4. Glitter
// ============================================================================

/// Glitter / micro-sparkle — hundreds of tiny flashing points that light up
/// as the mouse moves, like holographic foil with embedded metallic flakes.
#[derive(Clone, Copy)]
pub struct Glitter {
    pub grid_scale: f32,        // number of sparkle cells (10..80)
    pub sparkle_sharpness: f32, // pow exponent for alignment (50..500)
    pub sparkle_threshold: f32, // fraction of cells that can sparkle (0..1)
    pub z_depth: f32,           // view vector z, controls mouse sensitivity (0.1..1.0)
}

impl CardEffect for Glitter {
    fn name(&self) -> &str {
        "Glitter"
    }

    fn compute_colors(
        &self,
        vertices: &[EffectVertex],
        mouse_u: f32,
        mouse_v: f32,
    ) -> Vec<Color32> {
        vertices
            .iter()
            .map(|v| {
                let su = v.norm_u * self.grid_scale;
                let sv = v.norm_v * self.grid_scale;
                let cell_x = su.floor();
                let cell_y = sv.floor();
                let local_x = su.fract();
                let local_y = sv.fract();

                let (rx, ry) = hash2(cell_x, cell_y);
                let theta = rx * TAU;
                let phi = ry * std::f32::consts::FRAC_PI_2;
                let sparkle_x = phi.sin() * theta.cos();
                let sparkle_y = phi.sin() * theta.sin();
                let sparkle_z = phi.cos();

                let vx = mouse_u - v.norm_u;
                let vy = mouse_v - v.norm_v;
                let vl = (vx * vx + vy * vy + self.z_depth * self.z_depth).sqrt();
                let view_x = vx / vl;
                let view_y = vy / vl;
                let view_z = self.z_depth / vl;

                let alignment =
                    (sparkle_x * view_x + sparkle_y * view_y + sparkle_z * view_z).max(0.0);
                let sparkle = alignment.powf(self.sparkle_sharpness);

                let dx = local_x - 0.5;
                let dy = local_y - 0.5;
                let dist = (dx * dx + dy * dy).sqrt();
                let mask = ((0.45 - dist) / 0.1).clamp(0.0, 1.0);

                let brightness = hash1(cell_x + 42.0, cell_y + 17.0);
                let active = if brightness > self.sparkle_threshold {
                    1.0
                } else {
                    0.0
                };

                let intensity = (sparkle * mask * active).clamp(0.0, 1.0);
                let c = (intensity * 255.0) as u8;
                Color32::from_rgba_premultiplied(c, c, c, (intensity * 200.0).min(255.0) as u8)
            })
            .collect()
    }
}

// ============================================================================
// 5. Brushed Metal
// ============================================================================

/// Brushed metal — elongated anisotropic highlight like brushed steel, gold, or copper.
/// Based on Ward's anisotropic BRDF model.
#[derive(Clone, Copy)]
pub struct BrushedMetal {
    pub roughness_along: f32, // roughness along brush direction (0.01..0.5)
    pub roughness_perp: f32,  // roughness perpendicular (0.1..2.0)
    pub brush_angle: f32,     // brush direction in radians
    pub metal_r: f32,         // metal tint RGB
    pub metal_g: f32,
    pub metal_b: f32,
}

impl CardEffect for BrushedMetal {
    fn name(&self) -> &str {
        "Brushed Metal"
    }

    fn compute_colors(
        &self,
        vertices: &[EffectVertex],
        mouse_u: f32,
        mouse_v: f32,
    ) -> Vec<Color32> {
        let (tc, ts) = (self.brush_angle.cos(), self.brush_angle.sin());

        vertices
            .iter()
            .map(|v| {
                let lx = mouse_u - v.norm_u;
                let ly = mouse_v - v.norm_v;
                let vx = 0.5 - v.norm_u;
                let vy = 0.5 - v.norm_v;

                let hx = lx + vx;
                let hy = ly + vy;
                let hz = 0.5_f32 + 1.0_f32;
                let h_len = (hx * hx + hy * hy + hz * hz).sqrt().max(0.001);
                let hx = hx / h_len;
                let hy = hy / h_len;
                let hz = hz / h_len;

                let h_dot_t = hx * tc + hy * ts;
                let h_dot_b = hx * (-ts) + hy * tc;
                let h_dot_n = hz;

                let exponent = -2.0
                    * ((h_dot_t / self.roughness_along).powi(2)
                        + (h_dot_b / self.roughness_perp).powi(2))
                    / (1.0 + h_dot_n);
                let specular = (exponent.exp()
                    / (4.0 * std::f32::consts::PI * self.roughness_along * self.roughness_perp))
                    .clamp(0.0, 1.0);

                let r = (self.metal_r * specular * 255.0).min(255.0) as u8;
                let g = (self.metal_g * specular * 255.0).min(255.0) as u8;
                let b = (self.metal_b * specular * 255.0).min(255.0) as u8;
                let alpha = (specular * 0.8 * 255.0).min(255.0) as u8;
                Color32::from_rgba_premultiplied(r, g, b, alpha)
            })
            .collect()
    }
}

// ============================================================================
// 6. Aurora
// ============================================================================

/// Aurora / northern lights — flowing curtains of green, teal, purple, and pink
/// that ripple across the card surface.
#[derive(Clone, Copy)]
pub struct AuroraCurtain {
    pub freq1: f32,             // wave frequency layer 1 (3..15)
    pub freq2: f32,             // wave frequency layer 2
    pub curtain_sharpness: f32, // power exponent for curtain bands (2..8)
    pub vertical_falloff: f32,  // how quickly aurora fades top to bottom (0.5..3)
    pub brightness: f32,        // overall intensity
}

impl CardEffect for AuroraCurtain {
    fn name(&self) -> &str {
        "Aurora"
    }

    fn compute_colors(
        &self,
        vertices: &[EffectVertex],
        mouse_u: f32,
        mouse_v: f32,
    ) -> Vec<Color32> {
        let phase = mouse_u * TAU + mouse_v * 2.0;

        vertices
            .iter()
            .map(|v| {
                let u = v.norm_u;
                let vert = v.norm_v;

                let wave1 = (vert * self.freq1 + u * 2.0 + phase).sin() * 0.5 + 0.5;
                let wave2 = (vert * self.freq2 - u * 1.5 + phase * 0.7 + 1.3).sin() * 0.5 + 0.5;
                let wave3 = (vert * (self.freq1 + self.freq2) * 0.5 + u * 3.0 + phase * 1.3 + 2.7)
                    .sin()
                    * 0.5
                    + 0.5;

                let curtain1 = wave1.powf(self.curtain_sharpness);
                let curtain2 = wave2.powf(self.curtain_sharpness * 1.2);
                let curtain3 = wave3.powf(self.curtain_sharpness * 0.8);

                let vertical_fade = (1.0 - vert).powf(self.vertical_falloff);

                let t = (wave1 + wave2 * 0.5) / 1.5;
                let (pr, pg, pb) = if t < 0.33 {
                    let f = t / 0.33;
                    (
                        0.1 * (1.0 - f) + 0.1 * f,
                        0.9 * (1.0 - f) + 0.6 * f,
                        0.4 * (1.0 - f) + 0.8 * f,
                    )
                } else if t < 0.66 {
                    let f = (t - 0.33) / 0.33;
                    (
                        0.1 * (1.0 - f) + 0.6 * f,
                        0.6 * (1.0 - f) + 0.1 * f,
                        0.8 * (1.0 - f) + 0.8 * f,
                    )
                } else {
                    let f = (t - 0.66) / 0.34;
                    (
                        0.6 * (1.0 - f) + 0.9 * f,
                        0.1 * (1.0 - f) + 0.2 * f,
                        0.8 * (1.0 - f) + 0.5 * f,
                    )
                };

                let intensity =
                    (curtain1 + curtain2 * 0.7 + curtain3 * 0.5) * vertical_fade * self.brightness;
                let intensity = intensity.clamp(0.0, 1.0);

                let r = (pr * intensity * 255.0).min(255.0) as u8;
                let g = (pg * intensity * 255.0).min(255.0) as u8;
                let b = (pb * intensity * 255.0).min(255.0) as u8;
                let alpha = (intensity * 0.6 * 255.0).min(255.0) as u8;
                Color32::from_rgba_premultiplied(r, g, b, alpha)
            })
            .collect()
    }
}

// ============================================================================
// 7. Prismatic Dispersion
// ============================================================================

/// Prismatic dispersion — separated RGB channels + crystal facets,
/// simulating light through a prism or cut diamond.
#[derive(Clone, Copy)]
pub struct PrismaticDispersion {
    pub dispersion: f32,  // how much RGB channels separate (0.01..0.15)
    pub spread: f32,      // offset distance multiplier (0.02..0.2)
    pub facet_scale: f32, // voronoi cell count (4..20)
    pub intensity: f32,   // overall brightness
}

impl PrismaticDispersion {
    /// Voronoi-based facet brightness at a UV position.
    fn facet_brightness(&self, px: f32, py: f32) -> f32 {
        let sx = px * self.facet_scale;
        let sy = py * self.facet_scale;
        let cell_x = sx.floor();
        let cell_y = sy.floor();
        let local_x = sx.fract();
        let local_y = sy.fract();

        let mut min_dist = 8.0_f32;
        for j in -1..=1_i32 {
            for i in -1..=1_i32 {
                let (rx, ry) = hash2(cell_x + i as f32, cell_y + j as f32);
                let diff_x = i as f32 + rx - local_x;
                let diff_y = j as f32 + ry - local_y;
                let d = diff_x * diff_x + diff_y * diff_y;
                min_dist = min_dist.min(d);
            }
        }
        let dist = min_dist.sqrt();
        let center_bright = ((0.3 - dist) / 0.3).clamp(0.0, 1.0);
        let edge_bright = ((dist - 0.02) / 0.06).clamp(0.0, 1.0);
        center_bright * 0.6 + (1.0 - edge_bright) * 0.4
    }
}

impl CardEffect for PrismaticDispersion {
    fn name(&self) -> &str {
        "Prismatic"
    }

    fn compute_colors(
        &self,
        vertices: &[EffectVertex],
        mouse_u: f32,
        mouse_v: f32,
    ) -> Vec<Color32> {
        vertices
            .iter()
            .map(|v| {
                let dx = mouse_u - v.norm_u;
                let dy = mouse_v - v.norm_v;
                let dist = (dx * dx + dy * dy).sqrt().max(0.001);
                let dir_x = dx / dist;
                let dir_y = dy / dist;

                let off_r = dist * (1.0 + self.dispersion * 0.0) * self.spread;
                let off_g = dist * (1.0 + self.dispersion * 0.5) * self.spread;
                let off_b = dist * (1.0 + self.dispersion * 1.0) * self.spread;

                let r = self.facet_brightness(v.norm_u + dir_x * off_r, v.norm_v + dir_y * off_r);
                let g = self.facet_brightness(v.norm_u + dir_x * off_g, v.norm_v + dir_y * off_g);
                let b = self.facet_brightness(v.norm_u + dir_x * off_b, v.norm_v + dir_y * off_b);

                let fresnel = (1.0 - dist.clamp(0.0, 1.0)).powi(3);
                let ri = (r * fresnel * self.intensity).clamp(0.0, 1.0);
                let gi = (g * fresnel * self.intensity).clamp(0.0, 1.0);
                let bi = (b * fresnel * self.intensity).clamp(0.0, 1.0);
                let alpha = ((ri + gi + bi) / 3.0 * 0.7).clamp(0.0, 1.0);

                Color32::from_rgba_premultiplied(
                    (ri * 255.0) as u8,
                    (gi * 255.0) as u8,
                    (bi * 255.0) as u8,
                    (alpha * 255.0) as u8,
                )
            })
            .collect()
    }
}
