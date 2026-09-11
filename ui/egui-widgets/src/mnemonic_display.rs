//! BIP-39 mnemonic display with copy-to-clipboard + optional confirmation gate.
//!
//! Designed for the moment-of-truth UX where the platform hands a fresh
//! mnemonic to a client EXACTLY ONCE — provisioning a new client wallet,
//! GDPR Art. 20 export, etc. Visual emphasis on sensitivity:
//!
//! - the words sit on a dark inset card with a warning header
//! - the copy CTA is a single button (operator types nothing)
//! - a confirmation checkbox can gate dismissal until the user explicitly
//!   acknowledges they've recorded the phrase
//!
//! ## Usage
//!
//! ```ignore
//! // Minimum — just display + copy:
//! let out = MnemonicDisplay::new(&mnemonic).show(ui);
//! if out.copy_clicked { /* feedback */ }
//!
//! // With confirmation gate:
//! let mut confirmed = false;
//! let out = MnemonicDisplay::new(&mnemonic)
//!     .with_confirmation(&mut confirmed)
//!     .show(ui);
//! if confirmed { /* let the user click "Continue" */ }
//! ```
//!
//! The widget owns no state across frames; the caller persists the bool
//! it passes to [`with_confirmation`]. That keeps the widget composable
//! with parent dialogs / modals that already manage flow state.

use egui::{Frame, RichText, Stroke, Ui};

use crate::theme::{Radius, Space, SpaceExt, ThemeExt, with_alpha};

/// Layout settings the consumer can tweak before rendering.
#[derive(Debug, Clone, Copy)]
pub struct MnemonicDisplayStyle {
    /// Number of columns in the words grid. v1 default: 4 (works for 12 / 16 /
    /// 20 / 24 word phrases; renders any other count with a ragged last row).
    pub columns: usize,
    /// Width allocated to each word cell. Constrains the widget so it
    /// doesn't grow unboundedly inside a wide parent.
    pub cell_width: f32,
    /// Show the warning banner above the words. On by default — turn off
    /// only if the parent already carries equivalent messaging.
    pub show_warning: bool,
}

impl Default for MnemonicDisplayStyle {
    fn default() -> Self {
        Self {
            columns: 4,
            cell_width: 110.0,
            show_warning: true,
        }
    }
}

/// Builder for the mnemonic display.
pub struct MnemonicDisplay<'a> {
    mnemonic: &'a str,
    confirmed: Option<&'a mut bool>,
    style: MnemonicDisplayStyle,
}

impl<'a> MnemonicDisplay<'a> {
    /// Construct a display with default styling. `mnemonic` is the
    /// whitespace-separated BIP-39 phrase; the widget splits it into words
    /// without validating word count or BIP-39 wordlist membership (caller
    /// is responsible for handing us a valid phrase).
    pub fn new(mnemonic: &'a str) -> Self {
        Self {
            mnemonic,
            confirmed: None,
            style: MnemonicDisplayStyle::default(),
        }
    }

    /// Render a confirmation checkbox below the words. When the user ticks
    /// it, `*confirmed` becomes `true`. Parent flow gates "Continue" on
    /// this. Skip the call to render without a checkbox at all.
    pub fn with_confirmation(mut self, confirmed: &'a mut bool) -> Self {
        self.confirmed = Some(confirmed);
        self
    }

    /// Override the layout style.
    pub fn with_style(mut self, style: MnemonicDisplayStyle) -> Self {
        self.style = style;
        self
    }

    pub fn show(self, ui: &mut Ui) -> MnemonicDisplayOutput {
        let words: Vec<&str> = self.mnemonic.split_whitespace().collect();
        let style = self.style;

        let mut output = MnemonicDisplayOutput::default();

        // ── Warning banner ───────────────────────────────────────────────
        if style.show_warning {
            let c = ui.tokens().color;
            Frame::new()
                // A tinted card, not a filled one: the panel is a caution, and a
                // solid amber block under this much prose is unreadable.
                .fill(with_alpha(c.warning, 28))
                .stroke(Stroke::new(1.0_f32, c.warning.gamma_multiply(0.6)))
                .corner_radius(ui.tokens().corner(Radius::Base))
                .inner_margin(ui.tokens().margin_xy(Space::Xl, Space::Md))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        crate::icons::install_phosphor_font(ui.ctx());
                        let warn = c.warning;
                        ui.label(crate::PhosphorIcon::Warning.rich_text(13.0, warn));
                        ui.label(
                            RichText::new("This phrase is shown ONCE. Write it down.")
                                .color(warn)
                                .strong(),
                        );
                    });
                    ui.label(
                        RichText::new(
                            "Anyone with these words controls the wallet. Store offline; \
                             never paste into chat, email, or screenshots.",
                        )
                        .color(c.text_secondary)
                        .small(),
                    );
                });
            ui.gap(Space::Lg);
        }

        // ── Words grid ───────────────────────────────────────────────────
        Frame::new()
            .fill(ui.tokens().color.bg_primary)
            .stroke(ui.tokens().geometry.border(ui.tokens().color.border))
            .corner_radius(ui.tokens().corner(Radius::Md))
            .inner_margin(ui.tokens().margin(Space::Xl2))
            .show(ui, |ui| {
                let cols = style.columns.max(1);
                let rows = words.len().div_ceil(cols);
                for r in 0..rows {
                    ui.horizontal(|ui| {
                        for c in 0..cols {
                            let idx = r * cols + c;
                            if let Some(word) = words.get(idx) {
                                render_word_cell(ui, idx + 1, word, style.cell_width);
                            }
                        }
                    });
                }
            });

        ui.gap(Space::Lg);

        // ── Copy + confirm row ───────────────────────────────────────────
        ui.horizontal(|ui| {
            let copy_resp = ui.button("📋  Copy to clipboard");
            if copy_resp.clicked() {
                ui.ctx().copy_text(self.mnemonic.to_string());
                output.copy_clicked = true;
            }
            ui.label(
                RichText::new(
                    "(prefer writing it down — clipboard contents can leak to other apps)",
                )
                .color(ui.tokens().color.text_muted)
                .small(),
            );
        });

        if let Some(confirmed) = self.confirmed {
            ui.gap(Space::Md);
            ui.checkbox(
                confirmed,
                RichText::new("I have securely recorded this recovery phrase").strong(),
            );
            output.confirmed = *confirmed;
        }

        output
    }
}

/// What happened while the widget was on screen this frame. Caller drives
/// parent-level flow off these (e.g. show a "Copied!" toast when
/// `copy_clicked` flips true; enable "Continue" button when `confirmed`).
#[derive(Debug, Clone, Copy, Default)]
pub struct MnemonicDisplayOutput {
    /// User clicked the copy button this frame.
    pub copy_clicked: bool,
    /// User has the confirmation checkbox ticked. Always `false` if the
    /// caller didn't supply [`MnemonicDisplay::with_confirmation`].
    pub confirmed: bool,
}

fn render_word_cell(ui: &mut Ui, number: usize, word: &str, width: f32) {
    Frame::new()
        .fill(ui.tokens().color.bg_secondary)
        .stroke(ui.tokens().geometry.border(ui.tokens().color.border))
        .corner_radius(ui.tokens().corner(Radius::Sm))
        .inner_margin(ui.tokens().margin_xy(Space::Md, Space::Base))
        .show(ui, |ui| {
            ui.set_min_width(width);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{number:>2}."))
                        .color(ui.tokens().color.text_muted)
                        .monospace()
                        .small(),
                );
                ui.label(RichText::new(word).monospace().strong());
            });
        });
}
