//! Offer slot widget — a single asset card placed on a trade table.
//!
//! Compact card showing an async-loaded IIIF thumbnail, name, rarity rank,
//! and an optional remove button. Used inside [`super::trade_table`] to
//! represent assets that a party has placed on the trade desk.

use egui::{Color32, CornerRadius, Vec2};

use crate::card_browser;
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
    /// Card width.
    pub width: f32,
    /// Thumbnail height.
    pub thumb_height: f32,
    /// Font size for name label.
    pub font_size: f32,
    /// Font size for rank label.
    pub rank_font_size: f32,
    /// Whether to show the remove button.
    pub removable: bool,
}

impl Default for OfferSlotConfig {
    fn default() -> Self {
        Self {
            width: 100.0,
            thumb_height: 100.0,
            font_size: 9.0,
            rank_font_size: 8.0,
            removable: true,
        }
    }
}

/// Action emitted by an offer slot.
#[derive(Debug)]
pub enum OfferSlotAction {
    /// User clicked the remove button.
    Remove,
}

/// Response from rendering an offer slot.
pub struct OfferSlotResponse {
    pub action: Option<OfferSlotAction>,
    pub hovered: bool,
}

// ============================================================================
// Widget
// ============================================================================

/// Render a single offer slot card.
pub fn show(
    ui: &mut egui::Ui,
    data: &OfferSlotData,
    config: &OfferSlotConfig,
) -> OfferSlotResponse {
    crate::install_phosphor_font(ui.ctx());

    let mut action = None;

    let outer_rect = ui.available_rect_before_wrap();
    let card_rect = egui::Rect::from_min_size(
        outer_rect.min,
        Vec2::new(config.width, config.thumb_height + 28.0),
    );

    let response = ui.allocate_rect(card_rect, egui::Sense::hover());
    let hovered = response.hovered();

    let painter = ui.painter_at(card_rect);
    let rounding = CornerRadius::same(4);

    // Card background
    let bg = if hovered {
        theme::BG_HIGHLIGHT
    } else {
        theme::BG_SECONDARY
    };
    painter.rect_filled(card_rect, rounding, bg);
    painter.rect_stroke(
        card_rect,
        rounding,
        egui::Stroke::new(1.0, data.accent.linear_multiply(0.4)),
        egui::StrokeKind::Outside,
    );

    // Thumbnail area — use card_browser::draw_thumbnail for async loading + spinner
    let thumb_rect = egui::Rect::from_min_size(
        card_rect.min + egui::vec2(1.0, 1.0),
        Vec2::new(config.width - 2.0, config.thumb_height),
    );

    let image_url = data.thumbnail_url(400);
    let browser_config = crate::CardBrowserConfig {
        rounding: 3.0,
        bg_card_hover: Color32::from_rgb(40, 40, 55),
        ..Default::default()
    };
    let loading = card_browser::draw_thumbnail(ui, thumb_rect, Some(&image_url), &browser_config);
    if loading {
        crate::image_loader::CachedSpinner::request_repaint(ui);
    }

    // Name label (below thumbnail)
    let name_pos = egui::pos2(
        card_rect.min.x + 4.0,
        card_rect.min.y + config.thumb_height + 3.0,
    );
    let name_rect = egui::Rect::from_min_size(name_pos, Vec2::new(config.width - 8.0, 14.0));
    painter.with_clip_rect(name_rect).text(
        name_pos,
        egui::Align2::LEFT_TOP,
        &data.name,
        egui::FontId::monospace(config.font_size),
        theme::TEXT_PRIMARY,
    );

    // Rarity rank (bottom-right of name area) — colored by percentile
    if let Some(rank) = data.rarity_rank {
        let rank_pos = egui::pos2(
            card_rect.max.x - 4.0,
            card_rect.min.y + config.thumb_height + 3.0,
        );
        let rank_color = theme::rarity_rank_color(rank, data.total_ranked.unwrap_or(10000));
        painter.text(
            rank_pos,
            egui::Align2::RIGHT_TOP,
            format!("#{rank}"),
            egui::FontId::monospace(config.rank_font_size),
            rank_color,
        );
    }

    // Remove button (top-right corner, overlaid on thumbnail)
    if config.removable && hovered {
        let btn_center = egui::pos2(card_rect.max.x - 10.0, card_rect.min.y + 10.0);
        let btn_rect = egui::Rect::from_center_size(btn_center, Vec2::splat(16.0));

        // Dark circle background
        painter.circle_filled(
            btn_center,
            9.0,
            Color32::from_rgba_premultiplied(20, 20, 30, 200),
        );

        let btn_response = ui.allocate_rect(btn_rect, egui::Sense::click());
        let x_color = if btn_response.hovered() {
            theme::ACCENT_RED
        } else {
            theme::TEXT_SECONDARY
        };

        painter.text(
            btn_center,
            egui::Align2::CENTER_CENTER,
            crate::PhosphorIcon::X.codepoint().to_string(),
            egui::FontId::new(10.0, crate::icons::phosphor_family()),
            x_color,
        );

        if btn_response.clicked() {
            action = Some(OfferSlotAction::Remove);
        }
    }

    OfferSlotResponse { action, hovered }
}
