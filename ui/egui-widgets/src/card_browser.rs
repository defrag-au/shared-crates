//! Composable master-detail card browser widget.
//!
//! A filterable card grid on the left with an optional detail panel that slides
//! in on the right when a card is selected. The widget handles layout, scroll,
//! selection state, and card chrome — the caller provides rendering via closures.
//!
//! Supports both static thumbnails (via [`draw_thumbnail`]) and interactive cards
//! (e.g. `AssetCard` with 3D tilt) through the [`CardRenderContext::response`] field.

use crate::image_loader::CachedSpinner;
use crate::theme;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

// ============================================================================
// Config & State
// ============================================================================

/// Layout and color configuration for the card browser.
///
/// Card height is computed automatically from `card_width` and `text_lines`:
/// `inset + thumbnail (= card_width - 2*inset) + gap + text_lines * line_height + bottom_padding`
///
/// Use [`CardBrowserConfig::card_height`] to query the computed value.
pub struct CardBrowserConfig {
    /// Width of each card. The thumbnail fills this width (minus a small inset).
    pub card_width: f32,
    /// Aspect ratio of the thumbnail (height / width). Default 1.0 (square).
    /// Use 1.395 for MtG cards (680/488).
    pub thumb_aspect_ratio: f32,
    /// Number of text lines below the thumbnail (e.g. name + subtitle + price = 3).
    pub text_lines: u8,
    /// Width of the detail panel when a card is selected.
    pub detail_width: f32,
    /// Spacing between cards.
    pub spacing: f32,
    /// Card corner radius.
    pub rounding: f32,
    /// Scroll area ID salt (must be unique if multiple browsers on one page).
    pub scroll_id: &'static str,
    /// Card background color (normal).
    pub bg_card: Color32,
    /// Card background color (hovered).
    pub bg_card_hover: Color32,
    /// Card background color (selected).
    pub bg_card_selected: Color32,
    /// Card border color (normal).
    pub border_color: Color32,
    /// Card border color (selected).
    pub border_selected: Color32,
    /// Muted text / placeholder color.
    pub text_muted: Color32,
    /// Detail panel background color.
    pub bg_detail: Color32,
    /// Detail panel inner margin.
    pub detail_margin: f32,
}

/// Inset around the thumbnail within the card.
const CARD_INSET: f32 = 4.0;
/// Gap between thumbnail bottom and text area.
const TEXT_GAP: f32 = 4.0;
/// Height per text line below the thumbnail.
const LINE_HEIGHT: f32 = 14.0;
/// Padding below the last text line.
const BOTTOM_PAD: f32 = 4.0;

impl CardBrowserConfig {
    /// Computed card height based on `card_width`, `thumb_aspect_ratio`, and `text_lines`.
    pub fn card_height(&self) -> f32 {
        let thumb_w = self.card_width - CARD_INSET * 2.0;
        let thumb_h = thumb_w * self.thumb_aspect_ratio;
        CARD_INSET + thumb_h + TEXT_GAP + self.text_lines as f32 * LINE_HEIGHT + BOTTOM_PAD
    }
}

impl Default for CardBrowserConfig {
    fn default() -> Self {
        Self {
            card_width: 140.0,
            thumb_aspect_ratio: 1.0,
            text_lines: 3,
            detail_width: 420.0,
            spacing: 8.0,
            rounding: 6.0,
            scroll_id: "card_browser",
            bg_card: theme::BG_PRIMARY,
            bg_card_hover: theme::BG_HIGHLIGHT,
            bg_card_selected: Color32::from_rgb(40, 45, 55),
            border_color: Color32::from_rgba_premultiplied(86, 95, 137, 40),
            border_selected: theme::ACCENT_CYAN,
            text_muted: theme::TEXT_MUTED,
            bg_detail: Color32::from_rgb(30, 32, 42),
            detail_margin: 14.0,
        }
    }
}

/// Persistent state for the card browser (selection tracking).
#[derive(Default)]
pub struct CardBrowserState {
    /// Index of the currently selected card, if any.
    pub selected: Option<usize>,
    /// One-shot scroll anchor: (card_index, screen_y_at_click).
    /// Set when selection changes, consumed on the next frame to stabilize
    /// the selected card's screen position after the grid reflows.
    scroll_anchor: Option<(usize, f32)>,
}

