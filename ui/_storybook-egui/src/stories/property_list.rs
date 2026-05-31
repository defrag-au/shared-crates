//! `PropertyList` storybook story.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{PropertyLabelAlign, PropertyList};

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("PropertyList").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Label/value grid for read-only key data. Two columns, stable widths, \
             muted label colour, default text colour for the value. Used wherever \
             you'd otherwise hand-build an egui::Grid for descriptive metadata.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ── Phase-card use case ────────────────────────────────────────────
    ui.label(
        egui::RichText::new("Phase summary (default)")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    PropertyList::new()
        .id("story_phase_props")
        .add("Price", "FREE")
        .add("Window", "2026-06-01T09:00:00 → unbounded")
        .add("Per wallet", "3")
        .show(ui);

    ui.add_space(16.0);

    // ── add_optional + right-aligned labels ────────────────────────────
    ui.label(
        egui::RichText::new("Right-aligned labels + optional rows")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Right alignment suits dense layouts where the value column is the \
             scanning axis. `add_optional` keeps optional rows out of the layout \
             without a noisy match at the call site.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    let refund: Option<&str> = Some("0.50 ADA");
    let mint_tx: Option<&str> = None;
    PropertyList::new()
        .id("story_props_right")
        .label_align(PropertyLabelAlign::Right)
        .add("Status", "refund_queued")
        .add("Paid", "1.50 ADA")
        .add_optional("Refund", refund)
        .add_optional("Mint tx", mint_tx)
        .show(ui);

    ui.add_space(16.0);

    // ── Wallet card use case ───────────────────────────────────────────
    ui.label(
        egui::RichText::new("Wallet card readout")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    PropertyList::new()
        .id("story_props_wallet")
        .add("Account index", "0")
        .add("Role", "ClientMaster")
        .add("Fuel UTxOs", "20")
        .add("Total balance", "230 ADA")
        .show(ui);
}
