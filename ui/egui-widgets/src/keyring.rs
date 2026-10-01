//! `Keyring` — the API keys an app holds, one per provider.
//!
//! Rows of `(provider, key)`, each masked by default with a per-row reveal. The
//! widget is purely presentational: it does not persist, does not log, and never
//! writes a key anywhere — the caller decides where keys live (slopgen keeps them
//! in the browser's localStorage, so the key stays the reader's own).
//!
//! `Clear` empties a key rather than removing the row, so *which* providers the
//! app holds is the app's decision and not the reader's.

use egui::Ui;

use crate::theme::{TextSize, ThemeExt};

/// One provider's key.
#[derive(Debug, Clone, PartialEq)]
pub struct ApiKey {
    /// Display name, e.g. `fal` or `DeepSeek`.
    pub provider: String,
    /// One line on what it is for, e.g. "image generation".
    pub note: String,
    /// The secret. Empty means not set.
    pub key: String,
}

impl ApiKey {
    pub fn new(provider: impl Into<String>, note: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            note: note.into(),
            key: String::new(),
        }
    }
}

/// Cross-frame state: which rows the reader has chosen to reveal.
#[derive(Default)]
pub struct Keyring {
    revealed: Vec<bool>,
}

impl Keyring {
    /// Draw a row per key. Returns `true` when a key was edited or cleared.
    pub fn show(&mut self, ui: &mut Ui, keys: &mut [ApiKey]) -> bool {
        let Keyring { revealed } = self;
        revealed.resize(keys.len(), false);

        let mut changed = false;
        for (index, key) in keys.iter_mut().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(&key.provider)
                        .strong()
                        .size(ui.text_size(TextSize::Md)),
                );
                if !key.note.is_empty() {
                    ui.label(egui::RichText::new(&key.note).weak().small());
                }
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut key.key)
                            .password(!revealed[index])
                            .desired_width(300.0)
                            .hint_text("not set"),
                    )
                    .changed()
                {
                    changed = true;
                }
                ui.checkbox(&mut revealed[index], "show");
                if ui.button("Clear").clicked() {
                    key.key.clear();
                    changed = true;
                }
            });
        }

        changed
    }
}
