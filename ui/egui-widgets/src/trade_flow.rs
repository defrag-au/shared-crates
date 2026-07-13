//! Trade-flow widget — the local user's view of a P2P swap in plain
//! give / get / net terms, decoupled from the raw eUTxO structure.
//!
//! The point of this widget is legibility: a raw wallet view makes "changing
//! hands" and "passing through" look identical, which is what confuses users
//! (and makes a hardware wallet report an inflated "send"). This widget shows
//! what actually leaves and arrives, states the net, and — in a collapsible
//! detail — names the pass-through amounts explicitly so the scary numbers a
//! Ledger shows become explained ones.
//!
//! Companion to [`crate::tx_estimate`] (pre-lock cost estimate) and
//! [`crate::fee_report`] (two-party fee breakdown). Driven by a plain view-model
//! ([`TradeFlowData`]) so callers assemble it from their own wire types rather
//! than decoding transaction CBOR.

use egui::RichText;

use crate::chip::{Chip, ChipVariant};
use crate::icons::PhosphorIcon;
use crate::theme;
use crate::utils::format_lovelace;

// ============================================================================
// Types
// ============================================================================

/// One asset moving in the trade, ready for display.
pub struct FlowAsset {
    /// Human-readable label (e.g. "SpaceBud #7812").
    pub name: String,
    /// Quantity — rendered as `×N` when greater than 1.
    pub quantity: u64,
}

impl FlowAsset {
    /// Convenience constructor for a single-quantity asset.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            quantity: 1,
        }
    }
}

/// The local user's view of a trade. Assemble from your own session/fee types;
/// all lovelace amounts are raw lovelace.
pub struct TradeFlowData {
    /// Assets leaving the local wallet for the peer.
    pub you_give: Vec<FlowAsset>,
    /// ADA leaving the local wallet (payment / sweetener), lovelace.
    pub you_give_ada: u64,
    /// Assets arriving from the peer.
    pub you_get: Vec<FlowAsset>,
    /// ADA arriving from the peer, lovelace.
    pub you_get_ada: u64,
    /// Net ADA impact on the local wallet (positive = gains, negative = loses).
    pub net_ada: i64,
    /// The local user's share of the network fee, lovelace.
    pub network_fee: u64,
    /// Min-UTxO the local user funds for the assets they receive — locked in
    /// their own wallet with the asset (recoverable), not a true loss. Lovelace.
    pub min_utxo_locked: u64,
    /// The peer's own ADA that passes through the transaction and returns to
    /// them as change. Appears as a "send" on a hardware wallet but is not the
    /// local user's cost. Lovelace.
    pub peer_passthrough_ada: u64,
    /// Display label for the peer (e.g. "$djo").
    pub peer_label: String,
}

/// Display configuration.
pub struct TradeFlowConfig {
    /// Font size for line-item text.
    pub font_size: f32,
    /// Font size for the heading.
    pub heading_size: f32,
}

impl Default for TradeFlowConfig {
    fn default() -> Self {
        Self {
            font_size: 12.0,
            heading_size: 12.0,
        }
    }
}

// ============================================================================
// Widget
// ============================================================================

