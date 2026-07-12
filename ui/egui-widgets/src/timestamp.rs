//! `Timestamp` — a tiny atom that renders a unix-seconds timestamp **consistently**
//! as ISO-8601 (UTC), with an optional clean badge presentation.
//!
//! Why an atom: timestamps were being hand-rendered per-call-site with
//! `RichText::new(..).small().monospace()` etc., which is both inconsistent and
//! a trap — `.monospace()` is shorthand for `.text_style(Monospace)` and silently
//! overwrites a preceding `.small()`, so the text jumps to the monospace theme
//! size. This atom pins an **explicit** size + monospace family so every
//! timestamp in the app looks the same, and centralises the (dep-free) date math.
//!
//! ```ignore
//! ui.add(Timestamp::new(order.created_at).now(now));            // 2026-05-28 20:26
//! ui.add(Timestamp::new(ts).badge(true).with_seconds(true));   // framed chip, :SS
//! ```

use egui::{Color32, CornerRadius, Frame, Margin, Response, RichText, Stroke, Ui, Widget};

use crate::relative_time::relative_label;

/// Format unix seconds as ISO-8601 (UTC), `YYYY-MM-DD HH:MM[:SS]`. Dep-free (the
/// wasm/native widget can't pull `chrono`); shared by the widget + any caller
/// that needs the bare string.
pub fn format_iso8601(secs: i64, with_seconds: bool) -> String {
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (h, m, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    let (y, mo, d) = civil_from_days(days);
    if with_seconds {
        format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}")
    } else {
        format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}")
    }
}

/// A consistent timestamp atom. Defaults: plain (no badge), no seconds, muted
/// monospace at 11px, hover shows the full `:SS` form + relative "x ago" (when
/// a `now` is supplied).
pub struct Timestamp {
    secs: i64,
    now_override: Option<i64>,
    badge: bool,
    seconds: bool,
    color: Color32,
    size: f32,
}

impl Timestamp {
    pub fn new(secs: i64) -> Self {
        Self {
            secs,
            now_override: None,
            badge: false,
            seconds: false,
            color: Color32::from_gray(160),
            size: 11.0,
        }
    }

    /// Render inside a clean framed chip rather than bare text.
    pub fn badge(mut self, badge: bool) -> Self {
        self.badge = badge;
        self
    }

    /// Include `:SS` in the inline label (always present on the hover).
    pub fn with_seconds(mut self, seconds: bool) -> Self {
        self.seconds = seconds;
        self
    }

    /// Pin "now" (unix seconds) so the hover can show a relative "x ago" — and
    /// stay deterministic in stories.
    pub fn now(mut self, now_secs: i64) -> Self {
        self.now_override = Some(now_secs);
        self
    }

    pub fn color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// The rendered inline string (no drawing).
    pub fn label(&self) -> String {
        format_iso8601(self.secs, self.seconds)
    }
}

impl Widget for Timestamp {
    fn ui(self, ui: &mut Ui) -> Response {
        // Explicit size (NOT `.small()`) so `.monospace()` can't bump it.
        let rich = RichText::new(self.label())
            .monospace()
            .size(self.size)
            .color(self.color);
        let resp = if self.badge {
            Frame::new()
                .fill(Color32::from_gray(30))
                .stroke(Stroke::new(1.0_f32, Color32::from_gray(55)))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::symmetric(6, 1))
                .show(ui, |ui| ui.label(rich))
                .response
        } else {
            ui.label(rich)
        };

        let mut hover = format_iso8601(self.secs, true);
        hover.push_str(" UTC");
        if let Some(now) = self.now_override {
            hover.push('\n');
            hover.push_str(&relative_label(now - self.secs));
        }
        resp.on_hover_text(hover)
    }
}

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's `civil_from_days`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::format_iso8601;

    #[test]
    fn iso8601_known_epochs() {
        assert_eq!(format_iso8601(0, false), "1970-01-01 00:00");
        assert_eq!(format_iso8601(0, true), "1970-01-01 00:00:00");
        assert_eq!(format_iso8601(1_000_000_000, true), "2001-09-09 01:46:40");
        assert_eq!(format_iso8601(1_700_000_000, false), "2023-11-14 22:13");
    }
}
