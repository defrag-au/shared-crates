//! Asset strip — a horizontal row of square asset thumbnails that overlap
//! progressively as more items are added.
//!
//! Hovering an item lifts it to show it fully. Clicking emits an action
//! with the index of the clicked item.

use cardano_assets::AssetId;
use egui::{Color32, CornerRadius, Rect, Vec2};

use crate::card_browser;
use crate::image_loader::{iiif_asset_url, AssetImageSize};
use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// A single item in the strip.
#[derive(Clone, Debug)]
pub struct AssetStripItem {
    /// The on-chain asset identifier.
    pub asset_id: AssetId,
    /// Human-readable display name (shown in tooltip).
    pub display_name: String,
}

/// Configuration for the strip appearance.
pub struct AssetStripConfig {
    /// Size of each thumbnail (square).
    pub thumb_size: f32,
    /// Minimum visible width per item when overlapping (pixels).
    /// Items will never overlap more than `thumb_size - min_visible`.
    pub min_visible: f32,
}

impl Default for AssetStripConfig {
    fn default() -> Self {
        Self {
            thumb_size: 72.0,
            min_visible: 20.0,
        }
    }
}

/// Response from the strip.
pub struct AssetStripResponse {
    /// Index of the item that was clicked, if any.
    pub clicked: Option<usize>,
}

// ============================================================================
// Show
// ============================================================================

/// Render a horizontal strip of overlapping asset thumbnails.
///
/// Returns the index of any clicked item.
pub fn show(
    ui: &mut egui::Ui,
    items: &[AssetStripItem],
    config: &AssetStripConfig,
) -> AssetStripResponse {
    let mut clicked = None;

    if items.is_empty() {
        return AssetStripResponse { clicked };
    }

    crate::install_phosphor_font(ui.ctx());

    let n = items.len();
    let thumb = config.thumb_size;
    let available_width = ui.available_width();

    // Calculate step (horizontal advance per card).
    // If all cards fit without overlap, step = thumb_size.
    // Otherwise shrink step down to min_visible.
    let full_width = thumb * n as f32;
    let step = if full_width <= available_width {
        thumb
    } else {
        // Need to overlap. Total width = step * (n-1) + thumb = available_width
        // step = (available_width - thumb) / (n - 1)
        let computed = if n > 1 {
            (available_width - thumb) / (n - 1) as f32
        } else {
            thumb
        };
        computed.max(config.min_visible)
    };

    let total_width = if n > 1 {
        step * (n - 1) as f32 + thumb
    } else {
        thumb
    };

    // Lift amount is proportional to how covered each card is.
    // coverage_ratio: 0.0 (fully visible) → 1.0 (maximally overlapped)
    let coverage_ratio = (1.0 - step / thumb).max(0.0);
    // Subtle pop when uncovered, large lift when heavily stacked
    let min_lift = thumb * 0.08;
    let max_lift = thumb * 0.75;
    let lift_amount = min_lift + (max_lift - min_lift) * coverage_ratio;

    let (strip_rect, strip_response) = ui.allocate_exact_size(
        Vec2::new(total_width, thumb + lift_amount),
        egui::Sense::click(),
    );

    // The baseline where cards normally sit (bottom of the allocated rect)
    let baseline_y = strip_rect.min.y + lift_amount;

    let hover_pos = strip_response.hover_pos();
    let painter = ui.painter_at(strip_rect);
    let rounding = CornerRadius::same(4);

    // Determine which card is hovered (topmost = rightmost at overlap point)
    let hovered_idx = hover_pos.and_then(|pos| {
        // Walk right-to-left so topmost (last drawn) wins
        (0..n).rev().find(|&i| {
            let x = strip_rect.min.x + step * i as f32;
            let card_rect = Rect::from_min_size(egui::pos2(x, baseline_y), Vec2::splat(thumb));
            card_rect.contains(pos)
        })
    });

    // Draw cards left-to-right (later cards on top), but skip hovered to draw last
    let browser_config = crate::CardBrowserConfig {
        rounding: 4.0,
        bg_card_hover: Color32::from_rgb(40, 40, 55),
        ..Default::default()
    };

    // Animate dim overlay on non-hovered cards (smooth fade in/out)
    let any_hovered = hovered_idx.is_some();
    let dim_t = ui.ctx().animate_bool_with_time_and_easing(
        strip_response.id.with("dim"),
        any_hovered,
        0.15,
        egui::emath::easing::cubic_out,
    );

    // Single pass left-to-right: z-order stays constant (later cards on top)
    for (i, item) in items.iter().enumerate() {
        let is_hovered = hovered_idx == Some(i);

        // Animate vertical lift — cubic_out for snappy rise, smooth settle
        let anim_id = strip_response.id.with(("lift", i));
        let t = ui.ctx().animate_bool_with_time_and_easing(
            anim_id,
            is_hovered,
            0.18,
            egui::emath::easing::cubic_out,
        );
        let lift = t * lift_amount;

        let x = strip_rect.min.x + step * i as f32;
        let y = baseline_y - lift;
        let card_rect = Rect::from_min_size(egui::pos2(x, y), Vec2::splat(thumb));

        // Background
        painter.rect_filled(card_rect, rounding, theme::BG_SECONDARY);

        // Thumbnail
        let image_url = iiif_asset_url(
            item.asset_id.policy_id(),
            item.asset_id.asset_name_hex(),
            AssetImageSize::Thumbnail,
        );
        let loading =
            card_browser::draw_thumbnail(ui, card_rect, Some(&image_url), &browser_config);
        if loading {
            crate::image_loader::CachedSpinner::request_repaint(ui);
        }

        // Dim non-hovered cards when any card is hovered
        if any_hovered && !is_hovered {
            let dim_alpha = (dim_t * 160.0) as u8;
            painter.rect_filled(
                card_rect,
                rounding,
                Color32::from_rgba_premultiplied(8, 8, 15, dim_alpha),
            );
        }

        // Subtle border
        painter.rect_stroke(
            card_rect,
            rounding,
            egui::Stroke::new(1.0, theme::BG_HIGHLIGHT),
            egui::StrokeKind::Inside,
        );
    }

    // Tooltip for hovered item
    if let Some(idx) = hovered_idx {
        strip_response.clone().on_hover_ui_at_pointer(|ui| {
            ui.label(
                egui::RichText::new(&items[idx].display_name)
                    .color(theme::TEXT_PRIMARY)
                    .size(10.0)
                    .strong(),
            );
        });
    }

    // Click detection — check if cursor is in the hovered card
    if strip_response.clicked() {
        clicked = hovered_idx;
    }

    AssetStripResponse { clicked }
}
