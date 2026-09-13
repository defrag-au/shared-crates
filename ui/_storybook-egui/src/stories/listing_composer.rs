//! Storybook demo for the ListingComposer widget — pricing a batch of
//! listings by what the buyer pays, with the fee and the seller's payout
//! shown per row and in total.
//!
//! **The thing to look at:** the 10 ₳ row. Its fee is the FLOOR, not 5%,
//! because the fee output cannot carry less than the ledger's min-UTxO, so
//! the seller receives 8.84 ₳ of a 10 ₳ quote. The 52.63 ₳ row is the other
//! side of the same arithmetic: a seller who wants exactly 50 ₳ quotes 52.63
//! and gets 50.00. Nothing here is rounded for display — every figure is the
//! contract's own integer arithmetic, so the build agrees to the lovelace.

use egui_widgets::listing_composer::{
    self, ComposerRow, FeeFormula, ListingComposerAction, ListingComposerConfig,
    ListingComposerState,
};

use crate::{accent, muted};

pub struct ListingComposerStoryState {
    pub composer: ListingComposerState,
    pub config: ListingComposerConfig,
    pub last_action: String,
}

impl Default for ListingComposerStoryState {
    fn default() -> Self {
        let mut composer = ListingComposerState::default();
        composer.add(
            ComposerRow::new("perp-0011", "Perp 0011", "3f6c8e0a…3ab3c8").with_price_ada(52.631578),
        );
        composer.add(
            ComposerRow::new("perp-0012", "Perp 0012", "3f6c8e0a…3ab3c8").with_price_ada(10.0),
        );
        composer.add(ComposerRow::new(
            "perp-0013",
            "Perp 0013",
            "3f6c8e0a…3ab3c8",
        ));
        composer.add(
            ComposerRow::new("abandon-t", "$ABANDON", "c0ffee00…d00d")
                .with_quantity(2_500)
                .with_price_ada(1.5),
        );
        Self {
            composer,
            config: ListingComposerConfig {
                formula: FeeFormula::GrossPercent { pct: 5 },
                fee_floor_lovelace: 1_155_080,
                min_buyer_price_lovelace: 2_000_000,
                ..ListingComposerConfig::default()
            },
            last_action: String::new(),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut ListingComposerStoryState) {
    ui.label(
        egui::RichText::new("ListingComposer Widget")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Price by what the buyer pays; the widget shows the contract's fee and the \
             seller's payout per row. The 10 ₳ row pays the fee floor, not 5%; the 1.5 ₳ \
             row is below the host's minimum; the unpriced row blocks the build.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Fee %").color(muted(ui)).size(11.0));
        let mut pct = match state.config.formula {
            FeeFormula::GrossPercent { pct } => pct,
            FeeFormula::JpgV2 => 2,
        };
        if ui
            .add(egui::Slider::new(&mut pct, 1..=20).suffix("%"))
            .changed()
        {
            state.config.formula = FeeFormula::GrossPercent { pct };
        }
        ui.add_space(12.0);
        if ui.button("Add a row").clicked() {
            let n = state.composer.rows.len() + 1;
            state.composer.add(ComposerRow::new(
                format!("row-{n}"),
                format!("Perp {n:04}"),
                "3f6c8e0a…3ab3c8",
            ));
        }
        if ui.button("Reset").clicked() {
            *state = ListingComposerStoryState::default();
        }
    });
    ui.add_space(8.0);

    let action = ui
        .allocate_ui(egui::vec2(560.0_f32.min(ui.available_width()), 0.0), |ui| {
            listing_composer::show(ui, &mut state.composer, &state.config)
        })
        .inner;
    if let Some(action) = action {
        state.last_action = match action {
            ListingComposerAction::PriceChanged { id } => format!("price changed: {id}"),
            ListingComposerAction::Removed { id } => format!("removed: {id}"),
            ListingComposerAction::Cleared => "cleared".to_string(),
            ListingComposerAction::AppliedToAll {
                buyer_price_lovelace,
            } => format!("applied {buyer_price_lovelace} lovelace to all"),
        };
    }

    ui.add_space(10.0);
    let ready = state.composer.all_ready(&state.config);
    ui.label(
        egui::RichText::new(format!(
            "all_ready: {ready} · last action: {}",
            if state.last_action.is_empty() {
                "—"
            } else {
                &state.last_action
            }
        ))
        .color(muted(ui))
        .size(11.0),
    );
}
