//! `Keyring` story — the app's API keys, one row per provider.

use egui_widgets::keyring::{ApiKey, Keyring};

use crate::{accent, muted};

pub struct KeyringState {
    pub keyring: Keyring,
    pub keys: Vec<ApiKey>,
}

impl Default for KeyringState {
    fn default() -> Self {
        let mut keys = vec![
            ApiKey::new("fal", "image generation"),
            ApiKey::new("DeepSeek", "the agent"),
            ApiKey::new("x.ai", "alt model"),
        ];
        // A fixture only — enough to show a set row beside unset ones.
        keys[0].key = "fal-0000000000000000000000000000".into();
        Self {
            keyring: Keyring::default(),
            keys,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut KeyringState) {
    ui.label(egui::RichText::new("Keyring").color(accent(ui)).strong());
    ui.label(
        egui::RichText::new(
            "The API keys an app holds, one per provider. Masked by default; `show` \
             reveals a row, `Clear` empties it. The widget neither logs nor persists \
             anything — the app decides where keys live.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    if ui.button("Reset").clicked() {
        *state = KeyringState::default();
    }
    ui.add_space(8.0);

    let KeyringState { keyring, keys } = state;
    keyring.show(ui, keys);
}
