//! Composable master-detail card browser widget.
//!
//! A filterable card grid on the left with an optional detail panel that slides
//! in on the right when a card is selected. The widget handles layout, scroll,
//! selection state, and card chrome — the caller provides rendering via closures.
//!
//! Supports both static thumbnails (via [`draw_thumbnail`]) and interactive cards
//! (e.g. `AssetCard` with 3D tilt) through the [`CardRenderContext::response`] field.

use crate::image_loader::CachedSpinner;
use crate::smart_image::{ImagePass, ImageState, SmartImage};
use crate::theme::{Ink, Radius, Space, SpaceExt, ThemeExt, Token};
use egui::{Pos2, Rect, Sense, Stroke, Vec2};

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
    /// Gutter between cards. `None` takes the theme's [`Space::Md`], which is
    /// what the literal `8.0` here used to mean — same reasoning as the [`Ink`]
    /// fields below.
    pub spacing: Option<Space>,
    /// Card corner radius.
    pub rounding: f32,
    /// Scroll area ID salt (must be unique if multiple browsers on one page).
    pub scroll_id: &'static str,
    /// Rows drawn beyond the viewport, above and below.
    ///
    /// Cards outside this band are allocated but never painted, which is what
    /// keeps a 580-card grid from laying out thousands of galleys a frame. The
    /// band is also the image loader's lead time, and that is what sets the
    /// floor: a card only asks for its thumbnail when it is painted, so with
    /// too few rows a scroll reveals cards that are still fetching and they
    /// appear blank for a frame or two before filling in.
    ///
    /// It matters more than it looks because retention is LRU over "when was
    /// this last asked for" (`schedule::Schedule::release_cold`). Painting
    /// every card kept every image permanently warm; culling is what lets the
    /// byte cap evict genuinely cold images — correct, but it means a card
    /// scrolled back to may need a real reload, not just a texture upload.
    ///
    /// Two is enough to cover a fast scroll without painting a screenful of
    /// cards nobody sees. Lead time for the image loader is
    /// [`Self::warm_rows`], which is a separate and much wider band.
    pub overscan_rows: usize,
    /// Rows beyond [`Self::overscan_rows`] whose images are REQUESTED but not
    /// drawn — see [`ImagePass::Warm`].
    ///
    /// This is the loader's lead time, and it can afford to be generous
    /// because asking for an image costs a cache lookup. An image needs a
    /// fetch, a decode and a texture upload before it can appear, so the band
    /// has to be wide enough to cover that at scrolling speed, not merely
    /// one row.
    ///
    /// It matters more than it looks because retention is LRU over "when was
    /// this last asked for" (`schedule::Schedule::release_cold`): a card in
    /// the warm band counts as asked-for, so warming also keeps an image from
    /// being evicted just before it is needed.
    pub warm_rows: usize,
    /// Lay the grid out at its natural height instead of inside its own scroll
    /// area. For a bounded grid embedded in an already-scrolling page, where a
    /// nested scrollbar is the wrong affordance — the section should simply be
    /// as tall as its contents. `near_bottom` is then always true, since all
    /// items are on screen.
    pub grow_to_content: bool,
    /// Card background color (normal).
    pub bg_card: Ink,
    /// Card background color (hovered).
    pub bg_card_hover: Ink,
    /// Card background color (selected).
    pub bg_card_selected: Ink,
    /// Card border color (normal).
    pub border_color: Ink,
    /// Card border color (selected).
    pub border_selected: Ink,
    /// Muted text / placeholder color.
    pub text_muted: Ink,
    /// Detail panel background color.
    pub bg_detail: Ink,
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

    /// Vertical slack, in points, painted beyond the viewport each way.
    ///
    /// Pure, so the knobs that decide whether a scroll shows blank cards can
    /// be asserted on without a `Ui`.
    pub fn overscan_px(&self, gutter: f32) -> f32 {
        (self.card_height() + gutter) * self.overscan_rows as f32
    }

    /// Vertical slack, in points, whose images are requested each way.
    ///
    /// Measured from the viewport like [`Self::overscan_px`], and always at
    /// least as far — a card that paints without its image having been asked
    /// for is the blank-card bug.
    pub fn warm_px(&self, gutter: f32) -> f32 {
        let rows = self.warm_rows.max(self.overscan_rows);
        (self.card_height() + gutter) * rows as f32
    }
}

