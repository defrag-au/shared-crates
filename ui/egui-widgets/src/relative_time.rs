//! `RelativeTime` — a tiny auto-scaling "time ago" label.
//!
//! Renders a unix-seconds timestamp as a human relative time that steps sensibly
//! from seconds → minutes → hours → days → weeks (`20s ago`, `58m ago`, `2h ago`,
//! `4d ago`, `3w ago`) instead of an unbounded `3480s ago`. The scaling lives in
//! the pure, native-testable [`relative_label`]; the widget wraps it with styling
//! and a resolved "now" (live on wasm/native, or pinned for deterministic stories).

use egui::{Color32, Response, RichText, Ui, Widget};

const MINUTE: i64 = 60;
const HOUR: i64 = 3_600;
const DAY: i64 = 86_400;
const WEEK: i64 = 604_800;

/// Pure relative-time label from a signed delta (`now - timestamp`, in seconds).
/// Steps to the largest sensible unit; anything under ~5s (or negative clock skew)
/// reads as "just now". Native + `#[test]`-friendly — no clock access.
pub fn relative_label(delta_secs: i64) -> String {
    if delta_secs < 5 {
        "just now".to_string()
    } else if delta_secs < MINUTE {
        format!("{delta_secs}s ago")
    } else if delta_secs < HOUR {
        format!("{}m ago", delta_secs / MINUTE)
    } else if delta_secs < DAY {
        format!("{}h ago", delta_secs / HOUR)
    } else if delta_secs < WEEK {
        format!("{}d ago", delta_secs / DAY)
    } else {
        format!("{}w ago", delta_secs / WEEK)
    }
}

/// Resolve "now" as unix seconds — `js_sys::Date::now()` on wasm, `SystemTime`
/// natively (so the widget works in the desktop storybook too).
fn now_secs() -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Date::now() / 1000.0) as i64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

/// A drop-in auto-scaling "time ago" label: `ui.add(RelativeTime::new(ts))`.
/// Muted + small by default; tune with [`Self::color`] / [`Self::size`].
pub struct RelativeTime {
    timestamp_secs: i64,
    now_override: Option<i64>,
    color: Color32,
    size: f32,
}

impl RelativeTime {
    /// From a unix-seconds timestamp; "now" is resolved live unless [`Self::now`].
    pub fn new(timestamp_secs: i64) -> Self {
        Self {
            timestamp_secs,
            now_override: None,
            color: Color32::from_gray(150),
            size: 12.0,
        }
    }

    /// Pin "now" (unix seconds) — for deterministic stories / tests, or to share
    /// one clock read across a list so every row agrees.
    pub fn now(mut self, now_secs: i64) -> Self {
        self.now_override = Some(now_secs);
        self
    }

    /// Override the text colour (default: muted grey).
    pub fn color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Override the text size (default: 12.0).
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// The rendered string, without drawing (e.g. for a tooltip or test).
    pub fn label(&self) -> String {
        let now = self.now_override.unwrap_or_else(now_secs);
        relative_label(now - self.timestamp_secs)
    }
}

impl Widget for RelativeTime {
    fn ui(self, ui: &mut Ui) -> Response {
        let text = self.label();
        ui.label(RichText::new(text).color(self.color).size(self.size))
    }
}

#[cfg(test)]
mod tests {
    use super::relative_label;

    #[test]
    fn scales_to_the_largest_sensible_unit() {
        assert_eq!(relative_label(-10), "just now"); // clock skew
        assert_eq!(relative_label(0), "just now");
        assert_eq!(relative_label(4), "just now");
        assert_eq!(relative_label(20), "20s ago");
        assert_eq!(relative_label(59), "59s ago");
        assert_eq!(relative_label(60), "1m ago");
        assert_eq!(relative_label(3480), "58m ago"); // the screenshot's "3480s ago"
        assert_eq!(relative_label(3600), "1h ago");
        assert_eq!(relative_label(7_200), "2h ago");
        assert_eq!(relative_label(86_400), "1d ago");
        assert_eq!(relative_label(4 * DAY_T), "4d ago");
        assert_eq!(relative_label(14 * DAY_T), "2w ago");
    }

    const DAY_T: i64 = 86_400;
}
