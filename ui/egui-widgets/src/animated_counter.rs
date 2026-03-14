//! AnimatedCounter — smoothly interpolates a numeric value between snapshots.
//!
//! Given a last-known value, a rate of change, and a timestamp, the counter
//! simulates continuous incrementing between actual data refreshes. This makes
//! dashboards feel alive even when snapshots only arrive periodically.

/// An animated counter that interpolates between data snapshots.
///
/// Feed it a snapshot value and rate, and it will produce a smoothly
/// incrementing display value on each frame.
pub struct AnimatedCounter {
    /// The last confirmed snapshot value.
    snapshot_value: f64,
    /// The timestamp (seconds since epoch) when the snapshot was taken.
    snapshot_time: f64,
    /// Rate of change per second (e.g. tokens_per_hour / 3600).
    rate_per_second: f64,
    /// Optional ceiling — counter won't exceed this.
    ceiling: Option<f64>,
    /// Number of decimal places to display.
    decimals: u8,
}

impl AnimatedCounter {
    /// Create a new counter from a snapshot.
    ///
    /// - `value`: the confirmed value at snapshot time
    /// - `snapshot_time`: epoch seconds when value was captured
    /// - `rate_per_second`: how fast the value increases per second
    pub fn new(value: f64, snapshot_time: f64, rate_per_second: f64) -> Self {
        Self {
            snapshot_value: value,
            snapshot_time,
            rate_per_second,
            ceiling: None,
            decimals: 2,
        }
    }

    /// Set a ceiling the counter won't exceed.
    pub fn ceiling(mut self, max: f64) -> Self {
        self.ceiling = Some(max);
        self
    }

    /// Set the number of decimal places.
    pub fn decimals(mut self, decimals: u8) -> Self {
        self.decimals = decimals;
        self
    }

    /// Update with a new snapshot (e.g. after a data refresh).
    pub fn update_snapshot(&mut self, value: f64, time: f64, rate_per_second: f64) {
        self.snapshot_value = value;
        self.snapshot_time = time;
        self.rate_per_second = rate_per_second;
    }

    /// Get the interpolated value at the given time.
    pub fn value_at(&self, now: f64) -> f64 {
        let elapsed = (now - self.snapshot_time).max(0.0);
        let projected = self.snapshot_value + elapsed * self.rate_per_second;
        if let Some(ceil) = self.ceiling {
            projected.min(ceil)
        } else {
            projected
        }
    }

    /// Format the interpolated value as a display string at the given time.
    pub fn display_at(&self, now: f64) -> String {
        let val = self.value_at(now);
        format_with_commas(val, self.decimals)
    }

    /// Get the current interpolated value using wall-clock time (WASM-compatible).
    #[cfg(target_arch = "wasm32")]
    pub fn current_value(&self) -> f64 {
        self.value_at(crate::utils::now_secs())
    }

    /// Format the current interpolated value using wall-clock time (WASM-compatible).
    #[cfg(target_arch = "wasm32")]
    pub fn current_display(&self) -> String {
        self.display_at(crate::utils::now_secs())
    }
}

/// Format a float with comma-separated thousands and fixed decimal places.
fn format_with_commas(value: f64, decimals: u8) -> String {
    let rounded = if decimals == 0 {
        value.round() as i64
    } else {
        let factor = 10f64.powi(decimals as i32);
        (value * factor).round() as i64 / factor.round() as i64
    };

    // For zero decimals, use integer formatting with commas
    if decimals == 0 {
        return crate::utils::format_number(rounded);
    }

    // For decimals, format the integer part with commas, then append decimal part
    let factor = 10f64.powi(decimals as i32);
    let scaled = (value * factor).round() as i64;
    let int_part = scaled / factor as i64;
    let frac_part = (scaled % factor as i64).unsigned_abs();

    let int_str = crate::utils::format_number(int_part);
    format!("{int_str}.{frac_part:0>width$}", width = decimals as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolation() {
        let counter = AnimatedCounter::new(100.0, 1000.0, 1.0);
        assert!((counter.value_at(1000.0) - 100.0).abs() < f64::EPSILON);
        assert!((counter.value_at(1010.0) - 110.0).abs() < f64::EPSILON);
        assert!((counter.value_at(1100.0) - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_ceiling() {
        let counter = AnimatedCounter::new(95.0, 1000.0, 1.0).ceiling(100.0);
        assert!((counter.value_at(1010.0) - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_format_with_commas() {
        assert_eq!(format_with_commas(12345.0, 0), "12,345");
        assert_eq!(format_with_commas(12345.678, 2), "12,345.68");
        assert_eq!(format_with_commas(0.5, 1), "0.5");
    }

    #[test]
    fn test_no_negative_elapsed() {
        let counter = AnimatedCounter::new(100.0, 1000.0, 1.0);
        // Time before snapshot — should clamp to zero elapsed
        assert!((counter.value_at(990.0) - 100.0).abs() < f64::EPSILON);
    }
}
