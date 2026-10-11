//! RFC 3339 UTC timestamps without pulling in a date-time dependency.
//!
//! OCFL §3.5.3.1 requires `created` to be RFC 3339 with a timezone and
//! second-level granularity (fractional seconds optional). The crate never
//! reads a clock itself on the pure path — the host passes the timestamp, the
//! same "host owns the clock" rule the rest of the estate uses — so this is a
//! formatter for a unix-second count, plus a cheap shape check.

/// Format `secs` (unix seconds, UTC) as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn rfc3339_utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let time = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second) = (time / 3_600, (time % 3_600) / 60, time % 60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Does `value` have the shape of an RFC 3339 timestamp — `YYYY-MM-DDTHH:MM:SS`
/// followed by `Z` or a numeric offset, fractional seconds allowed? Shape only,
/// no calendar validation.
pub fn has_rfc3339_shape(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 20 {
        return false;
    }
    for (at, want) in [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')] {
        if bytes.get(at) != Some(&want) {
            return false;
        }
    }
    let digits = |range: std::ops::Range<usize>| bytes[range].iter().all(|b| b.is_ascii_digit());
    if !(digits(0..4)
        && digits(5..7)
        && digits(8..10)
        && digits(11..13)
        && digits(14..16)
        && digits(17..19))
    {
        return false;
    }
    // The tail after the seconds: `Z`, or an offset like `+10:00`.
    let tail = &bytes[19..];
    match tail.last() {
        Some(b'Z' | b'z') => true,
        Some(_) => tail.contains(&b'+') || tail.contains(&b'-'),
        None => false,
    }
}

/// Days-from-epoch → (year, month, day), Howard Hinnant's civil-from-days.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_unix_timestamps_format_correctly() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(946_684_800), "2000-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(1_704_067_200), "2024-01-01T00:00:00Z");
        // Leap day: 59 days after 2024-01-01.
        assert_eq!(
            rfc3339_utc(1_704_067_200 + 59 * 86_400),
            "2024-02-29T00:00:00Z"
        );
        assert_eq!(rfc3339_utc(1_704_067_200 + 86_399), "2024-01-01T23:59:59Z");
    }

    #[test]
    fn shape_check_accepts_and_rejects() {
        assert!(has_rfc3339_shape("2026-10-11T00:00:00Z"));
        assert!(has_rfc3339_shape("2026-10-11T00:00:00.5Z"));
        assert!(has_rfc3339_shape("2026-10-11T00:00:00+10:00"));
        assert!(!has_rfc3339_shape("2026-10-11"));
        assert!(!has_rfc3339_shape("2026-10-11 00:00:00Z"));
        assert!(!has_rfc3339_shape("2026-10-11T00:00:00"));
    }
}
