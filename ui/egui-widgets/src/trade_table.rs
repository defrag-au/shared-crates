//! Trade table widget — TCG-style top/bottom offer display for the trade desk.
//!
//! Renders a stacked layout: "THEIR OFFER" at the top (remote party, read-only),
//! "YOUR OFFER" at the bottom (local party, interactive). Each side shows NFT
//! assets as [`OfferSlotData`] cards plus an optional ADA sweetener amount.
//!
//! The vertical layout is mobile-friendly — your offer sits near the bottom of
//! the screen, close to your thumbs.

use egui::{RichText, Vec2};

use crate::icons::PhosphorIcon;
use crate::offer_slot::{self, OfferSlotConfig, OfferSlotData};
use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// A complete offer: NFT assets + optional ADA sweetener.
#[derive(Clone, Debug, Default)]
pub struct TradeOffer {
    /// NFT assets in this offer.
    pub assets: Vec<OfferSlotData>,
    /// ADA sweetener in lovelace (0 = no sweetener).
    pub lovelace: u64,
}

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
    /// User changed their ADA sweetener amount (new value in lovelace).
    SetYourLovelace(u64),
}

/// Response from rendering the trade table.
pub struct TradeTableResponse {
    pub action: Option<TradeTableAction>,
}

// ============================================================================
// Persistent state
// ============================================================================

/// Mutable state owned by the caller, persists across frames.
#[derive(Default)]
pub struct TradeTableState {
    /// Text buffer for the ADA input field.
    pub ada_input: String,
}

// ============================================================================
// Widget
// ============================================================================

/// Render the trade table in top/bottom TCG layout.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut TradeTableState,
    your_offer: &TradeOffer,
    their_offer: &TradeOffer,
    peer_state: &PeerState,
    config: &TradeTableConfig,
) -> TradeTableResponse {
    crate::install_phosphor_font(ui.ctx());

    let mut action = None;

    // ── Their offer (top) ──────────────────────────────────────────────
    draw_offer_panel(
        ui,
        config.their_heading,
        their_offer,
        config,
        false,
        peer_state,
        &mut action,
    );

    // ── Divider ────────────────────────────────────────────────────────
    ui.add_space(4.0);
    let avail_width = ui.available_width();
    let divider_rect = ui.allocate_space(Vec2::new(avail_width, 20.0)).1;
    let painter = ui.painter();

    // Horizontal line
    let mid_y = divider_rect.center().y;
    painter.line_segment(
        [
            egui::pos2(divider_rect.min.x, mid_y),
            egui::pos2(divider_rect.max.x, mid_y),
        ],
        egui::Stroke::new(1.0, theme::BG_HIGHLIGHT),
    );

    // Swap icon in center
    let icon_text = PhosphorIcon::ArrowsDownUp.codepoint().to_string();
    painter.text(
        divider_rect.center(),
        egui::Align2::CENTER_CENTER,
        &icon_text,
        egui::FontId::new(14.0, crate::icons::phosphor_family()),
        theme::TEXT_MUTED,
    );
    ui.add_space(4.0);

    // ── Your offer (bottom) ────────────────────────────────────────────
    draw_offer_panel(
        ui,
        config.your_heading,
        your_offer,
        config,
        true,
        &PeerState::Connected,
        &mut action,
    );

    // ── ADA sweetener (your side) ──────────────────────────────────────
    ui.add_space(6.0);
    draw_ada_sweetener(
        ui,
        state,
        your_offer.lovelace,
        their_offer.lovelace,
        &mut action,
    );

    TradeTableResponse { action }
}

// ============================================================================
// Internals
// ============================================================================

fn draw_offer_panel(
    ui: &mut egui::Ui,
    heading: &str,
    offer: &TradeOffer,
    config: &TradeTableConfig,
    is_local: bool,
    peer_state: &PeerState,
    action: &mut Option<TradeTableAction>,
) {
    let heading_color = if is_local {
        theme::ACCENT_GREEN
    } else {
        theme::ACCENT_CYAN
    };

    // Heading row
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(heading)
                .color(heading_color)
                .size(config.heading_size)
                .strong(),
        );

        // Remote ADA sweetener (read-only, shown inline with heading)
        if !is_local && offer.lovelace > 0 {
            let ada = offer.lovelace as f64 / 1_000_000.0;
            ui.label(
                RichText::new(format!("+ {ada:.1} ADA"))
                    .color(theme::ACCENT_YELLOW)
                    .size(config.heading_size),
            );
        }
    });
    ui.add_space(4.0);

    // Empty state for remote side waiting for peer
    if !is_local && *peer_state == PeerState::WaitingForPeer {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(
                RichText::new("Waiting for peer...")
                    .color(theme::TEXT_MUTED)
                    .size(10.0),
            );
        });
        ui.add_space(12.0);
        return;
    }

    // Asset cards in flow layout
    let slot_config = OfferSlotConfig {
        width: config.card_width,
        thumb_height: config.card_thumb_height,
        removable: is_local && config.can_remove,
        ..OfferSlotConfig::default()
    };

    if offer.assets.is_empty() {
        if is_local {
            // Empty local side — just show the add button below
        } else {
            ui.add_space(8.0);
            ui.label(
                RichText::new("No assets offered yet")
                    .color(theme::TEXT_MUTED)
                    .size(10.0),
            );
            ui.add_space(8.0);
        }
    } else {
        let cards_per_row = ((ui.available_width()) / (config.card_width + 6.0)).floor() as usize;
        let cards_per_row = cards_per_row.max(1);

        for (chunk_idx, chunk) in offer.assets.chunks(cards_per_row).enumerate() {
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
        ui.horizontal(|ui| {
            let btn = ui.add(
                egui::Button::new(
                    RichText::new(format!("{}  Add asset", PhosphorIcon::Plus.codepoint()))
                        .family(crate::icons::phosphor_family())
                        .color(theme::ACCENT_CYAN)
                        .size(11.0),
                )
                .frame(false),
            );
            if btn.clicked() {
                *action = Some(TradeTableAction::AddAsset);
            }
        });
    }
}

/// Draw the ADA sweetener row — editable for your side, read-only display for theirs.
fn draw_ada_sweetener(
    ui: &mut egui::Ui,
    state: &mut TradeTableState,
    your_lovelace: u64,
    _their_lovelace: u64,
    action: &mut Option<TradeTableAction>,
) {
    ui.horizontal(|ui| {
        // Coins icon
        ui.label(PhosphorIcon::Coins.rich_text(14.0, theme::ACCENT_YELLOW));

        ui.label(
            RichText::new("ADA")
                .color(theme::ACCENT_YELLOW)
                .size(10.0)
                .strong(),
        );

        // Initialize the input buffer from the current value if empty
        if state.ada_input.is_empty() && your_lovelace > 0 {
            let ada = your_lovelace as f64 / 1_000_000.0;
            state.ada_input = format!("{ada:.1}");
        }

        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.ada_input)
                .desired_width(80.0)
                .hint_text("0")
                .font(egui::FontId::monospace(11.0)),
        );

        // Parse and emit on change
        if resp.changed() {
            let trimmed = state.ada_input.trim();
            if trimmed.is_empty() || trimmed == "0" {
                *action = Some(TradeTableAction::SetYourLovelace(0));
            } else if let Ok(ada) = trimmed.parse::<f64>() {
                let lovelace = (ada * 1_000_000.0) as u64;
                *action = Some(TradeTableAction::SetYourLovelace(lovelace));
            }
        }
    });
}
