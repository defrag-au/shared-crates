//! `ErrorNote` — turns an ugly machine error string into a readable note.
//!
//! Engine/submit errors reach the UI as Rust-`Debug` blobs wrapping
//! triple-escaped JSON, e.g.
//!
//! ```text
//! submit_transaction: Http(Custom("Transaction submission failed (status 400):
//! {\"contents\":…{\"ConwayMempoolFailure \\\"All inputs are spent. Transaction
//! has probably already been included\\\"\"}…\"tag\":\"TxSubmitFail\"}"))
//! ```
//!
//! The one thing an operator wants — *"All inputs are spent…"* — is buried. The
//! pure [`summarize_error`] de-escapes the blob, pulls out the deepest
//! natural-language reason + any HTTP status, and the [`ErrorNote`] widget
//! renders that headline with the full de-escaped text behind a "show raw"
//! toggle.

use egui::{Color32, Label, RichText, Sense, Ui};

use crate::chip::{Chip, ChipVariant};
use crate::icons::{install_phosphor_font, PhosphorIcon};

/// The distilled view of a raw error string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorSummary {
    /// The human-readable reason — the deepest natural-language phrase, or a
    /// leading summary when there's no obvious nested reason.
    pub headline: String,
    /// The fully de-escaped raw text (for the "show raw" expander). Equals
    /// `headline` when there was nothing more to show.
    pub detail: String,
    /// HTTP status code, if the message embedded one (`… (status 400) …`).
    pub status: Option<u32>,
}

/// De-noise a raw error string into an [`ErrorSummary`]. Pure + testable.
pub fn summarize_error(raw: &str) -> ErrorSummary {
    let detail = unescape(raw);
    let status = extract_status(&detail);
    let headline = extract_reason(&detail).unwrap_or_else(|| leading_summary(&detail));
    ErrorSummary {
        headline,
        detail,
        status,
    }
}

/// Collapse backslash escaping (`\"` → `"`, `\\` → `\`, `\n`/`\t` → space),
/// repeatedly, so triple-escaped JSON (`\\\"`) flattens to readable text.
fn unescape(s: &str) -> String {
    let mut cur = s.to_string();
    for _ in 0..5 {
        let next = unescape_once(&cur);
        if next == cur {
            break;
        }
        cur = next;
    }
    cur
}

fn unescape_once(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('"') => {
                    out.push('"');
                    chars.next();
                }
                Some('\\') => {
                    out.push('\\');
                    chars.next();
                }
                Some('n') | Some('t') | Some('r') => {
                    out.push(' ');
                    chars.next();
                }
                _ => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// `… (status 400) …` → `400`.
fn extract_status(s: &str) -> Option<u32> {
    let i = s.find("status ")?;
    let digits: String = s[i + "status ".len()..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// The deepest human reason: of all double-quoted segments, the longest that
/// reads as natural language (has a space + a lowercase letter). This skips the
/// machine tags (`TxSubmitFail`, `ShelleyBasedEraConway`, …) and lands on the
/// actual message (`All inputs are spent. …`).
fn extract_reason(s: &str) -> Option<String> {
    s.split('"')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1) // odd segments are inside quotes
        .map(|(_, seg)| seg.trim())
        .filter(|seg| seg.contains(' ') && seg.chars().any(|c| c.is_ascii_lowercase()))
        .max_by_key(|seg| seg.len())
        .map(|s| s.to_string())
}

/// Fallback when there's no quoted reason: the text up to the first `{` (the
/// JSON body), trimmed of the Rust-`Debug` wrappers, capped to a readable length.
fn leading_summary(s: &str) -> String {
    let head = s.split('{').next().unwrap_or(s).trim();
    let head = head
        .trim_end_matches(['(', ':', ' '])
        .trim_start_matches(|c: char| c.is_ascii_lowercase() || c == '_' || c == ' ')
        .trim();
    let head = if head.is_empty() { s.trim() } else { head };
    if head.chars().count() > 200 {
        let cut: String = head.chars().take(197).collect();
        format!("{cut}…")
    } else {
        head.to_string()
    }
}

/// Render a raw error string as a clean note. See module docs.
pub struct ErrorNote<'a> {
    raw: &'a str,
}

impl<'a> ErrorNote<'a> {
    pub fn new(raw: &'a str) -> Self {
        Self { raw }
    }

    pub fn show(self, ui: &mut Ui) {
        let s = summarize_error(self.raw);
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                install_phosphor_font(ui.ctx());
                ui.label(PhosphorIcon::Warning.rich_text(13.0, Color32::from_rgb(220, 150, 90)));
                if let Some(code) = s.status {
                    Chip::new(&format!("HTTP {code}"))
                        .variant(ChipVariant::Danger)
                        .show(ui);
                }
                ui.label(
                    RichText::new(&s.headline)
                        .small()
                        .color(Color32::from_rgb(228, 184, 174)),
                );
            });

            // "show raw" only when the de-escaped detail carries more than the
            // headline already does.
            if s.detail.trim() != s.headline.trim() {
                let id = ui.id().with(("error_note_raw", self.raw));
                let mut open = ui.data_mut(|d| d.get_temp::<bool>(id)).unwrap_or(false);
                let toggle = ui.add(
                    Label::new(
                        RichText::new(if open { "hide raw" } else { "show raw" })
                            .small()
                            .underline()
                            .color(Color32::from_gray(135)),
                    )
                    .sense(Sense::click()),
                );
                if toggle.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if toggle.clicked() {
                    open = !open;
                    ui.data_mut(|d| d.insert_temp(id, open));
                }
                if open {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(&s.detail)
                            .monospace()
                            .small()
                            .color(Color32::from_gray(150)),
                    );
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_mempool_reason_and_status() {
        // The screenshot's error: Debug-wrapped, triple-escaped JSON.
        let raw = r#"submit_transaction: Http(Custom("Transaction submission failed (status 400): {\"contents\":{\"contents\":{\"era\":\"ShelleyBasedEraConway\",\"error\":[{\"ConwayMempoolFailure \\\"All inputs are spent. Transaction has probably already been included\\\"\"}],\"kind\":\"ShelleyTxValidationError\"},\"tag\":\"TxValidationErrorInCardanoMode\"},\"tag\":\"TxSubmitFail\"}"))"#;
        let s = summarize_error(raw);
        assert_eq!(
            s.headline,
            "All inputs are spent. Transaction has probably already been included"
        );
        assert_eq!(s.status, Some(400));
        // The de-escaped detail no longer carries backslash-escaped quotes.
        assert!(!s.detail.contains("\\\""));
    }

    #[test]
    fn plain_message_passes_through() {
        let s = summarize_error("collection sold out");
        assert_eq!(s.headline, "collection sold out");
        assert_eq!(s.status, None);
    }

    #[test]
    fn unescape_is_idempotent_on_clean_text() {
        assert_eq!(
            unescape("already clean (no escapes)"),
            "already clean (no escapes)"
        );
    }
}
