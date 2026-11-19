//! Relief light and colour for the cel style: the land's shading done the
//! way relief cartographers do it, then cut into cel bands.
//!
//! From `reports/Painterly terrain map techniques.md`:
//!
//! - **Generalise before shading.** The light reads a blend of the land form
//!   and a broader blur of it (Patterson's "resolution bumping"), so the big
//!   forms carry the light and texel-scale texture does not.
//! - **Soft, multi-directional light.** A light from the upper left with
//!   fill from the west and north, each weighted by the slope's aspect
//!   (Mark's multidirectional oblique weighting, 1992), so no ridge has both
//!   flanks the same tone.
//! - **Warm lights, cool shadows** (Imhof): the shadow band leans violet, the
//!   light band warm, and a fourth, deeper shadow band takes the steepest
//!   shaded slopes. Flat ground stays in the middle band, untouched.
//! - **Aerial perspective** (Imhof): contrast grows with height. Lowland
//!   bands are wide (calm), peaks' narrow (crisp).
//! - **Bands cut last**, with their edges blended over a narrow range of
//!   light so they do not step.
//!
//! The mixes and thresholds are starting values tuned by eye, not sourced.

use glam::Vec3;

use crate::colour::{Colour, mix, rgb, smoothstep};
use crate::palette::Tones;
use crate::sheet::{Grid, box_blur};

/// How relief is lit and painted. The values are starting points tuned by
/// eye, not sourced.
#[derive(Clone, Debug, PartialEq)]
pub struct ReliefStyle {
    /// The lights, toward each on the sheet (x east, y down), at 45°
    /// elevation, with each one's share before Mark's aspect weighting.
    pub lights: Vec<Light>,
    /// The warm tint the light band leans to, and how far.
    pub warm: Colour,
    pub warm_mix: f32,
    /// The cool tint the shadow band leans to, and how far.
    pub cool: Colour,
    pub cool_mix: f32,
    /// The deep shadow band: the shadow, darkened and cooled further, from
    /// `deep_at` middle-band half-widths below flat.
    pub deep_darken: f32,
    pub deep_mix: f32,
    pub deep_at: f32,
    /// Half-range of light over which a band edge blends.
    pub edge: f32,
    /// How much of the shading height is the broad blur, and the blur's
    /// radius as a share of the sheet's width.
    pub broad_share: f32,
    pub broad_radius: f32,
    /// Band contrast by surface: gentle ground, ice sheets (broad light
    /// only: the generator's grain shows as streaks under tight bands), and
    /// cliffs (steep, so most face well toward or away from the light; a
    /// wide middle band keeps them from going all shadow).
    pub ground: Contrast,
    pub ice: Contrast,
    pub cliffs: Contrast,
    /// For a renderer that lights an unshaded sheet (only each surface's
    /// mid tone, as the globe's): how far its light and shadow tones sit
    /// from the mid, as gains on it. About the palettes' own ratios (the
    /// green painting's tops run ~1.3× and ~0.7× their mid); the chart
    /// reads the palettes' tones directly instead.
    pub light_gain: f32,
    pub shadow_gain: f32,
}

/// One light: toward it on the sheet, and its share.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub toward: (f32, f32),
    pub share: f32,
}

/// How a surface cuts its bands: `calm` is the middle band's half-width in
/// the lowlands, narrowing by `crisp` toward the peaks (Imhof's aerial
/// perspective: contrast grows with height).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contrast {
    pub calm: f32,
    pub crisp: f32,
}

impl Default for ReliefStyle {
    fn default() -> Self {
        Self {
            // Upper left, with fill from the left and from above.
            lights: vec![
                Light {
                    toward: (-1.0, -1.0),
                    share: 1.0,
                },
                Light {
                    toward: (-1.0, 0.0),
                    share: 0.45,
                },
                Light {
                    toward: (0.0, -1.0),
                    share: 0.45,
                },
            ],
            warm: rgb(255, 226, 158),
            warm_mix: 0.2,
            cool: rgb(84, 66, 150),
            cool_mix: 0.18,
            deep_darken: 0.15,
            deep_mix: 0.28,
            deep_at: 3.2,
            edge: 0.02,
            broad_share: 0.55,
            broad_radius: 1.0 / 800.0,
            ground: Contrast {
                calm: 0.12,
                crisp: 0.07,
            },
            ice: Contrast {
                calm: 0.28,
                crisp: 0.08,
            },
            cliffs: Contrast {
                calm: 0.22,
                crisp: 0.0,
            },
            light_gain: 1.3,
            shadow_gain: 0.7,
        }
    }
}