#[cfg(test)]
mod overscan_tests {
    use super::CardBrowserConfig;

    /// The warm band must reach FURTHER than the paint band, or a card is
    /// painted before anything asked for its image — the blank-card bug. This
    /// is the invariant; the particular row counts are tuning.
    #[test]
    fn images_are_asked_for_further_out_than_cards_are_painted() {
        let config = CardBrowserConfig::default();
        assert!(
            config.warm_px(8.0) > config.overscan_px(8.0),
            "warm {} must exceed paint {}",
            config.warm_px(8.0),
            config.overscan_px(8.0),
        );
    }

    /// …and it holds even if someone configures the bands the wrong way
    /// round, because `warm_px` takes the larger of the two rather than
    /// trusting the number it was given.
    #[test]
    fn a_warm_band_narrower_than_the_paint_band_is_widened_to_match() {
        let config = CardBrowserConfig {
            overscan_rows: 6,
            warm_rows: 1,
            ..Default::default()
        };
        assert_eq!(config.warm_px(8.0), config.overscan_px(8.0));
    }

    #[test]
    fn the_bands_are_whole_rows_of_card_plus_gutter() {
        let config = CardBrowserConfig::default();
        let row = config.card_height() + 8.0;
        assert!(
            (config.overscan_px(8.0) - row * config.overscan_rows as f32).abs() < f32::EPSILON
        );
        assert!((config.warm_px(8.0) - row * config.warm_rows as f32).abs() < f32::EPSILON);
    }

    /// Zero is a legitimate setting — cull hard to the viewport — and must
    /// mean no slack rather than one row by accident.
    #[test]
    fn zero_overscan_culls_to_the_viewport() {
        let config = CardBrowserConfig {
            overscan_rows: 0,
            ..Default::default()
        };
        assert_eq!(config.overscan_px(8.0), 0.0);
    }
}