/// Context passed to the card render closure for each card.
pub struct CardRenderContext {
    /// The full card rect (including padding).
    pub rect: Rect,
    /// The thumbnail area rect (centered within the card).
    pub thumb_rect: Rect,
    /// Origin point for text below the thumbnail.
    pub text_origin: Pos2,
    /// Available width for text content.
    pub text_width: f32,
    /// Whether this card is the currently selected one.
    pub is_selected: bool,
    /// Whether this card is being hovered.
    pub is_hovered: bool,
    /// The card's egui Response — use `hover_pos()` for interactive content
    /// like 3D tilt, or `hovered()` for highlight effects.
    pub response: egui::Response,
}

/// Result from a `show()` call.
pub struct CardBrowserResponse {
    /// Index of the card that was clicked this frame (for caller to react).
    pub clicked: Option<usize>,
    /// Index of the card being hovered.
    pub hovered: Option<usize>,
    /// Whether the detail panel is visible.
    pub detail_visible: bool,
    /// True when scroll position is within ~2 rows of the bottom content edge.
    pub near_bottom: bool,
}

// ============================================================================
// Main widget
// ============================================================================

/// Draw a master-detail card browser.
///
/// The widget handles the grid layout, scroll, selection toggle, card chrome
/// (background, border, hover/selected states), and the side-by-side split with
/// the detail panel. The caller provides two closures:
///
/// - `render_card`: paints card content into the [`CardRenderContext`] rects
/// - `render_detail`: paints the detail panel when a card is selected
///
/// Items are `&mut` so card render closures can mutate per-item state (e.g.
/// `TiltState` for interactive cards). For read-only use cases, simply don't
/// mutate.
pub fn show<T>(
    ui: &mut egui::Ui,
    state: &mut CardBrowserState,
    items: &mut [T],
    config: &CardBrowserConfig,
    mut render_card: impl FnMut(&mut egui::Ui, &CardRenderContext, &mut T),
    mut render_detail: impl FnMut(&mut egui::Ui, usize, &mut T),
) -> CardBrowserResponse {
    // Ensure Phosphor icon font is available (used for close button etc.)
    crate::install_phosphor_font(ui.ctx());

    let has_selection = state.selected.is_some_and(|idx| idx < items.len());
    let detail_width = if has_selection {
        config.detail_width
    } else {
        0.0
    };

    let mut response = CardBrowserResponse {
        clicked: None,
        hovered: None,
        detail_visible: has_selection,
        near_bottom: false,
    };

    ui.horizontal_top(|ui| {
        // LEFT: card grid
        let grid_width = if has_selection {
            (ui.available_width() - detail_width - 12.0).max(200.0)
        } else {
            ui.available_width()
        };

        ui.vertical(|ui| {
            ui.set_max_width(grid_width);

            // Scroll stabilization: when selection changes, the grid width
            // changes and cards reflow. Compute the new scroll offset so the
            // clicked card stays at the same screen-Y position.
            let mut scroll = egui::ScrollArea::vertical().id_salt(config.scroll_id);
            if let Some((anchor_idx, anchor_screen_y)) = state.scroll_anchor.take() {
                let cols = ((grid_width + config.spacing) / (config.card_width + config.spacing))
                    .floor()
                    .max(1.0) as usize;
                let row = anchor_idx / cols;
                let card_y_in_content = row as f32 * (config.card_height() + config.spacing);
                let scroll_area_top = ui.cursor().min.y;
                let screen_y_relative = anchor_screen_y - scroll_area_top;
                let new_offset = (card_y_in_content - screen_y_relative).max(0.0);
                scroll = scroll.vertical_scroll_offset(new_offset);
            }

            let scroll_output = scroll.show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(config.spacing);
                    let spinner = CachedSpinner::new(ui, 12.0, config.text_muted);
                    let _ = spinner; // available for draw_thumbnail callers

                    for (idx, item) in items.iter_mut().enumerate() {
                        let card_size = Vec2::new(config.card_width, config.card_height());
                        let (rect, card_resp) = ui.allocate_exact_size(card_size, Sense::click());

                        let is_selected = state.selected == Some(idx);
                        let is_hovered = card_resp.hovered();

                        if is_hovered {
                            response.hovered = Some(idx);
                        }

                        // Card background
                        let bg = if is_selected {
                            config.bg_card_selected
                        } else if is_hovered {
                            config.bg_card_hover
                        } else {
                            config.bg_card
                        };
                        ui.painter().rect_filled(rect, config.rounding, bg);

                        // Border
                        if is_selected {
                            ui.painter().rect_stroke(
                                rect,
                                config.rounding,
                                Stroke::new(3.0, config.border_selected),
                                egui::StrokeKind::Inside,
                            );
                        } else {
                            ui.painter().rect_stroke(
                                rect,
                                config.rounding,
                                Stroke::new(1.0, config.border_color),
                                egui::StrokeKind::Inside,
                            );
                        }

                        // Compute sub-rects — thumbnail fills card width
                        let thumb_w = config.card_width - CARD_INSET * 2.0;
                        let thumb_h = thumb_w * config.thumb_aspect_ratio;
                        let thumb_rect = Rect::from_min_size(
                            rect.min + Vec2::splat(CARD_INSET),
                            Vec2::new(thumb_w, thumb_h),
                        );
                        let text_x = rect.min.x + 6.0;
                        let text_w = config.card_width - 12.0;
                        let text_y = thumb_rect.max.y + TEXT_GAP;

                        let ctx = CardRenderContext {
                            rect,
                            thumb_rect,
                            text_origin: Pos2::new(text_x, text_y),
                            text_width: text_w,
                            is_selected,
                            is_hovered,
                            response: card_resp.clone(),
                        };

                        // Caller renders card content
                        render_card(ui, &ctx, item);

                        // Selection toggle
                        if card_resp.clicked() {
                            state.scroll_anchor = Some((idx, rect.min.y));
                            if is_selected {
                                state.selected = None;
                            } else {
                                state.selected = Some(idx);
                            }
                            response.clicked = Some(idx);
                        }
                    }
                });
            });

            // Detect near-bottom: within ~2 card rows of the content bottom
            let content_height = scroll_output.content_size.y;
            let viewport_height = scroll_output.inner_rect.height();
            let offset = scroll_output.state.offset.y;
            let threshold = (config.card_height() + config.spacing) * 2.0;
            if content_height > viewport_height
                && offset + viewport_height >= content_height - threshold
            {
                response.near_bottom = true;
            }
        });

        // RIGHT: detail panel
        if let Some(sel_idx) = state.selected {
            if sel_idx < items.len() {
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_max_width(config.detail_width);
                    ui.set_min_width(config.detail_width);
                    let frame_resp = egui::Frame::new()
                        .fill(config.bg_detail)
                        .corner_radius(config.rounding)
                        .inner_margin(config.detail_margin)
                        .show(ui, |ui| {
                            render_detail(ui, sel_idx, &mut items[sel_idx]);
                        });

                    // Overlay close button at top-right of panel (no vertical space consumed)
                    let panel_rect = frame_resp.response.rect;
                    let btn_size = egui::Vec2::splat(20.0);
                    let btn_rect = egui::Rect::from_min_size(
                        egui::pos2(panel_rect.max.x - btn_size.x - 4.0, panel_rect.min.y + 4.0),
                        btn_size,
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(btn_rect), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    crate::PhosphorIcon::X.rich_text(14.0, config.border_selected),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            state.selected = None;
                        }
                    });
                });
            }
        }
    });

    response
}

