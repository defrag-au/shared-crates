use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::fungibles_row::FungiblesRow;

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Fungibles Row").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Compact single-row display for a Cardano Native Token holding. \
             Optional ticker chip and ADA value. Quantity is right-aligned.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("Wallet-viewer CNT list (real-shaped data)")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    FungiblesRow::new("IAGON", "3,365,232,987")
        .ticker(Some("IAG"))
        .show(ui);
    FungiblesRow::new("NikePig", "5,158,556")
        .ticker(Some("Nike"))
        .show(ui);
    FungiblesRow::new("Donut", "366,124,244")
        .ticker(Some("DONUT"))
        .show(ui);
    FungiblesRow::new("PERP COIN", "30,146,605")
        .ticker(Some("PERP"))
        .show(ui);
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("With ADA value attached")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    FungiblesRow::new("IAGON", "3,365.23")
        .ticker(Some("IAG"))
        .value_text(Some("≈ 1,247 ₳"))
        .show(ui);
    FungiblesRow::new("SNEK", "12,890.5")
        .ticker(Some("SNEK"))
        .value_text(Some("≈ 84 ₳"))
        .show(ui);
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("No ticker (unregistered token)")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    FungiblesRow::new("Some Obscure Token", "1,234").show(ui);
}
