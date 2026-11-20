//! The sun: where it stands at a moment, and how much light a latitude takes
//! over a day. A circular orbit and a fixed axial tilt; the planet turns
//! eastward (the way longitude grows), so the sun crosses westward.

use std::f32::consts::{PI, TAU};

use crate::clock::Moment;

/// The planet's orbit, as far as the sun's path needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orbit {
    /// Axial tilt, radians: how far the sun swings north and south over the
    /// year. It sets how strong the seasons are.
    pub axial_tilt: f32,
}

impl Default for Orbit {
    /// Earth's tilt, 23.44°: seasons a player already knows.
    fn default() -> Self {
        Orbit {
            axial_tilt: 23.44_f32.to_radians(),
        }
    }
}

impl Orbit {
    /// The sun's latitude at a point in the year (0 the northern spring
    /// equinox, 0.25 its summer solstice), radians.
    pub fn declination(&self, year_fraction: f32) -> f32 {
        (self.axial_tilt.sin() * (TAU * year_fraction).sin()).asin()
    }

    /// The direction from the planet's centre to the sun, in the planet's
    /// own (turning) frame.
    pub fn sun(&self, moment: &Moment) -> [f32; 3] {
        let lat = self.declination(moment.year_fraction);
        let lon = subsolar_longitude(moment.time_of_day);
        let (sl, cl) = lat.sin_cos();
        [cl * lon.cos(), sl, cl * lon.sin()]
    }

    /// The sunlight a latitude (radians) takes, averaged over a whole day,
    /// as a share of the full sun overhead all day: 0 in polar night,
    /// 1/π at the equator on an equinox.
    pub fn daily_light(&self, latitude: f32, year_fraction: f32) -> f32 {
        daily_light(latitude, self.declination(year_fraction))
    }
}

/// The longitude under the sun, radians in [-π, π): noon at longitude 0 is
/// time of day 0.5, and the sun moves west as the day goes on.
pub fn subsolar_longitude(time_of_day: f32) -> f32 {
    (PI - TAU * time_of_day + PI).rem_euclid(TAU) - PI
}

/// How high the sun stands over a direction: the cosine of its angle from
/// the zenith, negative when it is below the horizon.
pub fn elevation(dir: [f32; 3], sun: [f32; 3]) -> f32 {
    dir[0] * sun[0] + dir[1] * sun[1] + dir[2] * sun[2]
}

/// Daily mean insolation at a latitude for a declination, as a share of the
/// full sun all day: `(h₀ sin φ sin δ + cos φ cos δ sin h₀) / π`, where h₀ is
/// the hour angle of sunset.
pub fn daily_light(latitude: f32, declination: f32) -> f32 {
    let cos_h0 = (-latitude.tan() * declination.tan()).clamp(-1.0, 1.0);
    let h0 = cos_h0.acos();
    let light =
        h0 * latitude.sin() * declination.sin() + latitude.cos() * declination.cos() * h0.sin();
    (light / PI).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{DAY_SECONDS, YEAR_SECONDS};

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn at_noon_on_the_equinox_the_sun_is_over_longitude_zero_on_the_equator() {
        // The equinox itself falls at midnight on day 0; noon that day is
        // half a day on and the sun is already 0.2° north. So the moment is
        // built at the equinox with noon's time of day.
        let noon = Moment {
            year_fraction: 0.0,
            ..Moment::at(DAY_SECONDS * 0.5)
        };
        let sun = Orbit::default().sun(&noon);
        assert!(near(sun[0], 1.0) && near(sun[1], 0.0) && near(sun[2], 0.0));
    }

    #[test]
    fn the_sun_moves_west_through_the_day() {
        let morning = subsolar_longitude(0.4);
        let afternoon = subsolar_longitude(0.6);
        assert!(morning > 0.0 && afternoon < 0.0);
        assert!(near(subsolar_longitude(0.0), -PI) || near(subsolar_longitude(0.0), PI));
    }

    #[test]
    fn the_sun_reaches_the_tilt_at_the_northern_solstice_and_its_negative_half_a_year_on() {
        let orbit = Orbit::default();
        assert!(near(orbit.declination(0.25), orbit.axial_tilt));
        assert!(near(orbit.declination(0.75), -orbit.axial_tilt));
        let solstice = Moment::at(YEAR_SECONDS * 0.25 + DAY_SECONDS * 0.5);
        assert!(near(orbit.sun(&solstice)[1], orbit.axial_tilt.sin()));
    }

    #[test]
    fn a_pole_is_dark_all_winter_and_outshines_the_equator_at_midsummer() {
        let orbit = Orbit::default();
        let pole = std::f32::consts::FRAC_PI_2 - 1e-4;
        assert_eq!(orbit.daily_light(pole, 0.75), 0.0);
        assert!(orbit.daily_light(pole, 0.25) > orbit.daily_light(0.0, 0.25));
        assert!(near(orbit.daily_light(0.0, 0.0), 1.0 / PI));
    }

    #[test]
    fn the_hemispheres_trade_light_over_the_year() {
        let orbit = Orbit::default();
        let mid = 45_f32.to_radians();
        assert!(orbit.daily_light(mid, 0.25) > orbit.daily_light(-mid, 0.25));
        assert!(orbit.daily_light(-mid, 0.75) > orbit.daily_light(mid, 0.75));
        assert!(near(
            orbit.daily_light(mid, 0.0),
            orbit.daily_light(-mid, 0.0)
        ));
    }

    #[test]
    fn the_sun_is_up_over_its_own_point_and_down_on_the_far_side() {
        let sun = Orbit::default().sun(&Moment::at(DAY_SECONDS * 0.3));
        assert!(near(elevation(sun, sun), 1.0));
        let far = [-sun[0], -sun[1], -sun[2]];
        assert!(near(elevation(far, sun), -1.0));
    }
}
