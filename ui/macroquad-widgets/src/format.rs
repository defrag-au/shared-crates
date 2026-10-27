//! Numbers and times as a trader reads them: compact amounts, prices with
//! their significant digits, shares, and civil UTC dates.
//!
//! These are the exact forms the token arena uses, moved here so a second
//! market surface does not write a tenth copy of `ada()` that rounds
//! differently. Every function is pure and renderer-free — nothing here
//! touches a `Painter`, a font or the clock.
//!
//! UTC, never local time: a token's history is read against block timestamps,
//! and a reader comparing two screens should not see two dates for one beat.

/// `1234` → `1.2K`, `21848564` → `21.8M`.
pub fn compact(n: f64) -> String {
    let a = n.abs();
    let (v, unit) = if a >= 1e9 {
        (n / 1e9, "B")
    } else if a >= 1e6 {
        (n / 1e6, "M")
    } else if a >= 1e3 {
        (n / 1e3, "K")
    } else {
        return if a >= 10.0 || a == a.trunc() {
            format!("{n:.0}")
        } else {
            format!("{n:.1}")
        };
    };
    if v.abs() >= 100.0 {
        format!("{v:.0}{unit}")
    } else {
        format!("{v:.1}{unit}")
    }
}

/// Lovelace as whole ADA, compact: `99999627` → `100`.
pub fn ada(lovelace: u64) -> String {
    compact(lovelace as f64 / 1e6)
}

/// A price in ADA with four significant digits however small:
/// `0.000006626` → `0.000006627`.
pub fn price(ada: f64) -> String {
    if ada <= 0.0 || !ada.is_finite() {
        return "—".into();
    }
    if ada >= 1.0 {
        return format!("{ada:.3}");
    }
    let mut digits = 0;
    let mut v = ada;
    while v < 0.1 && digits < 12 {
        v *= 10.0;
        digits += 1;
    }
    let decimals = (digits + 4) as usize;
    format!("{ada:.decimals$}")
}

/// `+47.6%`, `-3.1%`.
pub fn percent(f: f64) -> String {
    let p = f * 100.0;
    // What rounds to nothing is nothing: not a red "-0.0%".
    if p.abs() < 0.05 {
        return "0.0%".into();
    }
    if p.abs() >= 1000.0 {
        format!("{p:+.0}%")
    } else {
        format!("{p:+.1}%")
    }
}

/// A share, unsigned: `13.5%`, `0.4%`, `<0.1%` for what is there but small.
pub fn share(f: f64) -> String {
    let p = f * 100.0;
    if p > 0.0 && p < 0.05 {
        return "<0.1%".into();
    }
    format!("{p:.1}%")
}

/// Unix seconds as UTC `HH:MM`.
pub fn clock(unix: u64) -> String {
    let s = unix % 86_400;
    format!("{:02}:{:02}", s / 3_600, (s % 3_600) / 60)
}

/// Unix seconds as a civil date: (year, month 1–12, day).
fn civil(unix: u64) -> (i64, i64, i64) {
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = (unix / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

/// Unix seconds as `24 Oct`.
pub fn day(unix: u64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (_, m, d) = civil(unix);
    format!("{d} {}", MONTHS[(m - 1) as usize])
}

/// Unix seconds as the year: `2026`.
pub fn year(unix: u64) -> String {
    civil(unix).0.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_read_compactly() {
        assert_eq!(compact(21_848_564.0), "21.8M");
        assert_eq!(compact(999.0), "999");
        assert_eq!(compact(1_234.0), "1.2K");
        assert_eq!(compact(6_106.0), "6.1K");
        assert_eq!(compact(150_000.0), "150K");
        assert_eq!(ada(99_999_627), "100");
        assert_eq!(ada(2_000_005), "2.0");
        assert_eq!(percent(-0.000_2), "0.0%");
        assert_eq!(percent(0.476), "+47.6%");
        assert_eq!(share(0.135_282), "13.5%");
        assert_eq!(share(0.000_2), "<0.1%");
        assert_eq!(share(0.0), "0.0%");
    }

    #[test]
    fn tiny_prices_keep_their_digits() {
        assert_eq!(price(0.000_006_626_7), "0.000006627");
        assert_eq!(price(0.000_184_86), "0.0001849");
        assert_eq!(price(1.5), "1.500");
    }

    #[test]
    fn dates_and_times_are_utc() {
        // BUNNY's launch: 2026-10-23 22:49:57 UTC.
        assert_eq!(day(1_792_795_797), "23 Oct");
        assert_eq!(clock(1_792_795_797), "22:49");
        assert_eq!(day(0), "1 Jan");
        assert_eq!(year(1_792_795_797), "2026");
        assert_eq!(year(1_736_812_511), "2025", "$PERP's launch, 14 Jan 2025");
    }
}
