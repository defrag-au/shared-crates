//! Storybook demo for the TradeTable widget — TCG-style top/bottom layout
//! with lock/unlock mechanics.

use egui::Color32;
use egui_widgets::offer_slot::OfferSlotData;
use egui_widgets::trade_table::{
    self, LockState, PeerState, TradeOffer, TradeTableConfig, TradeTableState,
};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

const POLICY_ID: &str = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

// A handful of real Hodlcroft Pirates hex asset names for demo thumbnails.
const YOUR_PIRATES: &[(&str, u32)] = &[
    ("5069726174653834", 42),
    ("506972617465323733", 273),
    ("50697261746531303430", 891),
];

const THEIR_PIRATES: &[(&str, u32)] = &[
    ("506972617465333830", 15),
    ("506972617465313432", 2103),
    ("506972617465393336", 777),
    ("50697261746531323736", 156),
];

fn decode_hex_name(hex: &str) -> String {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect();
    String::from_utf8(bytes).unwrap_or_else(|_| hex.to_string())
}

fn make_slot(hex: &str, rank: u32, accent: Color32) -> OfferSlotData {
    OfferSlotData {
        name: decode_hex_name(hex),
        policy_id: POLICY_ID.into(),
        asset_name_hex: hex.into(),
        rarity_rank: Some(rank),
        total_ranked: Some(2000),
        accent,
        quantity: 1,
        is_fungible: false,
        wallet_balance: None,
    }
}

pub struct TradeTableStoryState {
    pub your_offer: TradeOffer,
    pub their_offer: TradeOffer,
    pub peer_state: PeerState,
    pub lock_state: LockState,
    pub table_state: TradeTableState,
    pub last_action: String,
    next_idx: usize,
}

impl Default for TradeTableStoryState {
    fn default() -> Self {
        Self {
            your_offer: TradeOffer {
                assets: YOUR_PIRATES
                    .iter()
                    .map(|(hex, rank)| make_slot(hex, *rank, egui_widgets::theme::ACCENT_GREEN))
                    .collect(),
                lovelace: 25_000_000,
            },
            their_offer: TradeOffer {
                assets: THEIR_PIRATES
                    .iter()
                    .map(|(hex, rank)| make_slot(hex, *rank, egui_widgets::theme::ACCENT_CYAN))
                    .collect(),
                lovelace: 0,
            },
            peer_state: PeerState::Connected,
            lock_state: LockState::default(),
            table_state: TradeTableState::default(),
            last_action: String::new(),
            next_idx: YOUR_PIRATES.len(),
        }
    }
}

// Extra pirates for the "add" action.
const EXTRA_PIRATES: &[(&str, u32)] = &[
    ("506972617465373835", 500),
    ("50697261746531393133", 1200),
    ("50697261746531363133", 88),
    ("506972617465333138", 950),
];

pub fn show(ui: &mut egui::Ui, state: &mut TradeTableStoryState) {
    ui.label(
        egui::RichText::new("TradeTable Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "TCG-style top/bottom trade layout with lock/unlock, \
             ADA sweetener, and real IIIF thumbnails.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(8.0);

    // Controls row
    ui.horizontal(|ui| {
        // Peer state toggle
        ui.label(
            egui::RichText::new("Peer:")
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

        ui.add_space(12.0);

        // Simulate peer lock toggle
        ui.label(
            egui::RichText::new("Peer locked:")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );
        if ui
            .selectable_label(state.lock_state.they_locked, "Yes")
            .clicked()
        {
            state.lock_state.they_locked = true;
        }
        if ui
            .selectable_label(!state.lock_state.they_locked, "No")
            .clicked()
        {
            state.lock_state.they_locked = false;
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
                &mut state.table_state,
                &state.your_offer,
                &state.their_offer,
                &state.peer_state,
                &state.lock_state,
                &config,
            );

            if let Some(action) = resp.action {
                match action {
                    trade_table::TradeTableAction::AddAsset => {
                        state.last_action = "Add asset clicked".into();
                        let extra_idx = state.next_idx % EXTRA_PIRATES.len();
                        let (hex, rank) = EXTRA_PIRATES[extra_idx];
                        state.your_offer.assets.push(make_slot(
                            hex,
                            rank,
                            egui_widgets::theme::ACCENT_GREEN,
                        ));
                        state.next_idx += 1;
                    }
                    trade_table::TradeTableAction::RemoveYourAsset(idx) => {
                        state.last_action = format!(
                            "Removed: [{}] \"{}\"",
                            idx,
                            state.your_offer.assets.get(idx).map_or("?", |d| &d.name)
                        );
                        if idx < state.your_offer.assets.len() {
                            state.your_offer.assets.remove(idx);
                        }
                    }
                    trade_table::TradeTableAction::SetYourLovelace(lovelace) => {
                        state.last_action = format!("ADA sweetener: {lovelace} lovelace");
                        state.your_offer.lovelace = lovelace;
                    }
                    trade_table::TradeTableAction::Lock => {
                        state.last_action = "Locked your offer".into();
                        state.lock_state.you_locked = true;
                    }
                    trade_table::TradeTableAction::Unlock => {
                        state.last_action = "Unlocked — both sides reset".into();
                        state.lock_state.you_locked = false;
                        state.lock_state.they_locked = false;
                    }
                    trade_table::TradeTableAction::SetYourAssetQuantity { index, quantity } => {
                        state.last_action = format!("Set asset [{index}] quantity to {quantity}");
                        if let Some(asset) = state.your_offer.assets.get_mut(index) {
                            asset.quantity = quantity;
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
                "No actions yet -- try adding/removing assets, locking, or adjusting ADA",
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

    // Summary
    let lock_label = match (state.lock_state.you_locked, state.lock_state.they_locked) {
        (true, true) => "BOTH LOCKED",
        (true, false) => "You locked",
        (false, true) => "They locked",
        (false, false) => "Unlocked",
    };
    let your_ada = state.your_offer.lovelace as f64 / 1_000_000.0;
    let their_ada = state.their_offer.lovelace as f64 / 1_000_000.0;
    ui.label(
        egui::RichText::new(format!(
            "Your: {} assets + {your_ada:.1} ADA | Theirs: {} assets + {their_ada:.1} ADA | {lock_label}",
            state.your_offer.assets.len(),
            state.their_offer.assets.len()
        ))
        .color(TEXT_MUTED)
        .size(10.0),
    );

    ui.add_space(12.0);
    if ui.button("Reset to mock data").clicked() {
        *state = TradeTableStoryState::default();
    }
}