/// The height the light reads: `form` (per cell) blended with a broad blur
/// of itself.
pub fn shading_height(form: &[f32], grid: &Grid, style: &ReliefStyle) -> Vec<f32> {
    let radius = ((grid.w as f32 * style.broad_radius).round() as i32).max(1);
    let broad = box_blur(&box_blur(form, grid, radius), grid, radius);
    form.iter()
        .zip(&broad)
        .map(|(&f, &b)| f + (b - f) * style.broad_share)
        .collect()
}

/// Light on ground sloping by `(gx, gy)` (rise per unit east and per unit
/// down the sheet, in the same units as height), relative to flat ground:
/// 1 on the flat, above 1 facing the light, below facing away.
pub fn light(gx: f32, gy: f32, style: &ReliefStyle) -> f32 {
    let n = Vec3::new(-gx, -gy, 1.0).normalize();
    let slope = (gx * gx + gy * gy).sqrt();
    let flat = std::f32::consts::FRAC_1_SQRT_2; // each light at 45° elevation
    let (mut total, mut weights) = (0.0, 0.0);
    for &Light {
        toward: (lx, ly),
        share,
    } in &style.lights
    {
        let h = (lx * lx + ly * ly).sqrt();
        let l = Vec3::new(lx / h * flat, ly / h * flat, flat);
        // Mark's weighting: a light square to the slope's facing counts
        // most, one along it least; on the flat (no aspect) they are equal.
        let along = if slope > 1e-6 {
            ((-gx * lx - gy * ly) / (slope * h)).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let w = share * (0.25 + (1.0 - along * along));
        total += w * n.dot(l).max(0.0) / flat;
        weights += w;
    }
    total / weights
}

/// A surface's colour from its cel set (shadow, mid, light), the light on it
/// ([`light`]), its height as a share of the land's relief (0 at the coast,
/// 1 at the peaks), and how it cuts its bands.
pub fn paint(
    set: &Tones,
    lit: f32,
    height: f32,
    contrast: Contrast,
    style: &ReliefStyle,
) -> Colour {
    let k = (contrast.calm - contrast.crisp * height.clamp(0.0, 1.0).sqrt()).max(0.02);
    let (low, high, deepest) = (1.0 - k, 1.0 + k, 1.0 - style.deep_at * k);
    let edge = style.edge;
    let shadow = mix(set.shadow, style.cool, style.cool_mix);
    let lit_band = mix(set.light, style.warm, style.warm_mix);
    let deep = mix(
        mix(shadow, [0.0; 3], style.deep_darken),
        style.cool,
        style.deep_mix,
    );
    let mut colour = set.mid;
    colour = mix(colour, lit_band, smoothstep(high - edge, high + edge, lit));
    colour = mix(
        colour,
        shadow,
        1.0 - smoothstep(low - edge, low + edge, lit),
    );
    mix(
        colour,
        deep,
        1.0 - smoothstep(deepest - edge, deepest + edge, lit),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contrast these tests were written with.
    const CONTRAST: Contrast = Contrast {
        calm: 0.12,
        crisp: 0.08,
    };

    #[test]
    fn flat_ground_takes_the_middle_band_untouched() {
        let style = ReliefStyle::default();
        let light = |gx, gy| light(gx, gy, &style);
        assert!((light(0.0, 0.0) - 1.0).abs() < 1e-5);
        let set = Tones::new(rgb(10, 10, 10), rgb(100, 120, 80), rgb(200, 200, 200));
        assert_eq!(paint(&set, light(0.0, 0.0), 0.5, CONTRAST, &style), set.mid);
    }

    #[test]
    fn slopes_facing_the_light_are_lighter_than_their_opposites() {
        let style = ReliefStyle::default();
        let light = |gx, gy| light(gx, gy, &style);
        // Facing up-left (rising toward the lower right).
        let toward = light(0.6, 0.6);
        let away = light(-0.6, -0.6);
        assert!(toward > 1.0 && away < 1.0, "{toward} {away}");
        // Both flanks of a north-south ridge differ under the soft light.
        assert!((light(0.6, 0.0) - light(-0.6, 0.0)).abs() > 0.1);
    }

    #[test]
    fn peaks_cut_more_contrast_than_lowlands() {
        let style = ReliefStyle::default();
        let set = Tones::new(rgb(0, 0, 0), rgb(128, 128, 128), rgb(255, 255, 255));
        // A light just past the lowland band's edge reads as mid in the
        // lowlands but as light on a peak.
        let lit = 1.09;
        assert_eq!(paint(&set, lit, 0.0, CONTRAST, &style), set.mid);
        assert_ne!(paint(&set, lit, 1.0, CONTRAST, &style), set.mid);
    }
}
