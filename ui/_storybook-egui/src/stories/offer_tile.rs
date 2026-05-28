use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::offer_tile::{OfferTile, OfferTileState};
use egui_widgets::theme;

/// Same placeholder hero we use in the GroupedSection story —
/// avoids hauling in another asset just for the tile preview.
const PLACEHOLDER_IMAGE: egui::ImageSource<'_> =
    egui::include_image!("../../assets/placeholders/section_hero_64.png");

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Offer Tile").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Picker tile with a state machine (Active / InCart / Spent), an image \
             or text-placeholder content area, and a top-right corner badge for \
             quantity / multi-asset hints. Sized fixed so wrapping rows align.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("Image content — three states")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        let r = OfferTile::image("5.0 ADA", PLACEHOLDER_IMAGE.clone())
            .state(OfferTileState::Active)
            .tooltip("Click to add cancellation to cart")
            .show(ui);
        if r.clicked() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("offer_tile_clicked"), true));
        }
        OfferTile::image("5.0 ADA", PLACEHOLDER_IMAGE.clone())
            .state(OfferTileState::InCart)
            .tooltip("Already queued for cancel in your cart")
            .show(ui);
        OfferTile::image("5.0 ADA", PLACEHOLDER_IMAGE.clone())
            .state(OfferTileState::Spent)
            .tooltip("Already spent on chain")
            .show(ui);
    });
    let was_clicked = ui
        .ctx()
        .data_mut(|d| d.get_temp::<bool>(egui::Id::new("offer_tile_clicked")))
        .unwrap_or(false);
    if was_clicked {
        ui.label(
            egui::RichText::new("→ click registered (consumer dispatches cart-add)")
                .color(theme::ACCENT_GREEN)
                .size(10.0),
        );
    }
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("Image with corner badge (multi-asset hint)")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        OfferTile::image("5.0 ADA", PLACEHOLDER_IMAGE.clone())
            .badge("+2")
            .tooltip("3-asset offer — first asset shown")
            .show(ui);
        OfferTile::image("5.0 ADA", PLACEHOLDER_IMAGE.clone())
            .state(OfferTileState::InCart)
            .badge("+2")
            .show(ui);
    });
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("Placeholder content (collection offer stack)")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        OfferTile::placeholder("5.0 ADA", "CO")
            .badge("8×")
            .tooltip("8 collection offers at this price — click to queue one")
            .show(ui);
        OfferTile::placeholder("10.0 ADA", "CO")
            .badge("3×")
            .show(ui);
        OfferTile::placeholder("100.0 ADA", "CO")
            .badge("1×")
            .show(ui);
        OfferTile::placeholder("5.0 ADA", "CO")
            .state(OfferTileState::InCart)
            .badge("8")
            .tooltip("All 8 already queued in cart")
            .show(ui);
    });
    ui.add_space(16.0);

    // ---------------------------------------------------------------
    ui.label(
        egui::RichText::new("Mixed row — image and placeholder side by side")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Tiles share an outer size so rows align cleanly even when content varies.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        OfferTile::image("420.0 ADA", PLACEHOLDER_IMAGE.clone()).show(ui);
        OfferTile::image("150.0 ADA", PLACEHOLDER_IMAGE.clone()).show(ui);
        OfferTile::placeholder("7.0 ADA", "CO").badge("9×").show(ui);
        OfferTile::placeholder("5.0 ADA", "CO")
            .badge("12×")
            .show(ui);
        OfferTile::image("420.0 ADA", PLACEHOLDER_IMAGE.clone())
            .state(OfferTileState::Spent)
            .show(ui);
    });

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(egui::RichText::new("Test cases:").color(ACCENT).strong());
    ui.label("\u{2022} Active tile renders full-colour with hover cursor + click events");
    ui.label("\u{2022} InCart / Spent tiles dim the frame and tint the image; clicks are inert");
    ui.label("\u{2022} Tooltip is consumer-supplied; usually disambiguates dimmed states");
    ui.label("\u{2022} Top-right badge overlays the content area without reserving extra height");
    ui.label("\u{2022} Placeholder tiles paint centred glyph text in lieu of an image");
    ui.label("\u{2022} Tiles are uniform-sized so `horizontal_wrapped` rows line up cleanly");
}
