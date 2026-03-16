//! Offer slot widget — a single asset card placed on a trade table.
//!
//! Square card with full-bleed IIIF thumbnail and a semi-transparent name
//! overlay at the bottom. Hover tooltip shows rarity rank and additional
//! details. Also provides ADA sweetener and "add asset" card variants.

use egui::{Color32, CornerRadius, Vec2};

use crate::card_browser;
use crate::icons::PhosphorIcon;
use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// Data for a single offered asset.
#[derive(Clone, Debug)]
pub struct OfferSlotData {
    /// Asset display name (e.g. "Pirate84").
    pub name: String,
    /// Policy ID hex (for IIIF URL construction).
    pub policy_id: String,
    /// Asset name hex (for IIIF URL construction).
    pub asset_name_hex: String,
    /// Optional rarity rank (e.g. 42 of 5000).
    pub rarity_rank: Option<u32>,
    /// Total ranked assets in collection (for rarity coloring).
    pub total_ranked: Option<u32>,
    /// Accent color for the card border.
    pub accent: Color32,
}

impl OfferSlotData {
    /// Build the IIIF thumbnail URL for this asset.
    pub fn thumbnail_url(&self, size: u32) -> String {
        format!(
            "https://iiif.hodlcroft.com/iiif/3/{}:{}/full/{size},/0/default.jpg",
            self.policy_id, self.asset_name_hex
        )
    }
}

/// Configuration for offer slot appearance.
pub struct OfferSlotConfig {
    /// Card size (width = height, square).
    pub size: f32,
    /// Font size for the name overlay.
    pub font_size: f32,
    /// Whether to show the remove button on hover.
    pub removable: bool,
}

impl Default for OfferSlotConfig {
    fn default() -> Self {
        Self {
            size: 100.0,
            font_size: 9.0,
            removable: true,
        }
    }
}

/// Action emitted by an offer slot.
#[derive(Debug)]
pub enum OfferSlotAction {
    /// User clicked the remove button.
    Remove,
    /// User clicked the "add asset" placeholder card.
    Add,
}

/// Response from rendering an offer slot.
pub struct OfferSlotResponse {
    pub action: Option<OfferSlotAction>,
}

// ============================================================================
// Asset card (square)
// ============================================================================

