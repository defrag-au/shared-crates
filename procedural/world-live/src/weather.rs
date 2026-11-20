//! The clocks of what moves fast: the waves and the clouds. Functions of
//! time alone, so every client that agrees on the time draws the same crest
//! on the same shore and the same cloud over the same hill, with nothing
//! exchanged.
//!
//! Both take seconds of *real* time since the world clock's origin, whatever
//! the clock's speed: a test speed hurries the calendar (a season an hour),
//! and waves and weather hurried as much would be a blur.
//!
//! Each count wraps at [`WRAP`], a whole number, so a renderer can hold it
//! in a 32-bit float (a shader's uniform) without losing its fraction, and
//! what is keyed on its whole part (a cloud cycle's seed) agrees everywhere.

/// Seconds between one wave crest and the next.
pub const WAVE_SECONDS: f64 = 2.0;
/// Seconds a cloud cycle lasts: a system forms, drifts with its latitude's
/// wind and dissolves. Five minutes, sixteen to a world day: slow enough to
/// be weather, quick enough to see it move from orbit.
pub const CLOUD_CYCLE_SECONDS: f64 = 300.0;
/// Where the counts wrap.
pub const WRAP: f64 = 1024.0;

/// Wave periods elapsed, in 0..[`WRAP`]: a crest stands where the swell's
/// phase less this is whole.
pub fn wave_periods(seconds: f64) -> f32 {
    (seconds / WAVE_SECONDS).rem_euclid(WRAP) as f32
}

/// Cloud cycles elapsed, in 0..[`WRAP`].
pub fn cloud_cycles(seconds: f64) -> f32 {
    (seconds / CLOUD_CYCLE_SECONDS).rem_euclid(WRAP) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_counts_run_at_their_periods_and_agree_across_a_wrap() {
        assert_eq!(wave_periods(0.0), 0.0);
        assert_eq!(wave_periods(3.0), 1.5);
        assert_eq!(cloud_cycles(450.0), 1.5);
        // A whole wrap on, the same count: nothing jumps but the number.
        let later = 366_819.0;
        let wrap = WRAP * CLOUD_CYCLE_SECONDS;
        assert!((cloud_cycles(later) - cloud_cycles(later + wrap)).abs() < 1e-3);
        assert!((wave_periods(later) - wave_periods(later + WRAP * WAVE_SECONDS)).abs() < 1e-3);
        // Before the origin they still run forward, inside the range.
        assert!((0.0..WRAP as f32).contains(&wave_periods(-1.0)));
        assert!(wave_periods(-1.0) > wave_periods(-2.0));
    }

    #[test]
    fn two_years_on_a_count_keeps_its_fraction() {
        // Seconds since an origin two world years back: far past what a
        // 32-bit float holds a fraction of, were it not wrapped.
        let seconds = 2.0 * crate::clock::YEAR_SECONDS + 0.5;
        let fraction = wave_periods(seconds).fract();
        assert!((fraction - 0.25).abs() < 1e-3, "{fraction}");
    }
}
