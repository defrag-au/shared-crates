//! Swell crests over the shallows, from a bake's swell layers
//! (`world_bake::LayerName::SEA`, baked by `world_baker::bake_swell`).
//!
//! A crest is where the swell's travel time is a whole number of periods,
//! less the clock: the same field a live renderer animates
//! (`docs/perps-world-map/baked-vs-live.md`). A chart draws one moment.
//! Refraction is in the field: crests bend to run along the coast and crowd
//! together as the water shoals.

use crate::colour::smoothstep;
use crate::sheet::Grid;

/// One pixel's swell, as the bake records it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Swell {
    /// Travel time from the shelf edge, radians of deep-water travel; 0 in
    /// deep water, on land, and in shallows no swell reaches.
    pub time: f32,
    /// How squarely the shore here meets the prevailing swell, 0..1.
    pub exposure: f32,
}

/// How swell crests are drawn. Starting values tuned by eye.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveStyle {
    /// Time between crests, radians of deep-water travel: about eight
    /// pixels apart at the shelf edge on a 4000-wide chart, closing up
    /// shoreward. Stylised: real swell crests are far finer than a world
    /// map can show.
    pub period: f32,
    /// Crest half-width, pixels, and strongest mix toward the surf colour.
    pub crest_half: f32,
    pub crest_strength: f32,
    /// Share of a crest's strength a fully sheltered shore keeps.
    pub sheltered: f32,
    /// Crest half-width as a share of a period, for a renderer that has the
    /// phase but not its change per pixel (the globe's shader). About
    /// `crest_half` over the crests' ~8-pixel spacing at the shelf edge.
    pub crest_phase: f32,
    /// The surf zone a live renderer pulses: how far out from the shore it
    /// reaches, pixels (about the coast's stroke and surf band), and how
    /// much harder it breaks against a fully rocky shore.
    pub surf_reach: f32,
    pub rock_surge: f32,
    /// The pulse: strongest whitening as a crest arrives, and how fast the
    /// foam fades after it, per period (`exp(−decay · since)`).
    pub surf_pulse: f32,
    pub surf_decay: f32,
}

impl Default for WaveStyle {
    fn default() -> Self {
        Self {
            period: 0.0126,
            crest_half: 0.45,
            crest_strength: 0.5,
            sheltered: 0.3,
            crest_phase: 0.075,
            surf_reach: 6.0,
            rock_surge: 0.5,
            surf_pulse: 0.6,
            surf_decay: 4.0,
        }
    }
}

/// How strongly a sea pixel `sd` pixels from the shore pulses with surf,
/// 0..1: full at the shoreline, gone by [`WaveStyle::surf_reach`], stronger
/// where the swell meets the shore squarely (`exposure`) and against rock
/// (`rock`, 0..1).
pub fn surf_zone(sd: f32, exposure: f32, rock: f32, style: &WaveStyle) -> f32 {
    if sd <= 0.0 {
        return 0.0;
    }
    let near = 1.0 - smoothstep(0.0, style.surf_reach, sd);
    let open = style.sheltered + (1.0 - style.sheltered) * exposure;
    (near * open * (1.0 + style.rock_surge * rock)).min(1.0)
}

impl WaveStyle {
    /// A swell's crest strength before coverage: faded in from the shelf
    /// edge over the first period, and weaker where the shore is sheltered.
    fn strength(&self, swell: Swell) -> f32 {
        smoothstep(0.0, self.period, swell.time)
            * (self.sheltered + (1.0 - self.sheltered) * swell.exposure)
    }
}

/// How strongly swell crests ink pixel `i` at `clock` (periods elapsed),
/// 0..1.
pub fn crest(i: usize, swell: &[Swell], grid: &Grid, clock: f32, style: &WaveStyle) -> f32 {
    let here = swell[i];
    if here.time <= 0.0 {
        return 0.0;
    }
    // The field's change across a pixel, from neighbours the swell reaches
    // (a neighbour in deep water or on land records no travel).
    let at = |dx: i32, dy: i32| {
        let t = swell[grid.clamped(i, dx, dy)].time;
        if t > 0.0 { t } else { here.time }
    };
    let gx = (at(1, 0) - at(-1, 0)) * 0.5;
    let gy = (at(0, 1) - at(0, -1)) * 0.5;
    let per_pixel = (gx * gx + gy * gy).sqrt() / style.period;
    if per_pixel < 1e-6 {
        return 0.0;
    }
    let phase = here.time / style.period - clock;
    // Pixels from the nearest crest.
    let off = (phase - phase.round()).abs() / per_pixel;
    let cover = (style.crest_half + 0.5 - off).clamp(0.0, 1.0);
    cover * style.crest_strength * style.strength(here)
}