/// Render a single NFT offer slot as a square card.
///
/// The thumbnail fills the entire card. The asset name is rendered as a
/// semi-transparent overlay banner at the bottom. Rarity rank and details
/// appear in a hover tooltip.
pub fn show(
    ui: &mut egui::Ui,
    data: &OfferSlotData,
    config: &OfferSlotConfig,
) -> OfferSlotResponse {
    crate::install_phosphor_font(ui.ctx());

    let mut action = None;
    let card_size = Vec2::splat(config.size);

    let (card_rect, card_response) = ui.allocate_exact_size(card_size, egui::Sense::click());

    let hovered = card_response.hovered();
    let painter = ui.painter_at(card_rect);
    let rounding = CornerRadius::same(4);

    // Card background (visible while image loads)
    painter.rect_filled(card_rect, rounding, theme::BG_SECONDARY);

    // Full-bleed thumbnail
    let image_url = data.thumbnail_url(400);
    let browser_config = crate::CardBrowserConfig {
        rounding: 4.0,
        bg_card_hover: Color32::from_rgb(40, 40, 55),
        ..Default::default()
    };
    let loading = card_browser::draw_thumbnail(ui, card_rect, Some(&image_url), &browser_config);
    if loading {
        crate::image_loader::CachedSpinner::request_repaint(ui);
    }

    // Name overlay banner at the bottom
    let banner_h = 18.0;
    let banner_rect = egui::Rect::from_min_size(
        egui::pos2(card_rect.min.x, card_rect.max.y - banner_h),
        Vec2::new(config.size, banner_h),
    );
    painter.rect_filled(
        banner_rect,
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: 4,
            se: 4,
        },
        Color32::from_rgba_premultiplied(15, 15, 25, 200),
    );
    let name_rect = egui::Rect::from_min_size(
        egui::pos2(banner_rect.min.x + 4.0, banner_rect.min.y),
        Vec2::new(config.size - 8.0, banner_h),
    );
    painter.with_clip_rect(name_rect).text(
        name_rect.left_center(),
        egui::Align2::LEFT_CENTER,
        &data.name,
        egui::FontId::monospace(config.font_size),
        theme::TEXT_PRIMARY,
    );

    // Rarity border — always visible, color based on rank percentile
    let border_color = if let Some(rank) = data.rarity_rank {
        let total = data.total_ranked.unwrap_or(10000);
        theme::rarity_rank_color(rank, total)
    } else {
        theme::BG_HIGHLIGHT
    };
    let border_width = if hovered { 2.0 } else { 1.5 };
    painter.rect_stroke(
        card_rect,
        rounding,
        egui::Stroke::new(border_width, border_color),
        egui::StrokeKind::Inside,
    );

    // Tooltip with details
    card_response.clone().on_hover_ui(|ui| {
        ui.label(
            egui::RichText::new(&data.name)
                .color(theme::TEXT_PRIMARY)
                .size(11.0)
                .strong(),
        );
        if let Some(rank) = data.rarity_rank {
            let total = data.total_ranked.unwrap_or(0);
            let rank_color = theme::rarity_rank_color(rank, total);
            ui.label(
                egui::RichText::new(format!("Rank #{rank} / {total}"))
                    .color(rank_color)
                    .size(10.0),
            );
        }
    });

    // Remove button overlay (top-right)
    if config.removable && hovered {
        let btn_center = egui::pos2(card_rect.max.x - 10.0, card_rect.min.y + 10.0);

        painter.circle_filled(
            btn_center,
            9.0,
            Color32::from_rgba_premultiplied(20, 20, 30, 220),
        );

        let cursor_in_btn = card_response
            .hover_pos()
            .is_some_and(|p| p.distance(btn_center) <= 9.0);

        let x_color = if cursor_in_btn {
            theme::ACCENT_RED
        } else {
            theme::TEXT_SECONDARY
        };

        painter.text(
            btn_center,
            egui::Align2::CENTER_CENTER,
            PhosphorIcon::X.codepoint().to_string(),
            egui::FontId::new(10.0, crate::icons::phosphor_family()),
            x_color,
        );

        if card_response.clicked() && cursor_in_btn {
            action = Some(OfferSlotAction::Remove);
        }
    }

    OfferSlotResponse { action }
}

// ============================================================================
// ADA sweetener card
// ============================================================================

/// Render an ADA amount as a square card slot.
///
/// On the local side, shows an editable text field. On the remote side,
/// shows a read-only amount.
pub fn show_ada_card(
    ui: &mut egui::Ui,
    lovelace: u64,
    ada_input: Option<&mut String>,
    config: &OfferSlotConfig,
) -> Option<u64> {
    crate::install_phosphor_font(ui.ctx());

    let card_size = Vec2::splat(config.size);
    let (card_rect, _) = ui.allocate_exact_size(card_size, egui::Sense::hover());

    let painter = ui.painter_at(card_rect);
    painter.rect_filled(card_rect, CornerRadius::same(4), theme::BG_SECONDARY);

    // Large coins icon
    let icon_center = egui::pos2(card_rect.center().x, card_rect.center().y - 14.0);
    painter.text(
        icon_center,
        egui::Align2::CENTER_CENTER,
        PhosphorIcon::Coins.codepoint().to_string(),
        egui::FontId::new(28.0, crate::icons::phosphor_family()),
        theme::ACCENT_YELLOW,
    );

    let mut new_lovelace = None;

    if let Some(input) = ada_input {
        // Editable: text input
        let input_rect = egui::Rect::from_min_size(
            egui::pos2(card_rect.min.x + 4.0, card_rect.center().y + 4.0),
            Vec2::new(config.size - 8.0, 18.0),
        );

        if input.is_empty() && lovelace > 0 {
            let ada = lovelace as f64 / 1_000_000.0;
            *input = format!("{ada:.1}");
        }

        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(input_rect));
        let resp = child.add(
            egui::TextEdit::singleline(input)
                .desired_width(input_rect.width())
                .hint_text("0")
                .horizontal_align(egui::Align::Center)
                .font(egui::FontId::monospace(11.0)),
        );

        if resp.changed() {
            let trimmed = input.trim();
            if trimmed.is_empty() || trimmed == "0" {
                new_lovelace = Some(0);
            } else if let Ok(ada) = trimmed.parse::<f64>() {
                new_lovelace = Some((ada * 1_000_000.0) as u64);
            }
        }

        // "ADA" label below input
        painter.text(
            egui::pos2(card_rect.center().x, card_rect.center().y + 28.0),
            egui::Align2::CENTER_CENTER,
            "ADA",
            egui::FontId::monospace(config.font_size),
            theme::ACCENT_YELLOW,
        );
    } else {
        // Read-only amount
        let ada = lovelace as f64 / 1_000_000.0;
        let amount_text = if lovelace == 0 {
            "0".to_string()
        } else {
            format!("{ada:.1}")
        };
        painter.text(
            egui::pos2(card_rect.center().x, card_rect.center().y + 6.0),
            egui::Align2::CENTER_CENTER,
            &amount_text,
            egui::FontId::monospace(14.0),
            theme::TEXT_PRIMARY,
        );
        painter.text(
            egui::pos2(card_rect.center().x, card_rect.center().y + 24.0),
            egui::Align2::CENTER_CENTER,
            "ADA",
            egui::FontId::monospace(config.font_size),
            theme::ACCENT_YELLOW,
        );
    }

    new_lovelace
}

