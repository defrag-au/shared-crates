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

impl ErrorSummary {
    /// A single-line, clipboard-friendly rendering — the reason up front, then
    /// the whole de-escaped error, whitespace-collapsed (NO line breaks). This is
    /// the cleanest shape to paste back into a chat/issue: no `\"` escaping, no
    /// pretty-print newlines, the human reason leading the full context.
    pub fn clipboard(&self) -> String {
        let detail = collapse_ws(&self.detail);
        let headline = self.headline.trim();
        if detail == headline || headline.is_empty() {
            detail
        } else {
            format!("{headline} — {detail}")
        }
    }
}

/// Collapse every run of whitespace (incl. newlines) to a single space + trim.
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
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
        .filter(|seg| {
            // natural language: has a space + a lowercase letter, and isn't the
            // structural wrapper (the leading `… failed (status 400): {` segment
            // contains a brace — the real reason never does).
            seg.contains(' ')
                && seg.chars().any(|c| c.is_ascii_lowercase())
                && !seg.contains(['{', '}', '[', ']'])
        })
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

/// Lenient JSON pretty-printer — indents `{`/`[`, newlines after `,`, `": "`
/// after keys, 2-space indent (like JS `JSON.stringify(x, null, 2)`). It is
/// string-aware (won't break on structural chars inside strings) but does NOT
/// require valid JSON, so it still tidies the malformed aeson-style blobs these
/// submit errors carry. Non-structural text is copied verbatim.
pub fn pretty_json(s: &str) -> String {
    fn newline_indent(out: &mut String, depth: usize) {
        out.push('\n');
        for _ in 0..depth {
            out.push_str("  ");
        }
    }
    let mut out = String::with_capacity(s.len() * 2);
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut escaped = false;
    for c in s.chars() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '{' | '[' => {
                out.push(c);
                depth += 1;
                newline_indent(&mut out, depth);
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                newline_indent(&mut out, depth);
                out.push(c);
            }
            ',' => {
                out.push(c);
                newline_indent(&mut out, depth);
            }
            ':' => out.push_str(": "),
            _ => out.push(c),
        }
    }
    out
}

/// Split a de-escaped error into `(prefix, json)` — the leading message and the
/// `{ … }` body — so the body can be pretty-printed and the trailing Rust-Debug
/// wrapper junk (`"))`) dropped.
fn split_json(detail: &str) -> Option<(&str, &str)> {
    let start = detail.find('{')?;
    let end = detail.rfind('}')?;
    (end > start).then(|| (detail[..start].trim_end(), &detail[start..=end]))
}

/// The "show raw" body: leading message (if any) + the pretty-printed JSON body.
fn pretty_detail(detail: &str) -> String {
    match split_json(detail) {
        Some((prefix, json)) => {
            let pretty = pretty_json(json);
            if prefix.is_empty() {
                pretty
            } else {
                format!("{prefix}\n{pretty}")
            }
        }
        None => detail.to_string(),
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

            // Controls row: copy (always) + show-raw toggle (when the de-escaped
            // detail carries more than the headline). `open` is read before the
            // row and rendered after it, so the toggle can flip it.
            let has_raw = s.detail.trim() != s.headline.trim();
            let raw_id = ui.id().with(("error_note_raw", self.raw));
            let mut open = has_raw && ui.data_mut(|d| d.get_temp::<bool>(raw_id)).unwrap_or(false);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                // Copy: a single-line, de-escaped form — clean to paste back.
                let copy = ui.add(
                    Label::new(PhosphorIcon::Copy.rich_text(12.0, Color32::from_gray(140)))
                        .sense(Sense::click()),
                );
                if copy.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if copy
                    .on_hover_text("copy error (single line, for sharing)")
                    .clicked()
                {
                    ui.ctx().copy_text(s.clipboard());
                }
                if has_raw {
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
                        ui.data_mut(|d| d.insert_temp(raw_id, open));
                    }
                }
            });
            if open {
                ui.add_space(2.0);
                ui.label(
                    RichText::new(pretty_detail(&s.detail))
                        .monospace()
                        .small()
                        .color(Color32::from_gray(150)),
                );
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

    #[test]
    fn pretty_json_indents_valid_json() {
        assert_eq!(
            pretty_json(r#"{"a":1,"b":[2,3]}"#),
            "{\n  \"a\": 1,\n  \"b\": [\n    2,\n    3\n  ]\n}"
        );
    }

    #[test]
    fn pretty_json_keeps_structural_chars_inside_strings() {
        // Braces/commas inside a string must not trigger indentation.
        assert_eq!(
            pretty_json(r#"{"msg":"a, b {c}"}"#),
            "{\n  \"msg\": \"a, b {c}\"\n}"
        );
    }

    #[test]
    fn clipboard_is_single_line_with_reason_first() {
        let raw = r#"submit_transaction: Http(Custom("Transaction submission failed (status 400): {\"error\":[{\"ConwayMempoolFailure \\\"All inputs are spent\\\"\"}],\"tag\":\"TxSubmitFail\"}"))"#;
        let clip = summarize_error(raw).clipboard();
        assert!(!clip.contains('\n'), "clipboard must be single-line");
        assert!(clip.starts_with("All inputs are spent — "));
        assert!(clip.contains("TxSubmitFail"));
        assert!(!clip.contains("\\\""), "clipboard must be de-escaped");
    }

    #[test]
    fn pretty_detail_splits_prefix_and_body() {
        let detail = r#"submission failed (status 400): {"tag":"TxSubmitFail"}"#;
        let pretty = pretty_detail(detail);
        assert!(pretty.starts_with("submission failed (status 400):\n{"));
        assert!(pretty.contains("\n  \"tag\": \"TxSubmitFail\""));
    }
}
