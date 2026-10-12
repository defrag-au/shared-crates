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

/// Is `value` a valid RFC 3339 date-time, to the strictness OCFL §3.5.3.1
/// needs: full date, `T`, full time to the second, optional fractional
/// seconds, and a timezone — `Z` or a numeric `±HH:MM` offset. Field ranges
/// are checked (month 1–12, day 1–31, …), not calendar exactness: `02-30`
/// passes.
pub fn is_rfc3339_datetime(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 20 {
        return false;
    }
    if bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
        return false;
    }
    if bytes.get(10) != Some(&b'T') && bytes.get(10) != Some(&b't') {
        return false;
    }
    if bytes.get(13) != Some(&b':') || bytes.get(16) != Some(&b':') {
        return false;
    }
    let number = |range: std::ops::Range<usize>| -> Option<u32> {
        let slice = bytes.get(range)?;
        if !slice.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(slice).ok()?.parse().ok()
    };
    let (Some(month), Some(day)) = (number(5..7), number(8..10)) else {
        return false;
    };
    let (Some(hour), Some(minute), Some(second)) = (number(11..13), number(14..16), number(17..19))
    else {
        return false;
    };
    number(0..4).is_some()
        && (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour <= 23
        && minute <= 59
        && second <= 60 // leap second
        && timezone_is_valid(&bytes[19..])
}

/// The tail after `HH:MM:SS`: optional `.fraction`, then `Z`/`z` or `±HH:MM`.
fn timezone_is_valid(tail: &[u8]) -> bool {
    let mut rest = tail;
    if rest.first() == Some(&b'.') {
        let digits = rest[1..].iter().take_while(|b| b.is_ascii_digit()).count();
        if digits == 0 {
            return false;
        }
        rest = &rest[1 + digits..];
    }
    match rest {
        [b'Z' | b'z'] => true,
        [sign, h1, h2, b':', m1, m2]
            if (*sign == b'+' || *sign == b'-')
                && [h1, h2, m1, m2].iter().all(|b| b.is_ascii_digit()) =>
        {
            let hours = u32::from(h1 - b'0') * 10 + u32::from(h2 - b'0');
            let minutes = u32::from(m1 - b'0') * 10 + u32::from(m2 - b'0');
            hours <= 23 && minutes <= 59
        }
        _ => false,
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
    fn datetime_check_accepts_and_rejects() {
        assert!(is_rfc3339_datetime("2026-10-11T00:00:00Z"));
        assert!(is_rfc3339_datetime("2026-10-11T00:00:00.5Z"));
        assert!(is_rfc3339_datetime("2021-03-31T10:21:21.740101149-05:00"));
        assert!(is_rfc3339_datetime("2026-10-11T00:00:00+10:00"));
        assert!(is_rfc3339_datetime("2026-10-11t00:00:00z"));
        assert!(!is_rfc3339_datetime("2026-10-11"));
        assert!(!is_rfc3339_datetime("2026-10-11 00:00:00Z"));
        // No timezone, and not granular to the second (E049's fixtures).
        assert!(!is_rfc3339_datetime("2026-10-11T00:00:00"));
        assert!(!is_rfc3339_datetime("2019-01-01T01:01Z"));
        assert!(!is_rfc3339_datetime("2026-10-11T00:00:00.Z"));
        assert!(!is_rfc3339_datetime("2026-13-11T00:00:00Z"));
        assert!(!is_rfc3339_datetime("2026-10-11T24:00:00Z"));
        assert!(!is_rfc3339_datetime("2026-10-11T00:00:00+24:00"));
        // What the crate's own formatter emits always validates.
        assert!(is_rfc3339_datetime(&rfc3339_utc(1_704_067_200)));
    }
}
