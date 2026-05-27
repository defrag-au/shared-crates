//! Story: `MnemonicDisplay` — the moment-of-truth widget for showing a
//! freshly-generated BIP-39 phrase exactly once.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::mnemonic_display::MnemonicDisplay;

#[derive(Default)]
pub struct MnemonicDisplayState {
    pub copied_24: bool,
    pub confirmed_24: bool,
    pub copied_12: bool,
}

const SAMPLE_24: &str = "abandon ability able about above absent absorb abstract absurd abuse \
                         access accident account accuse achieve acid acoustic acquire across \
                         act action actor actress actual";

const SAMPLE_12: &str =
    "legal winner thank year wave sausage worth useful legal winner thank yellow";

pub fn show(ui: &mut egui::Ui, state: &mut MnemonicDisplayState) {
    ui.label(
        egui::RichText::new(
            "BIP-39 mnemonic display — shown once when provisioning a new client or on \
             GDPR Art. 20 export.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(16.0);

    // ── Variant 1: 24 words + confirmation gate ────────────────────────
    ui.label(
        egui::RichText::new("24-word phrase, with confirmation gate")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "The default provisioning flow: parent renders this then enables a \"Continue\" \
             button only once `confirmed` flips true.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let out = MnemonicDisplay::new(SAMPLE_24)
        .with_confirmation(&mut state.confirmed_24)
        .show(ui);
    if out.copy_clicked {
        state.copied_24 = true;
    }
    if state.copied_24 {
        ui.add_space(4.0);
        ui.colored_label(egui::Color32::LIGHT_GREEN, "✓ copied");
    }

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 2: 12 words, no confirmation ───────────────────────────
    ui.label(
        egui::RichText::new("12-word phrase, no confirmation gate")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "For lower-stakes flows where the parent already has a confirmation step \
             (e.g. an export modal that closes on its own dismiss button).",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let out = MnemonicDisplay::new(SAMPLE_12).show(ui);
    if out.copy_clicked {
        state.copied_12 = true;
    }
    if state.copied_12 {
        ui.add_space(4.0);
        ui.colored_label(egui::Color32::LIGHT_GREEN, "✓ copied");
    }

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 3: warning banner suppressed ───────────────────────────
    ui.label(
        egui::RichText::new("Banner suppressed (parent provides messaging)")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Use when the surrounding modal already carries the warning text, to avoid \
             duplicate copy.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let style = egui_widgets::mnemonic_display::MnemonicDisplayStyle {
        show_warning: false,
        ..Default::default()
    };
    let _ = MnemonicDisplay::new(SAMPLE_24).with_style(style).show(ui);
}
