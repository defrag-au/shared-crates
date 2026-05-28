use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::persona_strip::PersonaStrip;

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Persona Strip").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Italic one-liner + optional chip row. Persona summaries (wallet, collection, \
             holder) all follow this shape — a deterministic phrase derived from tags.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("Headline only — wallet-viewer's current shape")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    PersonaStrip::new("ADA-leaning Large with blue-chip taste").show(ui);
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("Headline + chips — same data with structured tags")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    PersonaStrip::new("ADA-leaning Large with blue-chip taste")
        .tags(&["ada_maximalist", "large", "specialist", "blue_chip_leaning"])
        .show(ui);
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("Many chips — wraps onto multiple rows")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    PersonaStrip::new("Eclectic Mid-tier NFT collector with long-tail taste, pixel-leaning")
        .tags(&[
            "nft_collector",
            "mid_tier",
            "eclectic",
            "long_tail",
            "pixel_leaning",
            "high_diversity",
            "experimental",
        ])
        .show(ui);
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("Empty headline, just chips")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    PersonaStrip::new("").tags(&["whale", "ada_maxi"]).show(ui);
}
