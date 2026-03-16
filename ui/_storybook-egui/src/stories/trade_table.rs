//! Storybook demo for the TradeTable widget (includes OfferSlot).

use egui::Color32;
use egui_widgets::offer_slot::OfferSlotData;
use egui_widgets::trade_table::{self, PeerState, TradeTableConfig};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

pub struct TradeTableStoryState {
    pub your_offer: Vec<OfferSlotData>,
    pub their_offer: Vec<OfferSlotData>,
    pub peer_state: PeerState,
    pub last_action: String,
}

impl Default for TradeTableStoryState {
    fn default() -> Self {
        Self {
            your_offer: vec![
                OfferSlotData {
                    name: "Alien Fren #1234".into(),
                    thumbnail_url: None,
                    rarity_rank: Some(42),
                    accent: egui_widgets::theme::ACCENT_GREEN,
                },
                OfferSlotData {
                    name: "Alien Fren #5678".into(),
                    thumbnail_url: None,
                    rarity_rank: Some(891),
                    accent: egui_widgets::theme::ACCENT_GREEN,
                },
            ],
            their_offer: vec![
                OfferSlotData {
                    name: "Alien Fren #9012".into(),
                    thumbnail_url: None,
                    rarity_rank: Some(15),
                    accent: egui_widgets::theme::ACCENT_CYAN,
                },
                OfferSlotData {
                    name: "Alien Fren #3456".into(),
                    thumbnail_url: None,
                    rarity_rank: Some(2103),
                    accent: egui_widgets::theme::ACCENT_CYAN,
                },
                OfferSlotData {
                    name: "Alien Fren #7777".into(),
                    thumbnail_url: None,
                    rarity_rank: Some(777),
                    accent: egui_widgets::theme::ACCENT_CYAN,
                },
            ],
            peer_state: PeerState::Connected,
            last_action: String::new(),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut TradeTableStoryState) {
    ui.label(
        egui::RichText::new("TradeTable Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Two-column trade offer layout with OfferSlot cards. \
             Your side has add/remove controls, their side is read-only.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(8.0);

    // Peer state toggle
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Peer state:")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        if ui
            .selectable_label(state.peer_state == PeerState::Connected, "Connected")
            .clicked()
        {
            state.peer_state = PeerState::Connected;
        }
        if ui
            .selectable_label(state.peer_state == PeerState::WaitingForPeer, "Waiting")
            .clicked()
        {
            state.peer_state = PeerState::WaitingForPeer;
        }
    });

    ui.add_space(12.0);

    // Trade table
    egui::Frame::new()
        .fill(BG_MAIN)
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
        .show(ui, |ui| {
            let config = TradeTableConfig::default();
            let resp = trade_table::show(
                ui,
                &state.your_offer,
                &state.their_offer,
                &state.peer_state,
                &config,
            );

            if let Some(action) = resp.action {
                match action {
                    trade_table::TradeTableAction::AddAsset => {
                        state.last_action = "Add asset clicked".into();
                        // Simulate adding a new asset
                        state.your_offer.push(OfferSlotData {
                            name: format!("Alien Fren #{}", 1000 + state.your_offer.len()),
                            thumbnail_url: None,
                            rarity_rank: Some(999),
                            accent: egui_widgets::theme::ACCENT_GREEN,
                        });
                    }
                    trade_table::TradeTableAction::RemoveYourAsset(idx) => {
                        state.last_action = format!(
                            "Remove: [{}] \"{}\"",
                            idx,
                            state.your_offer.get(idx).map_or("?", |d| &d.name)
                        );
                        if idx < state.your_offer.len() {
                            state.your_offer.remove(idx);
                        }
                    }
                }
            }
        });

    ui.add_space(12.0);

    // Action log
    if state.last_action.is_empty() {
        ui.label(
            egui::RichText::new(
                "No actions yet \u{2014} try adding or removing assets from your offer",
            )
            .color(TEXT_MUTED)
            .size(11.0),
        );
    } else {
        ui.label(
            egui::RichText::new(format!("Last action: {}", state.last_action))
                .color(Color32::from_rgb(125, 207, 255))
                .size(11.0),
        );
    }

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(format!(
            "Your: {} assets | Theirs: {} assets",
            state.your_offer.len(),
            state.their_offer.len()
        ))
        .color(TEXT_MUTED)
        .size(10.0),
    );

    ui.add_space(12.0);
    if ui.button("Reset to mock data").clicked() {
        *state = TradeTableStoryState::default();
    }
}