/// One pixel's swell for a renderer that animates it, as RGBA8:
///
/// - R, G: the crest phase (`time / period`, fractional part) as a unit
///   vector `(cos, sin)` mapped to 0..1. Stored as a vector so filtering
///   between texels never tears at the wrap from one crest to the next.
/// - B: crest strength, 0..1: the exposure-weighted strength [`crest`]
///   uses, faded in from the shelf edge.
/// - A: the surf zone, 0..1 ([`surf_zone`]): where the renderer pulses
///   whitewater as each crest arrives, then lets it fade.
///
/// A renderer recovers the phase as `atan(G′, R′) / 2π` (with `R′ = 2R − 1`
/// and so on), and draws a crest where `phase − clock` is whole.
pub fn encode(swell: Swell, surf: f32, style: &WaveStyle) -> [u8; 4] {
    let unit = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    if swell.time <= 0.0 {
        return [128, 128, 0, unit(surf)];
    }
    let angle = std::f32::consts::TAU * (swell.time / style.period).fract();
    let byte = |v: f32| unit(v * 0.5 + 0.5);
    [
        byte(angle.cos()),
        byte(angle.sin()),
        unit(style.strength(swell)),
        unit(surf),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet::Sheet;

    #[test]
    fn crests_fall_where_the_travel_time_is_a_whole_number_of_periods() {
        let style = WaveStyle::default();
        let period = style.period;
        // A ramp of travel time across a row: crests every 8 pixels.
        let grid = Sheet::regional(64, 8, 0.0, 0.0, 10.0).grid();
        let swell: Vec<Swell> = (0..grid.w * grid.h)
            .map(|i| Swell {
                time: period * (1.0 + (i % grid.w) as f32 / 8.0),
                exposure: 1.0,
            })
            .collect();
        let row = 4 * grid.w;
        let on = crest(row + 24, &swell, &grid, 0.0, &style); // time = 4 periods
        let off = crest(row + 28, &swell, &grid, 0.0, &style); // half-way between
        assert!(on > 0.4 && off == 0.0, "on {on}, off {off}");
        // A quarter period later the crest has moved a quarter of the way
        // on, toward longer travel (the shore).
        let moved = crest(row + 26, &swell, &grid, 0.25, &style);
        assert!(moved > 0.4, "{moved}");
    }

    #[test]
    fn the_encoded_phase_decodes_to_the_crest_phase() {
        let style = WaveStyle::default();
        for k in [0.1f32, 0.37, 0.5, 0.92] {
            let s = Swell {
                time: style.period * (3.0 + k),
                exposure: 1.0,
            };
            let [r, g, b, _] = encode(s, 0.0, &style);
            let (x, y) = (r as f32 / 255.0 * 2.0 - 1.0, g as f32 / 255.0 * 2.0 - 1.0);
            let phase = y.atan2(x).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
            assert!((phase - k).abs() < 0.01, "{phase} vs {k}");
            assert!(b > 200);
        }
        assert_eq!(
            encode(Swell::default(), 0.0, &style)[2],
            0,
            "no swell, no strength"
        );
    }

    #[test]
    fn surf_pulses_at_the_shore_and_harder_on_open_rock() {
        let style = WaveStyle::default();
        let zone = |sd, exposure, rock| surf_zone(sd, exposure, rock, &style);
        assert_eq!(zone(-1.0, 1.0, 0.0), 0.0, "none on land");
        assert_eq!(zone(style.surf_reach + 1.0, 1.0, 0.0), 0.0, "none offshore");
        assert!(
            zone(0.5, 1.0, 0.0) > zone(0.5, 0.0, 0.0),
            "open beats sheltered"
        );
        assert!(zone(0.5, 0.5, 1.0) > zone(0.5, 0.5, 0.0), "rock beats sand");
        assert!(
            zone(0.5, 1.0, 0.0) > zone(3.0, 1.0, 0.0),
            "strongest at the shore"
        );
        // The zone rides in the sheet's alpha.
        assert_eq!(encode(Swell::default(), 1.0, &style)[3], 255);
        assert_eq!(encode(Swell::default(), 0.0, &style)[3], 0);
    }
}