// ============================================================================
// Add-asset placeholder card
// ============================================================================

/// Render a dashed-border "add asset" placeholder as a square card.
///
/// Returns `true` if clicked.
pub fn show_add_card(ui: &mut egui::Ui, config: &OfferSlotConfig) -> bool {
    crate::install_phosphor_font(ui.ctx());

    let card_size = Vec2::splat(config.size);
    let (card_rect, response) = ui.allocate_exact_size(card_size, egui::Sense::click());

    let hovered = response.hovered();
    let painter = ui.painter_at(card_rect);
    let rounding = CornerRadius::same(4);

    // Border and fill
    let (border_color, icon_color, text_color) = if hovered {
        (theme::ACCENT_CYAN, theme::ACCENT_CYAN, theme::TEXT_PRIMARY)
    } else {
        (theme::TEXT_MUTED, theme::TEXT_MUTED, theme::TEXT_MUTED)
    };

    if hovered {
        painter.rect_filled(card_rect, rounding, theme::BG_HIGHLIGHT);
    }

    draw_dashed_rect(&painter, card_rect, 4.0, border_color);

    // Plus icon centered
    painter.text(
        egui::pos2(card_rect.center().x, card_rect.center().y - 6.0),
        egui::Align2::CENTER_CENTER,
        PhosphorIcon::Plus.codepoint().to_string(),
        egui::FontId::new(24.0, crate::icons::phosphor_family()),
        icon_color,
    );

    // "Add" label
    painter.text(
        egui::pos2(card_rect.center().x, card_rect.center().y + 14.0),
        egui::Align2::CENTER_CENTER,
        "Add",
        egui::FontId::monospace(9.0),
        text_color,
    );

    response.clicked()
}

/// Draw a dashed rectangle outline.
fn draw_dashed_rect(painter: &egui::Painter, rect: egui::Rect, rounding: f32, color: Color32) {
    let stroke = egui::Stroke::new(1.0, color);
    let dash = 5.0;
    let gap = 3.0;

    let edges: [(egui::Pos2, egui::Pos2); 4] = [
        (
            egui::pos2(rect.min.x + rounding, rect.min.y),
            egui::pos2(rect.max.x - rounding, rect.min.y),
        ),
        (
            egui::pos2(rect.max.x, rect.min.y + rounding),
            egui::pos2(rect.max.x, rect.max.y - rounding),
        ),
        (
            egui::pos2(rect.max.x - rounding, rect.max.y),
            egui::pos2(rect.min.x + rounding, rect.max.y),
        ),
        (
            egui::pos2(rect.min.x, rect.max.y - rounding),
            egui::pos2(rect.min.x, rect.min.y + rounding),
        ),
    ];

    for (start, end) in edges {
        let dir = end - start;
        let len = dir.length();
        if len < 0.1 {
            continue;
        }
        let norm = dir / len;
        let mut pos = 0.0;
        while pos < len {
            let seg_end = (pos + dash).min(len);
            painter.line_segment([start + norm * pos, start + norm * seg_end], stroke);
            pos = seg_end + gap;
        }
    }
}
