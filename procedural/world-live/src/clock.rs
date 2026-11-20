//! The world clock.
//!
//! One season is one Cardano epoch: exactly 5 real days (432,000 s). A world
//! day is 80 minutes (4,800 s), worked back from that, so a season is exactly
//! 90 world days and a year (four seasons, four epochs) 360. Every season
//! begins at the same hour of the world's day.
//!
//! The year begins at the northern spring equinox. The clock's origin is a
//! real instant (an epoch boundary read from the chain and recorded in the
//! world's config); this crate does not know it.

use serde::{Deserialize, Serialize};

/// Real seconds in a world day at speed 1.
pub const DAY_SECONDS: f64 = 4_800.0;
pub const DAYS_PER_SEASON: u32 = 90;
pub const SEASONS_PER_YEAR: u32 = 4;
pub const DAYS_PER_YEAR: u32 = DAYS_PER_SEASON * SEASONS_PER_YEAR;
/// World seconds in a season: one Cardano epoch.
pub const SEASON_SECONDS: f64 = DAY_SECONDS * DAYS_PER_SEASON as f64;
pub const YEAR_SECONDS: f64 = SEASON_SECONDS * SEASONS_PER_YEAR as f64;

/// Real time to world time.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Clock {
    /// The real instant (Unix seconds) at which world time is zero: the
    /// start of a year, so an epoch boundary.
    pub origin_unix: f64,
    /// World seconds per real second: 1 in play, larger for testing (120
    /// makes a season an hour and a day 40 seconds).
    pub speed: f64,
}

impl Clock {
    /// World seconds at a real instant (Unix seconds). Before the origin,
    /// negative.
    pub fn world_seconds(&self, now_unix: f64) -> f64 {
        (now_unix - self.origin_unix) * self.speed
    }

    /// Real seconds since the origin at a real instant, whatever the speed:
    /// what the waves and the weather run on ([`crate::weather`]).
    pub fn real_seconds(&self, now_unix: f64) -> f64 {
        now_unix - self.origin_unix
    }

    /// The moment at a real instant.
    pub fn moment(&self, now_unix: f64) -> Moment {
        Moment::at(self.world_seconds(now_unix))
    }
}

/// A world's clock as its config records it: the [`Clock`], and where its
/// origin came from. The origin is the start of a Cardano epoch read from
/// the chain; the world's first spring begins with it, and each epoch after
/// is a season.
///
/// Every mainnet epoch so far has been 432,000 s, but that is the chain's
/// to change: the clock runs on its own origin and [`SEASON_SECONDS`], and
/// `epoch` records which boundary it was set to, so a drift from the
/// chain's epochs can be seen and the origin set again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldClock {
    #[serde(flatten)]
    pub clock: Clock,
    /// The chain the origin was read from (`mainnet`).
    pub network: String,
    /// The epoch whose start is the origin.
    pub epoch: u64,
}

impl WorldClock {
    /// The epoch a real instant falls in, by this clock: one a season from
    /// the origin's, at the clock's speed.
    pub fn epoch_at(&self, now_unix: f64) -> i64 {
        self.epoch as i64 + (self.clock.world_seconds(now_unix) / SEASON_SECONDS).floor() as i64
    }
}

/// A season, as the northern hemisphere has it unless asked for a
/// hemisphere ([`Season::in_hemisphere`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    const ORDER: [Season; 4] = [
        Season::Spring,
        Season::Summer,
        Season::Autumn,
        Season::Winter,
    ];

    /// The season in the north (`north`) or the south: half a year apart.
    pub fn in_hemisphere(self, north: bool) -> Season {
        if north {
            self
        } else {
            let i = Self::ORDER.iter().position(|&s| s == self).unwrap_or(0);
            Self::ORDER[(i + 2) % 4]
        }
    }
}

/// A moment of world time, broken into the calendar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moment {
    /// World seconds since the origin.
    pub seconds: f64,
    /// Whole years since the origin (negative before it).
    pub year: i64,
    /// The day of the year, 0 to 359.
    pub day: u32,
    /// How far through the year, 0 to 1: 0 is the northern spring
    /// equinox, 0.25 the northern summer solstice.
    pub year_fraction: f32,
    /// How far through the day, 0 to 1, at longitude 0: 0.5 is noon there.
    pub time_of_day: f32,
}