// ============================================================================
// Thumbnail helper
// ============================================================================

/// Draw an async-loading thumbnail into a rect.
///
/// Uses egui's built-in image loader. Returns `true` if the image is still
/// loading (caller should batch `CachedSpinner::request_repaint` calls).
///
/// If `image_url` is `None`, draws a placeholder with a "?" glyph.
pub fn draw_thumbnail(
    ui: &mut egui::Ui,
    thumb_rect: Rect,
    image_url: Option<&str>,
    config: &CardBrowserConfig,
) -> bool {
    let Some(url) = image_url else {
        // No URL — placeholder
        ui.painter()
            .rect_filled(thumb_rect, 4.0, config.bg_card_hover);
        ui.painter().text(
            thumb_rect.center(),
            egui::Align2::CENTER_CENTER,
            "?",
            egui::FontId::proportional(20.0),
            config.text_muted,
        );
        return false;
    };

    let visible = ui.clip_rect().intersects(thumb_rect);
    if !visible {
        ui.painter()
            .rect_filled(thumb_rect, 4.0, config.bg_card_hover);
        return false;
    }

    let is_loaded = ui
        .ctx()
        .try_load_texture(
            url,
            egui::TextureOptions::default(),
            egui::load::SizeHint::default(),
        )
        .is_ok_and(|poll| matches!(poll, egui::load::TexturePoll::Ready { .. }));

    if is_loaded {
        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(thumb_rect).layout(
            egui::Layout::centered_and_justified(egui::Direction::TopDown),
        ));
        child_ui.add(
            egui::Image::new(url)
                .fit_to_exact_size(thumb_rect.size())
                .show_loading_spinner(false)
                .corner_radius(4),
        );
        false
    } else {
        ui.painter()
            .rect_filled(thumb_rect, 4.0, config.bg_card_hover);
        let spinner = CachedSpinner::new(ui, 12.0, config.text_muted);
        spinner.paint(ui, thumb_rect);
        true
    }
}
