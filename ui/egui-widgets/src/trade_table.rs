//! Trade table widget — TCG-style top/bottom offer display for the trade desk.
//!
//! A live workspace where both parties build their offers in real time.
//! Each side shows NFT assets, an ADA sweetener card, and (while unlocked)
//! an "add asset" placeholder card.
//!
//! ## Lock mechanics
//!
//! Either party can **lock** their side to signal "I'm happy with this offer."
//! Once locked, that side's cards become read-only (no add/remove/edit).
//! When both sides are locked the trade is ready to sign.
//!
//! Unlocking resets *both* locks — if you change your mind after locking,
//! both parties need to re-confirm since the deal has changed.
//!
//! The vertical layout is mobile-friendly — your offer sits near the bottom
//! of the screen, close to your thumbs.

use egui::{Color32, CornerRadius, RichText, Vec2};

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

/// Lock state for the trade — drives editability and progression.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockState {
    /// Whether the local party has locked their offer.
    pub you_locked: bool,
    /// Whether the remote party has locked their offer.
    pub they_locked: bool,
}

impl LockState {
    /// Both sides locked — ready to proceed to signing.
    pub fn both_locked(self) -> bool {
        self.you_locked && self.they_locked
    }
}

/// Configuration for the trade table.
pub struct TradeTableConfig {
    /// Heading for the local side (e.g. "$handle (You)").
    pub your_heading: String,
    /// Heading for the remote side (e.g. "$handle" or truncated stake).
    pub their_heading: String,
    /// Card size (square, width = height).
    pub card_size: f32,
    /// Font size for headings.
    pub heading_size: f32,
}

impl Default for TradeTableConfig {
    fn default() -> Self {
        Self {
            your_heading: "YOUR OFFER".into(),
            their_heading: "THEIR OFFER".into(),
            card_size: 90.0,
            heading_size: 11.0,
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
    /// User changed a fungible token quantity at the given offer index.
    SetYourAssetQuantity { index: usize, quantity: u64 },
    /// User locked their offer.
    Lock,
    /// User unlocked their offer (resets both locks).
    Unlock,
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
    /// Per-slot text buffers for FT quantity editing (keyed by offer index).
    pub asset_qty_inputs: std::collections::HashMap<usize, String>,
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
    lock_state: &LockState,
    config: &TradeTableConfig,
) -> TradeTableResponse {
    crate::install_phosphor_font(ui.ctx());

    let mut action = None;
    let you_locked = lock_state.you_locked;
    let they_locked = lock_state.they_locked;

    let slot_config = OfferSlotConfig {
        size: config.card_size,
        ..OfferSlotConfig::default()
    };

    // ── Their offer (top) ──────────────────────────────────────────────
    draw_offer_heading(ui, &config.their_heading, theme::ACCENT_CYAN, they_locked);

    if *peer_state == PeerState::WaitingForPeer {
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
    } else {
        draw_card_row(
            ui,
            &their_offer.assets,
            their_offer.lovelace,
            &OfferSlotConfig {
                removable: false,
                ..slot_config
            },
            CardRowOptions {
                is_local: false,
                show_add: false,
                ada_input: None,
                asset_qty_inputs: None,
            },
            &mut action,
        );
    }

    // ── Divider ────────────────────────────────────────────────────────
    ui.add_space(4.0);
    draw_divider(ui, lock_state);
    ui.add_space(4.0);

    // ── Your offer (bottom) ────────────────────────────────────────────
    draw_offer_heading(ui, &config.your_heading, theme::ACCENT_GREEN, you_locked);

    draw_card_row(
        ui,
        &your_offer.assets,
        your_offer.lovelace,
        &OfferSlotConfig {
            removable: !you_locked,
            ..slot_config
        },
        CardRowOptions {
            is_local: true,
            show_add: !you_locked,
            ada_input: if you_locked {
                None
            } else {
                Some(&mut state.ada_input)
            },
            asset_qty_inputs: if you_locked {
                None
            } else {
                Some(&mut state.asset_qty_inputs)
            },
        },
        &mut action,
    );

    // ── Lock/Unlock button ─────────────────────────────────────────────
    ui.add_space(8.0);
    draw_lock_button(ui, lock_state, &mut action);

    TradeTableResponse { action }
}

// ============================================================================
// Internals
// ============================================================================

fn draw_offer_heading(ui: &mut egui::Ui, heading: &str, color: Color32, locked: bool) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(heading).color(color).size(11.0).strong());
        if locked {
            ui.label(PhosphorIcon::Lock.rich_text(11.0, theme::ACCENT_YELLOW));
        }
    });
    ui.add_space(4.0);
}

fn draw_divider(ui: &mut egui::Ui, lock_state: &LockState) {
    let avail_width = ui.available_width();
    let divider_rect = ui.allocate_space(Vec2::new(avail_width, 20.0)).1;
    let painter = ui.painter();

    let mid_y = divider_rect.center().y;

    // Line color reflects state
    let line_color = if lock_state.both_locked() {
        theme::ACCENT_GREEN
    } else {
        theme::BG_HIGHLIGHT
    };

    painter.line_segment(
        [
            egui::pos2(divider_rect.min.x, mid_y),
            egui::pos2(divider_rect.max.x, mid_y),
        ],
        egui::Stroke::new(1.0, line_color),
    );

    // Center icon: handshake when both locked, arrows otherwise
    let (icon, icon_color) = if lock_state.both_locked() {
        (PhosphorIcon::Handshake, theme::ACCENT_GREEN)
    } else {
        (PhosphorIcon::ArrowsDownUp, theme::TEXT_MUTED)
    };

    // Background circle so icon doesn't sit on the line
    painter.circle_filled(divider_rect.center(), 12.0, theme::BG_PRIMARY);
    icon.paint(
        painter,
        divider_rect.center(),
        egui::Align2::CENTER_CENTER,
        14.0,
        icon_color,
    );
}

