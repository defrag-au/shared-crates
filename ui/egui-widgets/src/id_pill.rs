//! `IdPill` — small inline display of a long identifier with a copy
//! affordance. The recurring `policy 8532f316dd09…45d87a1b 📋` pattern
//! used for policy_id, wallet bech32 addresses, deposit addresses, tx
//! hashes, and other hash-shaped strings.
//!
//! Composed of three parts, any subset optional:
//! - small muted **label** ("policy" / "wallet" / "deposit" / "tx")
//! - monospace **truncated value** (middle-elided) with the full value
//!   on hover for verification
//! - **copy button** (Phosphor `Copy` glyph by default; the widget
//!   handles the clipboard call itself)
//!
//! The widget owns truncation: callers pass the full value + a
//! `(prefix_width, suffix_width)` pair; if the value already fits the
//! pill renders it verbatim. Hosts that need a specific truncation can
//! pass their own pre-truncated string via [`IdPill::with_short`].
//!
//! ## Example
//!
//! ```ignore
//! IdPill::new("policy", policy_id_full).show(ui);
//! IdPill::new("wallet", address)
//!     .with_widths(14, 8)
//!     .show(ui);
//! ```

use egui::{Color32, RichText, Ui};

use crate::icons::{install_phosphor_font, PhosphorIcon};

/// Builder.
pub struct IdPill<'a> {
    label: Option<&'a str>,
    value_full: &'a str,
    value_short: Option<String>,
    widths: (usize, usize),
    copyable: bool,
    label_color: Color32,
    value_color: Color32,
}

/// Outcome of one `IdPill::show()` call.
#[derive(Default, Debug)]
pub struct IdPillResponse {
    /// `true` when the user clicked the copy button (the widget has
    /// already written `value_full` to the clipboard).
    pub copied: bool,
}

impl<'a> IdPill<'a> {
    /// Construct a pill. `label` shows in muted grey to the left of the
    /// truncated value; pass an empty string (or use the no-label
    /// constructor) to omit it.
    pub fn new(label: &'a str, value_full: &'a str) -> Self {
        Self {
            label: Some(label).filter(|s| !s.is_empty()),
            value_full,
            value_short: None,
            widths: (10, 6),
            copyable: true,
            label_color: Color32::from_gray(140),
            value_color: Color32::from_gray(190),
        }
    }

    /// Override the widget-computed middle-elision. Useful when the host
    /// already has a pre-formatted display value (e.g. from a row VM
    /// that pre-computed `address_short`).
    pub fn with_short(mut self, short: impl Into<String>) -> Self {
        self.value_short = Some(short.into());
        self
    }

    /// Set the prefix / suffix widths for the middle-elision. Default
    /// `(10, 6)` suits a 56-char policy_id; bech32 addresses normally
    /// take `(14, 8)`.
    pub fn with_widths(mut self, prefix: usize, suffix: usize) -> Self {
        self.widths = (prefix, suffix);
        self
    }

    /// Suppress the copy button.
    pub fn copyable(mut self, b: bool) -> Self {
        self.copyable = b;
        self
    }

    /// Override the muted label colour.
    pub fn label_color(mut self, c: Color32) -> Self {
        self.label_color = c;
        self
    }

    /// Override the value colour.
    pub fn value_color(mut self, c: Color32) -> Self {
        self.value_color = c;
        self
    }

    /// Render.
    pub fn show(self, ui: &mut Ui) -> IdPillResponse {
        let mut response = IdPillResponse::default();
        ui.horizontal(|ui| {
            if let Some(label) = self.label {
                ui.label(RichText::new(label).small().color(self.label_color));
            }
            let short = self
                .value_short
                .unwrap_or_else(|| truncate_middle(self.value_full, self.widths.0, self.widths.1));
            ui.label(
                RichText::new(short)
                    .monospace()
                    .small()
                    .color(self.value_color),
            )
            .on_hover_text(self.value_full);
            if self.copyable {
                install_phosphor_font(ui.ctx());
                if ui
                    .small_button(PhosphorIcon::Copy.rich_text(11.0, self.label_color).small())
                    .on_hover_text("Copy to clipboard")
                    .clicked()
                {
                    ui.ctx().copy_text(self.value_full.to_string());
                    response.copied = true;
                }
            }
        });
        response
    }
}

/// Middle-elide a string at `prefix + ellipsis + suffix` width. Returns
/// the input unchanged when it's already short enough.
fn truncate_middle(s: &str, prefix: usize, suffix: usize) -> String {
    if s.chars().count() <= prefix + suffix + 1 {
        return s.to_string();
    }
    let p: String = s.chars().take(prefix).collect();
    let q: String = s
        .chars()
        .rev()
        .take(suffix)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{p}…{q}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate_middle("abc", 10, 6), "abc");
    }

    #[test]
    fn truncate_long_string_elides() {
        let s = "0123456789abcdef0123456789abcdef";
        assert_eq!(truncate_middle(s, 8, 6), "01234567…abcdef");
    }
}