impl Moment {
    pub fn at(seconds: f64) -> Moment {
        let year = (seconds / YEAR_SECONDS).floor();
        let in_year = seconds - year * YEAR_SECONDS;
        let day = ((in_year / DAY_SECONDS).floor() as u32).min(DAYS_PER_YEAR - 1);
        let in_day = in_year - f64::from(day) * DAY_SECONDS;
        Moment {
            seconds,
            year: year as i64,
            day,
            year_fraction: (in_year / YEAR_SECONDS) as f32,
            time_of_day: (in_day / DAY_SECONDS) as f32,
        }
    }

    /// The northern hemisphere's season.
    pub fn season(&self) -> Season {
        Season::ORDER[(self.day / DAYS_PER_SEASON) as usize]
    }

    /// The day of the season, 0 to 89.
    pub fn day_of_season(&self) -> u32 {
        self.day % DAYS_PER_SEASON
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_season_is_one_cardano_epoch_of_ninety_eighty_minute_days() {
        assert_eq!(SEASON_SECONDS, 432_000.0);
        assert_eq!(DAY_SECONDS, 80.0 * 60.0);
        assert_eq!(DAYS_PER_YEAR, 360);
        assert_eq!(YEAR_SECONDS, 20.0 * 86_400.0);
    }

    #[test]
    fn the_origin_is_the_first_moment_of_a_northern_spring() {
        let m = Moment::at(0.0);
        assert_eq!((m.year, m.day, m.time_of_day), (0, 0, 0.0));
        assert_eq!(m.season(), Season::Spring);
        assert_eq!(m.season().in_hemisphere(false), Season::Autumn);
    }

    #[test]
    fn each_epoch_turns_the_season_at_the_same_hour() {
        for (i, season) in Season::ORDER.into_iter().enumerate() {
            let m = Moment::at(SEASON_SECONDS * i as f64 + 1.0);
            assert_eq!(m.season(), season);
            assert_eq!(m.day_of_season(), 0);
            assert!(m.time_of_day < 1e-3);
        }
        let next_year = Moment::at(YEAR_SECONDS + DAY_SECONDS * 1.5);
        assert_eq!((next_year.year, next_year.day), (1, 1));
        assert!((next_year.time_of_day - 0.5).abs() < 1e-6);
    }

    #[test]
    fn at_testing_speed_a_real_hour_is_a_season() {
        let clock = Clock {
            origin_unix: 1_000.0,
            speed: 120.0,
        };
        let m = clock.moment(1_000.0 + 3_600.0);
        assert_eq!(m.season(), Season::Summer);
        assert_eq!(m.day_of_season(), 0);
        let day = clock.moment(1_000.0 + 40.0);
        assert_eq!(day.day, 1);
    }

    #[test]
    fn a_world_clock_set_to_an_epoch_counts_a_season_an_epoch() {
        // Mainnet epoch 668, as the chain gave it (Koios /epoch_info,
        // 2026-11-20): it began at 1794779091 and ended 432,000 s on.
        let world = WorldClock {
            clock: Clock {
                origin_unix: 1_794_779_091.0,
                speed: 1.0,
            },
            network: "mainnet".to_string(),
            epoch: 668,
        };
        let end_of_668 = 1_795_211_091.0;
        assert_eq!(world.epoch_at(end_of_668 - 1.0), 668);
        assert_eq!(world.epoch_at(end_of_668), 669);
        assert_eq!(
            world.clock.moment(end_of_668 - 1.0).season(),
            Season::Spring
        );
        assert_eq!(world.clock.moment(end_of_668).season(), Season::Summer);
        // Mid-epoch: day 45 of the season's 90.
        let mid = world.clock.moment(1_794_779_091.0 + SEASON_SECONDS / 2.0);
        assert_eq!(mid.day_of_season(), 45);
    }

    #[test]
    fn before_the_origin_the_calendar_runs_backwards_into_last_year() {
        let m = Moment::at(-DAY_SECONDS * 0.25);
        assert_eq!((m.year, m.day), (-1, DAYS_PER_YEAR - 1));
        assert!((m.time_of_day - 0.75).abs() < 1e-6);
        assert_eq!(m.season(), Season::Winter);
    }
}
