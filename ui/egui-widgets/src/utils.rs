//! Shared UI utility functions — formatting, display helpers, and reusable
//! egui components used across defrag frontends.

use egui::{RichText, Ui};

use crate::theme;

// ============================================================================
// Number formatting
// ============================================================================

/// Format an integer with comma separators (e.g. 1234567 → "1,234,567").
pub fn format_number(n: i64) -> String {
    if n < 1000 && n > -1000 {
        return n.to_string();
    }
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    let formatted: String = result.chars().rev().collect();
    if n < 0 {
        format!("-{formatted}")
    } else {
        formatted
    }
}

// ============================================================================
// ADA / currency formatting
// ============================================================================

/// Format lovelace as ADA with comma-separated whole part (e.g. 3_000_000_000 → "3,000").
/// Shows decimals only when they're non-zero (e.g. 1_500_000 → "1.5", 3_000_000 → "3").
pub fn format_ada(lovelace: i64) -> String {
    let ada = lovelace as f64 / 1_000_000.0;
    let whole = ada.trunc() as i64;
    let frac = (ada.fract().abs() * 100.0).round() as i64;
    if frac == 0 {
        format_number(whole)
    } else if frac % 10 == 0 {
        format!("{}.{}", format_number(whole), frac / 10)
    } else {
        format!("{}.{frac:02}", format_number(whole))
    }
}

/// Format lovelace as ADA with "ADA" suffix (e.g. 3_000_000_000 → "3,000 ADA").
pub fn format_lovelace(lovelace: i64) -> String {
    format!("{} ADA", format_ada(lovelace))
}

/// Format a percentage, dropping unnecessary trailing zeros
/// (e.g. 4.0 → "4%", 4.5 → "4.5%", 12.75 → "12.75%").
pub fn format_percent(pct: f64) -> String {
    if pct.fract().abs() < 0.001 {
        format!("{}%", pct as i64)
    } else if (pct * 10.0).fract().abs() < 0.01 {
        format!("{pct:.1}%")
    } else {
        format!("{pct:.2}%")
    }
}

// ============================================================================
// String truncation
// ============================================================================

/// Truncate a hex string for display (e.g. "abc123...def456").
///
/// Returns the original string if it's already short enough.
pub fn truncate_hex(hex: &str, prefix_len: usize, suffix_len: usize) -> String {
    if hex.len() <= prefix_len + suffix_len + 3 {
        return hex.to_string();
    }
    format!(
        "{}...{}",
        &hex[..prefix_len],
        &hex[hex.len() - suffix_len..]
    )
}

// ============================================================================
// Duration formatting
// ============================================================================

/// Format seconds as a human-readable duration (e.g. "2d 3h", "45m", "30s").
pub fn format_duration(secs: u64) -> String {
    if secs >= 86400 {
        let d = secs / 86400;
        let h = (secs % 86400) / 3600;
        if h > 0 {
            format!("{d}d {h}h")
        } else {
            format!("{d}d")
        }
    } else if secs >= 3600 {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m > 0 {
            format!("{h}h {m}m")
        } else {
            format!("{h}h")
        }
    } else if secs >= 60 {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

// ============================================================================
// Time formatting (WASM only — requires js_sys)
// ============================================================================

/// Format a unix timestamp as relative time (e.g. "2m ago", "3h ago").
#[cfg(target_arch = "wasm32")]
pub fn relative_time(timestamp: i64) -> String {
    let now = (js_sys::Date::now() / 1000.0) as i64;
    let delta = now - timestamp;

    if delta < 0 {
        return "just now".to_string();
    }
    if delta < 60 {
        return format!("{delta}s ago");
    }
    if delta < 3600 {
        return format!("{}m ago", delta / 60);
    }
    if delta < 86400 {
        return format!("{}h ago", delta / 3600);
    }
    format!("{}d ago", delta / 86400)
}

/// Current time as seconds since epoch (WASM-compatible).
#[cfg(target_arch = "wasm32")]
pub fn now_secs() -> f64 {
    js_sys::Date::now() / 1000.0
}

// ============================================================================
// Reusable UI components
// ============================================================================

/// Compact stat card: small muted label above a larger primary value.
pub fn stat_card(ui: &mut Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).color(theme::TEXT_MUTED).size(10.0));
        ui.label(RichText::new(value).color(theme::TEXT_PRIMARY).size(16.0));
    });
}

/// Section heading: bold primary text with spacing below.
pub fn section_heading(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .color(theme::TEXT_PRIMARY)
            .size(16.0)
            .strong(),
    );
    ui.add_space(8.0);
}