fn draw_lock_button(
    ui: &mut egui::Ui,
    lock_state: &LockState,
    action: &mut Option<TradeTableAction>,
) {
    if lock_state.both_locked() {
        // Both locked — show status, offer unlock
        ui.horizontal(|ui| {
            ui.label(PhosphorIcon::CheckCircle.rich_text(14.0, theme::ACCENT_GREEN));
            ui.label(
                RichText::new("Both sides locked — ready to sign")
                    .color(theme::ACCENT_GREEN)
                    .size(10.0),
            );
            if ui
                .add(
                    egui::Button::new(RichText::new("Unlock").color(theme::TEXT_MUTED).size(10.0))
                        .fill(theme::BG_SECONDARY)
                        .corner_radius(CornerRadius::same(4)),
                )
                .clicked()
            {
                *action = Some(TradeTableAction::Unlock);
            }
        });
    } else if lock_state.you_locked {
        // You locked, waiting for them
        ui.horizontal(|ui| {
            ui.label(PhosphorIcon::Lock.rich_text(14.0, theme::ACCENT_YELLOW));
            ui.label(
                RichText::new("Your offer is locked — waiting for peer")
                    .color(theme::ACCENT_YELLOW)
                    .size(10.0),
            );
            if ui
                .add(
                    egui::Button::new(RichText::new("Unlock").color(theme::TEXT_MUTED).size(10.0))
                        .fill(theme::BG_SECONDARY)
                        .corner_radius(CornerRadius::same(4)),
                )
                .clicked()
            {
                *action = Some(TradeTableAction::Unlock);
            }
        });
    } else {
        // Not locked — show prominent lock button
        let (label, btn_color) = if lock_state.they_locked {
            ("Lock Offer to Proceed", theme::ACCENT_GREEN)
        } else {
            ("Lock Offer", theme::ACCENT_CYAN)
        };

        let btn = egui::Button::new(
            RichText::new(label)
                .color(theme::BG_PRIMARY)
                .size(12.0)
                .strong(),
        )
        .fill(btn_color)
        .corner_radius(CornerRadius::same(6))
        .min_size(egui::Vec2::new(180.0, 36.0));

        if ui.add(btn).clicked() {
            *action = Some(TradeTableAction::Lock);
        }
    }
}

/// Options for a single card row.
struct CardRowOptions<'a> {
    is_local: bool,
    show_add: bool,
    ada_input: Option<&'a mut String>,
    asset_qty_inputs: Option<&'a mut std::collections::HashMap<usize, String>>,
}

/// Draw a flow row of: [asset cards...] [ADA card] [+ add card].
fn draw_card_row(
    ui: &mut egui::Ui,
    assets: &[OfferSlotData],
    lovelace: u64,
    slot_config: &OfferSlotConfig,
    opts: CardRowOptions<'_>,
    action: &mut Option<TradeTableAction>,
) {
    let CardRowOptions {
        is_local,
        show_add,
        ada_input,
        mut asset_qty_inputs,
    } = opts;

    // Total cards: assets + ada card + maybe add card
    let total_cards = assets.len() + 1 + if show_add { 1 } else { 0 };
    let cards_per_row = ((ui.available_width()) / (slot_config.size + 6.0))
        .floor()
        .max(1.0) as usize;

    let mut card_idx = 0;
    let mut ada_input = ada_input;

    while card_idx < total_cards {
        let row_end = (card_idx + cards_per_row).min(total_cards);
        ui.horizontal(|ui| {
            for i in card_idx..row_end {
                if i < assets.len() {
                    // For local fungible slots, provide a qty input buffer
                    let qty_input = if assets[i].is_fungible {
                        asset_qty_inputs
                            .as_deref_mut()
                            .map(|m| m.entry(i).or_default() as &mut String)
                    } else {
                        None
                    };
                    let resp = offer_slot::show(ui, &assets[i], slot_config, qty_input);
                    match resp.action {
                        Some(offer_slot::OfferSlotAction::Remove) => {
                            *action = Some(TradeTableAction::RemoveYourAsset(i));
                        }
                        Some(offer_slot::OfferSlotAction::SetQuantity(q)) => {
                            *action = Some(TradeTableAction::SetYourAssetQuantity {
                                index: i,
                                quantity: q,
                            });
                        }
                        _ => {}
                    }
                } else if i == assets.len() {
                    let input = if is_local {
                        ada_input.as_deref_mut()
                    } else {
                        None
                    };
                    if let Some(new_lovelace) =
                        offer_slot::show_ada_card(ui, lovelace, input, slot_config)
                    {
                        *action = Some(TradeTableAction::SetYourLovelace(new_lovelace));
                    }
                } else if offer_slot::show_add_card(ui, slot_config) {
                    *action = Some(TradeTableAction::AddAsset);
                }
            }
        });
        ui.add_space(4.0);
        card_idx = row_end;
    }

    // Empty remote side message
    if !is_local && assets.is_empty() && lovelace == 0 {
        ui.label(
            RichText::new("No assets offered yet")
                .color(theme::TEXT_MUTED)
                .size(10.0),
        );
    }
}