impl Default for CardBrowserConfig {
    fn default() -> Self {
        Self {
            card_width: 140.0,
            thumb_aspect_ratio: 1.0,
            text_lines: 3,
            detail_width: 420.0,
            spacing: None,
            rounding: 6.0,
            scroll_id: "card_browser",
            overscan_rows: 2,
            warm_rows: 8,
            grow_to_content: false,
            bg_card: Ink::Token(Token::BgPrimary),
            bg_card_hover: Ink::Token(Token::BgHighlight),
            bg_card_selected: Ink::Token(Token::BgHighlight),
            // Was `from_rgba_premultiplied(86, 95, 137, 40)` — the constructor
            // that expects already-scaled channels, so it blended additively and
            // painted a hairline brighter than any border in the palette. The
            // token whose whole job is a card edge says this directly.
            border_color: Ink::Token(Token::Border),
            border_selected: Ink::Token(Token::AccentCyan),
            text_muted: Ink::Token(Token::TextMuted),
            // One step up the background ramp from the cards, so the panel reads
            // as a surface in front of the grid rather than a hole in it.
            bg_detail: Ink::Token(Token::BgSecondary),
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

/// The thumbnail area inside a card rect.
///
/// Shared by both passes so a warmed card computes the same thumbnail it will
/// later paint — a warm pass that asked for a different size would fill the
/// cache with an image the paint pass then misses.
fn thumb_rect_of(rect: Rect, config: &CardBrowserConfig) -> Rect {
    let thumb_w = config.card_width - CARD_INSET * 2.0;
    let thumb_h = thumb_w * config.thumb_aspect_ratio;
    Rect::from_min_size(
        rect.min + Vec2::splat(CARD_INSET),
        Vec2::new(thumb_w, thumb_h),
    )
}

/// Context passed to the card render closure for each card.
pub struct CardRenderContext {
    /// Whether this call should draw the card, or only get its images on
    /// their way.
    ///
    /// Hand it straight to [`crate::smart_image::SmartImage::show`] and write
    /// no branch — that widget does the right thing for either pass, and is
    /// the single place the decode size is chosen. A closure that ignores
    /// this and always draws is CORRECT but pays the paint cost across the
    /// whole warm band.
    pub pass: ImagePass,
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
    profiling::function_scope!();
    // Ensure Phosphor icon font is available (used for close button etc.)
    crate::icons::ensure_fonts(ui);

    // Resolved once, ahead of the closures that read them: a `Default` config
    // names its tokens, so the values arrive here.
    let t = ui.tokens();
    let bg_card = config.bg_card.resolve(&t);
    let bg_card_hover = config.bg_card_hover.resolve(&t);
    let bg_card_selected = config.bg_card_selected.resolve(&t);
    let border_color = config.border_color.resolve(&t);
    let border_selected = config.border_selected.resolve(&t);
    let text_muted = config.text_muted.resolve(&t);
    let bg_detail = config.bg_detail.resolve(&t);
    // Same reason: the gutter is part of the theme's rhythm unless a surface
    // overrides it, and it feeds the column arithmetic below as well as the
    // layout, so it has to be resolved to one value here.
    let gutter = t.space(config.spacing.unwrap_or(Space::Md));

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
                let cols = ((grid_width + gutter) / (config.card_width + gutter))
                    .floor()
                    .max(1.0) as usize;
                let row = anchor_idx / cols;
                let card_y_in_content = row as f32 * (config.card_height() + gutter);
                let scroll_area_top = ui.cursor().min.y;
                let screen_y_relative = anchor_screen_y - scroll_area_top;
                let new_offset = (card_y_in_content - screen_y_relative).max(0.0);
                scroll = scroll.vertical_scroll_offset(new_offset);
            }

            let mut grid = |ui: &mut egui::Ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(gutter);
                    let spinner = CachedSpinner::new(ui, 12.0, text_muted);
                    let _ = spinner; // available for draw_thumbnail callers

                    // Two bands, because painting and loading want different
                    // answers — see `ImagePass`. Inside `paint_rect` the card
                    // is drawn; between there and `warm_rect` only its image
                    // is asked for; beyond that the card is allocated and
                    // nothing else happens to it.
                    let viewport = ui.clip_rect();
                    let paint_rect =
                        viewport.expand2(Vec2::new(0.0, config.overscan_px(gutter)));
                    let warm_rect = viewport.expand2(Vec2::new(0.0, config.warm_px(gutter)));

                    for (idx, item) in items.iter_mut().enumerate() {
                        let card_size = Vec2::new(config.card_width, config.card_height());
                        let (rect, card_resp) = ui.allocate_exact_size(card_size, Sense::click());

                        // Offscreen cards are ALLOCATED but not painted.
                        //
                        // The allocation has to happen either way — it is what
                        // gives the scroll area its extent and keeps the
                        // wrapping positions stable — but painting one costs a
                        // background, a border and the caller's whole
                        // `render_card`, which for a typical card is four or
                        // five galley layouts. A browse grid of ~580 assets was
                        // laying out 2,885 galleys per frame, every frame, for
                        // the thirty-odd cards anyone could actually see.
                        //
                        // Nothing below this point can fire for a card the
                        // reader cannot reach: egui resolves interaction
                        // against the clip rect, so an offscreen card is never
                        // hovered and never clicked.
                        if !warm_rect.intersects(rect) {
                            continue;
                        }

                        // In the warm band only: let the caller ask for the
                        // image and nothing else. No background, no border,
                        // no response — none of it would be visible, and the
                        // point is to have the picture ready BEFORE the card
                        // is worth drawing.
                        if !paint_rect.intersects(rect) {
                            let ctx = CardRenderContext {
                                pass: ImagePass::Warm,
                                rect,
                                thumb_rect: thumb_rect_of(rect, config),
                                text_origin: Pos2::new(rect.min.x + 6.0, rect.min.y),
                                text_width: config.card_width - 12.0,
                                is_selected: false,
                                is_hovered: false,
                                response: card_resp,
                            };
                            render_card(ui, &ctx, item);
                            continue;
                        }

                        let is_selected = state.selected == Some(idx);
                        let is_hovered = card_resp.hovered();

                        if is_hovered {
                            response.hovered = Some(idx);
                        }

                        // Card background
                        let bg = if is_selected {
                            bg_card_selected
                        } else if is_hovered {
                            bg_card_hover
                        } else {
                            bg_card
                        };
                        ui.painter().rect_filled(rect, config.rounding, bg);

                        // Border
                        if is_selected {
                            ui.painter().rect_stroke(
                                rect,
                                config.rounding,
                                Stroke::new(3.0_f32, border_selected),
                                egui::StrokeKind::Inside,
                            );
                        } else {
                            ui.painter().rect_stroke(
                                rect,
                                config.rounding,
                                Stroke::new(1.0_f32, border_color),
                                egui::StrokeKind::Inside,
                            );
                        }

                        // Compute sub-rects — thumbnail fills card width
                        let thumb_rect = thumb_rect_of(rect, config);
                        let text_x = rect.min.x + 6.0;
                        let text_w = config.card_width - 12.0;
                        let text_y = thumb_rect.max.y + TEXT_GAP;

                        let ctx = CardRenderContext {
                            pass: ImagePass::Paint,
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
            };

            if config.grow_to_content {
                // Every item is laid out and visible, so any lazy loader
                // watching `near_bottom` should fetch the next page.
                grid(ui);
                response.near_bottom = true;
            } else {
                let scroll_output = scroll.show(ui, &mut grid);

                // Detect near-bottom: within ~2 card rows of the content bottom
                let content_height = scroll_output.content_size.y;
                let viewport_height = scroll_output.inner_rect.height();
                let offset = scroll_output.state.offset.y;
                let threshold = (config.card_height() + gutter) * 2.0;
                if content_height > viewport_height
                    && offset + viewport_height >= content_height - threshold
                {
                    response.near_bottom = true;
                }
            }
        });

        // RIGHT: detail panel
        if let Some(sel_idx) = state.selected
            && sel_idx < items.len()
        {
            ui.gap(Space::Xl);
            ui.vertical(|ui| {
                ui.set_max_width(config.detail_width);
                ui.set_min_width(config.detail_width);
                let frame_resp = egui::Frame::new()
                    .fill(bg_detail)
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
                                crate::PhosphorIcon::X.rich_text(14.0, border_selected),
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
    let t = ui.tokens();
    let bg_card_hover = config.bg_card_hover.resolve(&t);
    let text_muted = config.text_muted.resolve(&t);
    let radius = t.corner(Radius::Base);

    // A thumbnail entirely outside the clip rect still gets its backdrop —
    // cheap, and it keeps a partially-scrolled row from showing holes — but
    // must not ask for an image. That is the warm band's job, and it has a
    // much better idea of how far ahead to look.
    if !ui.clip_rect().intersects(thumb_rect) {
        ui.painter().rect_filled(thumb_rect, radius, bg_card_hover);
        return false;
    }

    let state = SmartImage::from_option(image_url)
        .corner_radius(radius)
        .backdrop(config.bg_card_hover)
        .show(ui, thumb_rect, ImagePass::Paint);

    // The spinner is this widget's own flourish rather than something
    // `SmartImage` imposes on every caller — a 48pt leaderboard thumbnail
    // wants a quiet backdrop, not a spinner in it.
    if state == ImageState::Loading {
        CachedSpinner::new(ui, 12.0, text_muted).paint(ui, thumb_rect);
    }
    state.wants_repaint()
}
