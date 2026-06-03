//! `SupplyBar` story — the two-band mint supply bar across every state it has to
//! handle on the dashboard: empty, minting, a big ordered backlog, near-sold-out,
//! sold out, and oversubscribed (ordered demand exceeding the remaining supply).

use egui_widgets::SupplyBar;

use crate::{ACCENT, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Supply Bar").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Two bands: minted (fulfilled, left) then the ordered backlog, over the \
             unsold track. Oversubscription is tinted, not silently clamped.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(10.0);

    // (label, minted, ordered, total)
    let cases: [(&str, u64, u64, u64); 6] = [
        ("Empty — no orders yet", 0, 0, 1250),
        ("Minting, no backlog", 120, 0, 1250),
        ("Minting with backlog (~450 orders)", 120, 900, 1250),
        ("Near sold out", 1180, 60, 1250),
        ("Sold out", 1250, 0, 1250),
        ("Oversubscribed — demand > supply", 120, 2000, 1250),
    ];

    for (label, minted, ordered, total) in cases {
        let oversub = total > 0 && minted.saturating_add(ordered) > total;
        let pct = if total > 0 {
            minted as f32 / total as f32 * 100.0
        } else {
            0.0
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label).small());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if oversub {
                    ui.label(
                        egui::RichText::new("OVERSUBSCRIBED")
                            .small()
                            .color(egui::Color32::from_rgb(210, 120, 90)),
                    );
                }
                ui.label(
                    egui::RichText::new(format!("{minted} / {total}  ({pct:.0}%)"))
                        .small()
                        .color(TEXT_MUTED),
                );
            });
        });
        ui.add_space(2.0);
        SupplyBar::new(minted, ordered, total).height(6.0).show(ui);
        ui.add_space(12.0);
    }
}
