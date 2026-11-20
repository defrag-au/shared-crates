//! The shape of the year at a place, and the weather's means at a moment.
//!
//! A bake stores three numbers a place keeps all year ([`Seasonal`]): how far
//! its temperature swings, how late its summer peaks, and how strongly its
//! rain comes in one season. [`SeasonModel::derive`] makes them from the
//! latitude and how far inland the place is, by rule (the sun's light over
//! the year, damped by the sea; a rain belt that follows the sun). A climate
//! simulation run through the year could replace the rule later and write
//! the same numbers; nothing that reads them would change.
//!
//! [`temperature`] and [`rain`] turn them, with the annual means, into a
//! moment's values: a few operations, cheap enough for a shader.

use std::f32::consts::TAU;

use crate::sun::Orbit;

/// The three numbers of a place's year, as a bake stores them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seasonal {
    /// Kelvin either side of the annual mean.
    pub swing: f32,
    /// Share of a year from the local summer solstice to the warmest day.
    pub lag: f32,
    /// The rain's amplitude about its annual mean, 0 (even) to 1.
    pub rain_season: f32,
}

/// The rules [`Seasonal`] is derived by. Every number is a starting value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeasonModel {
    pub orbit: Orbit,
    /// Kelvin of swing per unit of the swing in daily light (summer's less
    /// winter's, halved, as a share of full sun all day), deep inland.
    /// 160 gives about ±27 K at 60° inside a continent (Earth's Siberia
    /// swings about ±30 K) and ±16 K at 30°.
    pub swing_per_light: f32,
    /// The share of the inland swing the open sea keeps: water stores heat
    /// and evens out the year.
    pub sea_damping: f32,
    /// The warmest day's lag after the solstice, inland and at sea.
    pub land_lag: f32,
    pub sea_lag: f32,
    /// The rain belt: no wet season within `rain_inner` degrees of the
    /// equator (rain all year), full from there to `rain_full`, gone by
    /// `rain_outer`. And its strength at full, inland.
    pub rain_inner: f32,
    pub rain_full: (f32, f32),
    pub rain_outer: f32,
    pub rain_strength: f32,
    /// The wettest day's lag after the solstice.
    pub rain_lag: f32,
}

impl Default for SeasonModel {
    fn default() -> Self {
        SeasonModel {
            orbit: Orbit::default(),
            swing_per_light: 160.0,
            sea_damping: 0.2,
            land_lag: 0.08,
            sea_lag: 0.15,
            rain_inner: 4.0,
            rain_full: (12.0, 22.0),
            rain_outer: 35.0,
            rain_strength: 0.8,
            rain_lag: 0.06,
        }
    }
}

impl SeasonModel {
    /// A place's year from its latitude (radians) and its continentality: 0
    /// on the open sea, 1 deep inside a continent (the share of land around
    /// it, say).
    pub fn derive(&self, latitude: f32, continentality: f32) -> Seasonal {
        let inland = continentality.clamp(0.0, 1.0);
        let summer = self.orbit.daily_light(latitude.abs(), 0.25);
        let winter = self.orbit.daily_light(latitude.abs(), 0.75);
        let damping = self.sea_damping + (1.0 - self.sea_damping) * inland;
        let swing = self.swing_per_light * (summer - winter) * 0.5 * damping;
        let lag = self.sea_lag + (self.land_lag - self.sea_lag) * inland;

        let degrees = latitude.abs().to_degrees();
        let belt = smoothstep(self.rain_inner, self.rain_full.0, degrees)
            * (1.0 - smoothstep(self.rain_full.1, self.rain_outer, degrees));
        let rain_season = self.rain_strength * belt * (0.6 + 0.4 * inland);
        Seasonal {
            swing,
            lag,
            rain_season,
        }
    }

    /// The rain at a moment, as [`rain`] with this model's lag.
    pub fn rain(&self, annual: f32, rain_season: f32, latitude: f32, year_fraction: f32) -> f32 {
        rain(annual, rain_season, self.rain_lag, latitude, year_fraction)
    }
}

/// Which way a latitude's year runs: 1 in the north, −1 in the south,
/// eased through the equator, where the swing is near nothing anyway.
fn hemisphere(latitude: f32) -> f32 {
    (latitude.to_degrees() / 5.0).clamp(-1.0, 1.0)
}

