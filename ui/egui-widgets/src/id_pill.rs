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

use std::borrow::Cow;

use egui::{Align, Color32, Layout, RichText, Ui};

use crate::icons::{install_phosphor_font, PhosphorIcon};

/// Visual layout pick — `IdPill::layout(…)` consumes one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdPillLayout {
    /// Three rows inside a subtle framed pill: **label** (small muted
    /// header) over **value** (monospace body) over **copy button**
    /// (right-aligned footer). The richer presentation, used as the
    /// default. Shows the *full* value when it fits the available width;
    /// auto-truncates only when constrained. Best for standalone display
    /// stacks (the typical "Common uses" pattern: policy / wallet /
    /// stake / tx all visible at once).
    Stacked,
    /// Single horizontal row: `label  value [Copy]`. Compact, no frame.
    /// Truncation always applies (per [`IdPill::with_widths`] / the
    /// default). Best for inline use in header bars where vertical
    /// budget is tight (e.g. the portal's Configure-window header,
    /// where the policy pill shares a row with a Refresh button).
    Inline,
}

/// Builder.
pub struct IdPill<'a> {
    label: Option<&'a str>,
    value_full: Cow<'a, str>,
    value_short: Option<String>,
    widths: (usize, usize),
    label_min_width: Option<f32>,
    value_min_width: Option<f32>,
    min_width: Option<f32>,
    copyable: bool,
    layout: IdPillLayout,
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
            value_full: Cow::Borrowed(value_full),
            value_short: None,
            // (10, 8) → 19-char middle-elision: `8532f316dd…45d87a1b`.
            // Reads as a consistent total length across policy_id (56 chars),
            // bech32 addresses (~103 chars), and tx hashes (64 chars).
            widths: (10, 8),
            label_min_width: None,
            value_min_width: None,
            min_width: None,
            copyable: true,
            layout: IdPillLayout::Stacked,
            label_color: Color32::from_gray(140),
            value_color: Color32::from_gray(220),
        }
    }

    /// A pill for a **UTxO reference** — `tx_hash#index`, label-less, in the
    /// compact [`IdPillLayout::Inline`] shape (suited to a dense UTxO list).
    /// The copy button writes the canonical `tx_hash#index` (paste into an
    /// explorer — trim the `#index` — or straight into `cardano-cli`); the
    /// display middle-elides the hash but keeps the index: `739df5ec…6df854#2`.
    /// Returns `IdPill<'static>` (it owns the formatted value), so the caller
    /// can chain builders + `show(ui)` without holding the strings.
    pub fn utxo_ref(tx_hash: &str, index: u32) -> IdPill<'static> {
        let full = format!("{tx_hash}#{index}");
        let short = format!("{}#{index}", truncate_middle(tx_hash, 8, 6));
        IdPill {
            label: None,
            value_full: Cow::Owned(full),
            value_short: Some(short),
            widths: (8, 6),
            label_min_width: None,
            value_min_width: None,
            min_width: None,
            copyable: true,
            layout: IdPillLayout::Inline,
            label_color: Color32::from_gray(140),
            value_color: Color32::from_gray(160),
        }
    }

    /// Pick the visual presentation. Default is [`IdPillLayout::Stacked`]
    /// (three-row framed pill); pass `Inline` for the compact one-row
    /// shape suited to header bars.
    pub fn layout(mut self, layout: IdPillLayout) -> Self {
        self.layout = layout;
        self
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

    /// Reserve a minimum width (px) for the label so a vertical stack of
    /// `IdPill`s with different-length labels align their value columns.
    /// Pick a width that fits the longest label in the stack — e.g. `45.0`
    /// is comfortable for short labels like `policy`/`wallet`/`stake`/`tx`.
    /// Without this set, the label takes its natural width (the source of
    /// the alignment drift in stacked groups).
    pub fn label_min_width(mut self, w: f32) -> Self {
        self.label_min_width = Some(w);
        self
    }

    /// Reserve a minimum width (px) for the value column so the copy
    /// button sits at the same x-position across a stack of pills with
    /// slightly different truncated value lengths (testnet addresses
    /// usually push wider than mainnet because of the longer prefix).
    pub fn value_min_width(mut self, w: f32) -> Self {
        self.value_min_width = Some(w);
        self
    }

    /// Reserve a minimum width (px) for the whole pill frame, so a
    /// vertical stack of [`IdPillLayout::Stacked`] pills renders with a
    /// uniform border regardless of value length. The widget has no view
    /// of its siblings — pair with [`stacked_width_for`] across the
    /// values you intend to render and take the max:
    ///
    /// ```ignore
    /// let w = [policy, addr, stake, tx]
    ///     .iter()
    ///     .map(|v| stacked_width_for(ui, v, true))
    ///     .fold(0.0_f32, f32::max);
    /// IdPill::new("policy", policy).min_width(w).show(ui);
    /// IdPill::new("wallet", addr).min_width(w).show(ui);
    /// // …
    /// ```
    ///
    /// Has no effect on [`IdPillLayout::Inline`].
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = Some(w);
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

    /// Render. Dispatches on [`IdPill::layout`].
    pub fn show(self, ui: &mut Ui) -> IdPillResponse {
        match self.layout {
            IdPillLayout::Stacked => self.show_stacked(ui),
            IdPillLayout::Inline => self.show_inline(ui),
        }
    }

    /// Two-row framed shape: label header / value-with-trailing-copy
    /// body. Shows the full value when it fits the available width;
    /// falls back to middle-elision (`widths`) when constrained.
    fn show_stacked(self, ui: &mut Ui) -> IdPillResponse {
        let mut response = IdPillResponse::default();
        if self.copyable {
            install_phosphor_font(ui.ctx());
        }
        // `min_width` caps the frame's *outer* width so a column of
        // pills shares a uniform border AND stays at the natural
        // max-content width of the stack — not the parent's full
        // available width. `ui.set_min_width` only enforces a lower
        // bound, so a wide parent (e.g. the storybook scroll, the
        // dashboard centre panel) would still let the frame stretch.
        // `allocate_ui_with_layout(vec2(w, 0), …)` carves out exactly
        // `w` of horizontal space for the frame to occupy.
        if let Some(w) = self.min_width {
            ui.allocate_ui_with_layout(egui::vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
                self.render_stacked_frame(ui, &mut response, true)
            });
        } else {
            self.render_stacked_frame(ui, &mut response, false);
        }
        response
    }

    /// Render the framed two-row pill. `right_align_copy` decides
    /// whether the copy button rides the row's right edge (used when
    /// `min_width` has carved out a uniform-width slot for the frame)
    /// or trails inline next to the value (used when the frame is
    /// content-sized — any `right_to_left` allocation there would
    /// stretch the row across the parent's full available width,
    /// re-introducing the original "frame grew huge" bug).
    fn render_stacked_frame(
        &self,
        ui: &mut Ui,
        response: &mut IdPillResponse,
        right_align_copy: bool,
    ) {
        // Subtle dark fill + thin border — matches the crate's
        // framed-block palette (Chip uses similar tones).
        let fill = Color32::from_rgb(22, 24, 30);
        let stroke = Color32::from_rgb(48, 52, 64);
        egui::Frame::new()
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, stroke))
            .corner_radius(egui::CornerRadius::same(4))
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                // Header: small muted label on its own row.
                if let Some(label) = self.label {
                    ui.label(RichText::new(label).small().color(self.label_color));
                }
                // Body row: monospace value + copy button.
                ui.horizontal(|ui| {
                    let copy_budget = if self.copyable { 22.0 } else { 0.0 };
                    let display =
                        self.choose_display_with_budget(ui, ui.available_width() - copy_budget);
                    ui.label(RichText::new(display).monospace().color(self.value_color))
                        .on_hover_text(self.value_full.as_ref());
                    if !self.copyable {
                        return;
                    }
                    let clicked = if right_align_copy {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.small_button(PhosphorIcon::Copy.rich_text(12.0, self.label_color))
                                .on_hover_text("Copy to clipboard")
                                .clicked()
                        })
                        .inner
                    } else {
                        ui.small_button(PhosphorIcon::Copy.rich_text(12.0, self.label_color))
                            .on_hover_text("Copy to clipboard")
                            .clicked()
                    };
                    if clicked {
                        ui.ctx().copy_text(self.value_full.to_string());
                        response.copied = true;
                    }
                });
            });
    }

    /// Single-row compact shape — `label  value [Copy]`, no frame.
    /// `label_min_width` / `value_min_width` reserve column widths for
    /// alignment within a vertical stack of Inline pills.
    fn show_inline(self, ui: &mut Ui) -> IdPillResponse {
        let mut response = IdPillResponse::default();
        ui.horizontal(|ui| {
            if let Some(label) = self.label {
                let label_widget =
                    egui::Label::new(RichText::new(label).small().color(self.label_color));
                if let Some(w) = self.label_min_width {
                    ui.add_sized([w, ui.spacing().interact_size.y], label_widget);
                } else {
                    ui.add(label_widget);
                }
            }
            let short = self.value_short.unwrap_or_else(|| {
                truncate_middle(self.value_full.as_ref(), self.widths.0, self.widths.1)
            });
            let value_widget = egui::Label::new(
                RichText::new(short)
                    .monospace()
                    .small()
                    .color(self.value_color),
            );
            let value_resp = if let Some(w) = self.value_min_width {
                ui.add_sized([w, ui.spacing().interact_size.y], value_widget)
            } else {
                ui.add(value_widget)
            };
            value_resp.on_hover_text(self.value_full.as_ref());
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

    /// Choose the value display string against a caller-supplied width
    /// budget: host override, or the full value when it fits the budget,
    /// or a middle-elision otherwise. Used by the Stacked layout (which
    /// subtracts the trailing copy button width from `ui.available_width`
    /// so the button still fits inline); Inline always truncates per
    /// [`IdPill::with_widths`].
    fn choose_display_with_budget(&self, ui: &Ui, budget: f32) -> String {
        if let Some(short) = &self.value_short {
            return short.clone();
        }
        let monospace = egui::FontId::new(
            egui::TextStyle::Body.resolve(ui.style()).size,
            egui::FontFamily::Monospace,
        );
        // `painter().layout_no_wrap` returns an `Arc<Galley>` and handles
        // the `Fonts` lock internally — `ui.fonts(|f| f.layout_no_wrap(…))`
        // doesn't compile because `layout_no_wrap` needs `&mut Fonts`
        // while the closure receives `&Fonts`.
        let galley =
            ui.painter()
                .layout_no_wrap(self.value_full.to_string(), monospace, Color32::WHITE);
        let full_width = galley.rect.width();
        if full_width <= budget {
            self.value_full.to_string()
        } else {
            truncate_middle(self.value_full.as_ref(), self.widths.0, self.widths.1)
        }
    }
}

/// Measure the natural width (in px) a [`IdPillLayout::Stacked`] pill
/// needs for the given monospace value at the current ui's
/// `TextStyle::Body` size. Accounts for the 8+8 frame padding and (when
/// `copyable`) the trailing copy button. Fold a max across a slice of
/// values you intend to render in the same stack and pass the result
/// to [`IdPill::min_width`] on each.
pub fn stacked_width_for(ui: &Ui, value: &str, copyable: bool) -> f32 {
    let monospace = egui::FontId::new(
        egui::TextStyle::Body.resolve(ui.style()).size,
        egui::FontFamily::Monospace,
    );
    let text_w = ui
        .painter()
        .layout_no_wrap(value.to_string(), monospace, Color32::WHITE)
        .rect
        .width();
    // Mirrors `inner_margin(symmetric(8, 6))` + trailing-button budget.
    let frame_pad = 16.0;
    let copy_w = if copyable { 22.0 } else { 0.0 };
    text_w + frame_pad + copy_w
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
