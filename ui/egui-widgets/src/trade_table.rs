//! Trade table widget — two-column offer display for the trade desk.
//!
//! Renders a split layout: "YOUR OFFER" on the left, "THEIR OFFER" on the
//! right. Each side shows placed assets as [`OfferSlotData`] cards with an
//! optional "Add asset" button on the local side.

use egui::{RichText, Vec2};

use crate::icons::PhosphorIcon;
use crate::offer_slot::{self, OfferSlotConfig, OfferSlotData};
use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// Configuration for the trade table.
pub struct TradeTableConfig {
    /// Heading for the local side.
    pub your_heading: &'static str,
    /// Heading for the remote side.
    pub their_heading: &'static str,
    /// Card width for offer slots.
    pub card_width: f32,
    /// Card thumbnail height.
    pub card_thumb_height: f32,
    /// Font size for headings.
    pub heading_size: f32,
    /// Whether the "Add asset" button is shown on the local side.
    pub can_add: bool,
    /// Whether the local side's assets are removable.
    pub can_remove: bool,
}

impl Default for TradeTableConfig {
    fn default() -> Self {
        Self {
            your_heading: "YOUR OFFER",
            their_heading: "THEIR OFFER",
            card_width: 90.0,
            card_thumb_height: 90.0,
            heading_size: 11.0,
            can_add: true,
            can_remove: true,
        }
    }
}

/// Peer connection state for the remote side.
#[derive(Clone, Debug, PartialEq)]
pub enum PeerState {
    /// Waiting for peer to connect.
    WaitingForPeer,
    /// Peer is connected, their offers are shown.
    Connected,
}

/// Action emitted by the trade table.
#[derive(Debug)]
pub enum TradeTableAction {
    /// User clicked "Add asset" on their side.
    AddAsset,
    /// User removed an asset from their offer at the given index.
    RemoveYourAsset(usize),
}

/// Response from rendering the trade table.
pub struct TradeTableResponse {
    pub action: Option<TradeTableAction>,
}

// ============================================================================
// Widget
// ============================================================================

/// Render the trade table.
pub fn show(
    ui: &mut egui::Ui,
    your_offer: &[OfferSlotData],
    their_offer: &[OfferSlotData],
    peer_state: &PeerState,
    config: &TradeTableConfig,
) -> TradeTableResponse {
    crate::install_phosphor_font(ui.ctx());

    let mut action = None;

    let total_width = ui.available_width();
    let half_width = (total_width - 8.0) / 2.0; // 8px gap between columns

    ui.horizontal(|ui| {
        // Left column — your offer
        ui.allocate_ui(Vec2::new(half_width, ui.available_height()), |ui| {
            draw_offer_column(
                ui,
                config.your_heading,
                your_offer,
                config,
                true,
                &PeerState::Connected,
                &mut action,
            );
        });

        // Divider
        let divider_rect = ui.available_rect_before_wrap();
        let top = divider_rect.min;
        let bottom = egui::pos2(top.x + 1.0, top.y + 300.0);
        ui.painter()
            .line_segment([top, bottom], egui::Stroke::new(1.0, theme::BG_HIGHLIGHT));
        ui.add_space(4.0);

        // Right column — their offer
        ui.allocate_ui(Vec2::new(half_width, ui.available_height()), |ui| {
            draw_offer_column(
                ui,
                config.their_heading,
                their_offer,
                config,
                false,
                peer_state,
                &mut action,
            );
        });
    });

    TradeTableResponse { action }
}

fn draw_offer_column(
    ui: &mut egui::Ui,
    heading: &str,
    offers: &[OfferSlotData],
    config: &TradeTableConfig,
    is_local: bool,
    peer_state: &PeerState,
    action: &mut Option<TradeTableAction>,
) {
    // Heading
    let heading_color = if is_local {
        theme::ACCENT_GREEN
    } else {
        theme::ACCENT_CYAN
    };
    ui.label(
        RichText::new(heading)
            .color(heading_color)
            .size(config.heading_size)
            .strong(),
    );
    ui.add_space(6.0);

    // Empty state for remote side
    if !is_local && *peer_state == PeerState::WaitingForPeer {
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(
                RichText::new("Waiting for peer...")
                    .color(theme::TEXT_MUTED)
                    .size(10.0),
            );
        });
        return;
    }

    // Asset cards in a flow layout
    let slot_config = OfferSlotConfig {
        width: config.card_width,
        thumb_height: config.card_thumb_height,
        removable: is_local && config.can_remove,
        ..OfferSlotConfig::default()
    };

    if offers.is_empty() && !is_local {
        ui.add_space(20.0);
        ui.label(
            RichText::new("No assets offered yet")
                .color(theme::TEXT_MUTED)
                .size(10.0),
        );
    } else {
        // Wrap cards in rows
        let cards_per_row = ((ui.available_width()) / (config.card_width + 6.0)).floor() as usize;
        let cards_per_row = cards_per_row.max(1);

        for (chunk_idx, chunk) in offers.chunks(cards_per_row).enumerate() {
            ui.horizontal(|ui| {
                for (offset, data) in chunk.iter().enumerate() {
                    let idx = chunk_idx * cards_per_row + offset;
                    let resp = offer_slot::show(ui, data, &slot_config);
                    if let Some(offer_slot::OfferSlotAction::Remove) = resp.action {
                        *action = Some(TradeTableAction::RemoveYourAsset(idx));
                    }
                }
            });
            ui.add_space(4.0);
        }
    }

    // Add button (local side only)
    if is_local && config.can_add {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let icon_resp = ui.add(
                egui::Button::new(PhosphorIcon::Plus.rich_text(12.0, theme::ACCENT_CYAN))
                    .frame(false),
            );
            let text_resp = ui.add(
                egui::Button::new(
                    RichText::new("Add asset")
                        .color(theme::ACCENT_CYAN)
                        .size(10.0),
                )
                .frame(false),
            );
            if icon_resp.clicked() || text_resp.clicked() {
                *action = Some(TradeTableAction::AddAsset);
            }
        });
    }
}