/// How far into its local summer a place is, −1 (midwinter) to 1
/// (midsummer), a `lag` (share of a year) after the solstice.
fn summer(latitude: f32, lag: f32, year_fraction: f32) -> f32 {
    hemisphere(latitude) * (TAU * (year_fraction - 0.25 - lag)).cos()
}

/// The temperature (kelvin) at a moment of the year.
pub fn temperature(mean: f32, swing: f32, lag: f32, latitude: f32, year_fraction: f32) -> f32 {
    mean + swing * summer(latitude, lag, year_fraction)
}

/// The rain at a moment of the year, in the annual mean's units: wettest a
/// `lag` after the local summer solstice. Never below 0.
pub fn rain(annual: f32, rain_season: f32, lag: f32, latitude: f32, year_fraction: f32) -> f32 {
    let season = rain_season * hemisphere(latitude).abs();
    let toward = (TAU * (year_fraction - 0.25 - lag)).cos() * hemisphere(latitude).signum();
    (annual * (1.0 + season * toward)).max(0.0)
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lat(degrees: f32) -> f32 {
        degrees.to_radians()
    }

    #[test]
    fn deep_inland_at_high_latitude_swings_most_and_the_sea_least() {
        let model = SeasonModel::default();
        let siberia = model.derive(lat(60.0), 1.0);
        let ocean = model.derive(lat(60.0), 0.0);
        let equator = model.derive(0.0, 1.0);
        assert!(
            (22.0..32.0).contains(&siberia.swing),
            "inland 60° swings ±{:.1} K",
            siberia.swing
        );
        assert!(ocean.swing < siberia.swing * 0.3);
        assert!(equator.swing < 1.0);
        assert!(ocean.lag > siberia.lag);
    }

    #[test]
    fn the_hemispheres_have_opposite_summers() {
        let model = SeasonModel::default();
        let north = model.derive(lat(45.0), 0.8);
        let south = model.derive(lat(-45.0), 0.8);
        assert_eq!(north.swing, south.swing);
        let peak = 0.25 + north.lag;
        let t = |s: &Seasonal, l, f| temperature(280.0, s.swing, s.lag, lat(l), f);
        assert!(t(&north, 45.0, peak) > t(&south, -45.0, peak));
        assert!((t(&north, 45.0, peak) - (280.0 + north.swing)).abs() < 1e-3);
        assert!((t(&south, -45.0, peak + 0.5) - (280.0 + south.swing)).abs() < 1e-3);
    }

    #[test]
    fn a_year_averages_back_to_the_annual_mean() {
        let model = SeasonModel::default();
        for degrees in [-50.0, -15.0, 0.0, 15.0, 70.0] {
            let s = model.derive(lat(degrees), 0.7);
            let n = 360;
            let (mut warm, mut wet) = (0.0, 0.0);
            for i in 0..n {
                let f = i as f32 / n as f32;
                warm += temperature(270.0, s.swing, s.lag, lat(degrees), f);
                wet += model.rain(0.5, s.rain_season, lat(degrees), f);
            }
            assert!((warm / n as f32 - 270.0).abs() < 1e-2, "{degrees}°");
            assert!((wet / n as f32 - 0.5).abs() < 1e-3, "{degrees}°");
        }
    }

    #[test]
    fn the_tropics_have_a_wet_season_and_the_equator_and_mid_latitudes_do_not() {
        let model = SeasonModel::default();
        let savanna = model.derive(lat(15.0), 1.0);
        assert!(savanna.rain_season > 0.6);
        assert!(model.derive(lat(1.0), 1.0).rain_season < 0.05);
        assert!(model.derive(lat(50.0), 1.0).rain_season < 0.05);
        // Wet in local summer, dry in winter.
        let wet = model.rain(1.0, savanna.rain_season, lat(15.0), 0.25 + model.rain_lag);
        let dry = model.rain(1.0, savanna.rain_season, lat(15.0), 0.75 + model.rain_lag);
        assert!(wet > 1.5 && dry < 0.5, "wet {wet} dry {dry}");
        let south = model.rain(1.0, savanna.rain_season, lat(-15.0), 0.75 + model.rain_lag);
        assert!(south > 1.5);
    }
}