/// Render the trade-flow card. The caller supplies the surrounding frame.
pub fn show(ui: &mut egui::Ui, data: &TradeFlowData, config: &TradeFlowConfig) {
    crate::install_phosphor_font(ui.ctx());

    // Heading
    ui.horizontal(|ui| {
        ui.label(PhosphorIcon::Handshake.rich_text(config.heading_size + 1.0, theme::ACCENT));
        ui.label(
            RichText::new("TRADE FLOW")
                .color(theme::TEXT_SECONDARY)
                .size(config.heading_size)
                .strong(),
        );
    });
    ui.add_space(8.0);

    // You give
    flow_row(
        ui,
        "You give",
        theme::ACCENT_RED,
        ChipVariant::Danger,
        &data.you_give,
        data.you_give_ada,
        &format!("Leaves your wallet -> {}", data.peer_label),
        config,
    );

    // A small downward hand-off cue between the two sides.
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(PhosphorIcon::ArrowDown.rich_text(config.font_size, theme::TEXT_MUTED));
    });
    ui.add_space(2.0);

    // You get
    flow_row(
        ui,
        "You get",
        theme::ACCENT_GREEN,
        ChipVariant::Success,
        &data.you_get,
        data.you_get_ada,
        &format!("Arrives from {}", data.peer_label),
        config,
    );

    ui.add_space(8.0);
    separator(ui);
    ui.add_space(6.0);

    // Net line
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Net")
                .color(theme::TEXT_MUTED)
                .size(config.font_size)
                .strong(),
        );
        ui.add_space(8.0);
        let (sign, color) = if data.net_ada >= 0 {
            ("+", theme::ACCENT_GREEN)
        } else {
            ("", theme::ACCENT_RED)
        };
        ui.label(
            RichText::new(format!("{sign}{}", format_lovelace(data.net_ada)))
                .color(color)
                .size(config.font_size + 1.0)
                .strong(),
        );
    });

    // Collapsible "what else is in this transaction" — names the pass-through so
    // the numbers a hardware wallet shows are explained, not surprising.
    ui.add_space(6.0);
    egui::CollapsingHeader::new(
        RichText::new("What else is in this transaction")
            .color(theme::TEXT_MUTED)
            .size(config.font_size - 1.0),
    )
    .id_salt("trade_flow_detail")
    .default_open(false)
    .show(ui, |ui| {
        detail_line(
            ui,
            "Network fee (your share)",
            &format_lovelace(data.network_fee as i64),
            None,
            config,
        );
        if data.min_utxo_locked > 0 {
            detail_line(
                ui,
                "Min-UTxO you lock with received assets",
                &format_lovelace(data.min_utxo_locked as i64),
                Some("Stays in your wallet with the asset — recoverable when you next move it."),
                config,
            );
        }
        if data.peer_passthrough_ada > 0 {
            let hover = format!(
                "Cardano can't move an asset without spending the whole UTxO it sits in. The ADA \
                 locked alongside the assets {} sends is spent and returned to them as change — \
                 rebalancing within the transaction, not a transfer to or from you.",
                data.peer_label
            );
            detail_line(
                ui,
                &format!("UTxO rebalancing ({})", data.peer_label),
                &format_lovelace(data.peer_passthrough_ada as i64),
                Some(&hover),
                config,
            );
        }

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(PhosphorIcon::Warning.rich_text(config.font_size - 1.0, theme::WARNING));
            ui.label(
                RichText::new(
                    "A hardware wallet shows raw UTxO movement, so it reports this rebalancing as \
                     part of a larger \"send\". Your actual net is shown above.",
                )
                .color(theme::TEXT_MUTED)
                .size(config.font_size - 2.0)
                .italics(),
            );
        });
    });
}

// ============================================================================
// Internal helpers
// ============================================================================

/// A "You give / You get" row: a label, then wrapped chips for each asset plus
/// an ADA chip when non-zero. Renders "— nothing —" when the side is empty.
#[allow(clippy::too_many_arguments)]
fn flow_row(
    ui: &mut egui::Ui,
    label: &str,
    label_color: egui::Color32,
    chip_variant: ChipVariant,
    assets: &[FlowAsset],
    ada_lovelace: u64,
    hover: &str,
    config: &TradeFlowConfig,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            RichText::new(label)
                .color(label_color)
                .size(config.font_size)
                .strong(),
        );

        if assets.is_empty() && ada_lovelace == 0 {
            ui.label(
                RichText::new("— nothing —")
                    .color(theme::TEXT_MUTED)
                    .size(config.font_size)
                    .italics(),
            );
            return;
        }

        for asset in assets {
            let text = if asset.quantity > 1 {
                format!("{} ×{}", asset.name, asset.quantity)
            } else {
                asset.name.clone()
            };
            Chip::new(&text)
                .variant(chip_variant)
                .on_hover_text(hover)
                .show(ui);
        }

        if ada_lovelace > 0 {
            Chip::new(&format_lovelace(ada_lovelace as i64))
                .variant(chip_variant)
                .on_hover_text(hover)
                .show(ui);
        }
    });
}

/// A muted `label … value` detail line with an optional hover explainer.
fn detail_line(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    hover: Option<&str>,
    config: &TradeFlowConfig,
) {
    ui.horizontal(|ui| {
        let resp = ui.label(
            RichText::new(label)
                .color(theme::TEXT_MUTED)
                .size(config.font_size - 1.0),
        );
        if let Some(h) = hover {
            resp.on_hover_text(h);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(value)
                    .color(theme::TEXT_SECONDARY)
                    .size(config.font_size - 1.0),
            );
        });
    });
}

fn separator(ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    let y = rect.min.y;
    ui.painter().line_segment(
        [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
        egui::Stroke::new(1.0_f32, theme::BORDER),
    );
    ui.add_space(1.0);
}
