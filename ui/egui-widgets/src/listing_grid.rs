use crate::image_loader::CachedSpinner;
use egui::{Color32, RichText, Sense, Vec2};

/// A single listing card to display in the grid.
#[derive(Clone)]
pub struct ListingCard {
    pub name: String,
    pub image_url: Option<String>,
    pub price_lovelace: u64,
    pub marketplace: String,
    pub marketplace_url: Option<String>,
    pub unit: String,
    /// Optional gap-fill badge: how many trait gaps this listing fills.
    pub gap_fill_count: Option<usize>,
    /// Optional tooltip text describing which traits are filled.
    pub gap_fill_tooltip: Option<String>,
}

/// Configuration for the listing grid layout and colors.
pub struct ListingGridConfig {
    pub card_width: f32,
    pub thumbnail_size: f32,
    pub spacing: f32,
    pub bg_color: Color32,
    pub bg_hover_color: Color32,
    pub text_primary: Color32,
    pub text_muted: Color32,
    pub accent_green: Color32,
    pub rounding: f32,
}

impl Default for ListingGridConfig {
    fn default() -> Self {
        Self {
            card_width: 84.0,
            thumbnail_size: 100.0,
            spacing: 8.0,
            bg_color: Color32::from_rgb(30, 31, 48),
            bg_hover_color: Color32::from_rgb(45, 46, 68),
            text_primary: Color32::from_rgb(192, 202, 224),
            text_muted: Color32::from_rgb(96, 104, 128),
            accent_green: Color32::from_rgb(158, 206, 106),
            rounding: 6.0,
        }
    }
}

/// A reusable grid widget for displaying marketplace listings with thumbnails.
///
/// Uses egui's built-in image loader (`egui_extras::install_image_loaders`)
/// for async thumbnail display. The consumer must call `install_image_loaders`
/// once at startup.
pub struct ListingGrid {
    config: ListingGridConfig,
}

impl Default for ListingGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl ListingGrid {
    pub fn new() -> Self {
        Self {
            config: ListingGridConfig::default(),
        }
    }

    pub fn with_config(config: ListingGridConfig) -> Self {
        Self { config }
    }

    /// Draw the listing grid. Returns the index of the hovered card, if any.
    pub fn show(&self, ui: &mut egui::Ui, listings: &[ListingCard]) -> Option<usize> {
        if listings.is_empty() {
            ui.label(
                RichText::new("No listings found")
                    .color(self.config.text_muted)
                    .size(11.0),
            );
            return None;
        }

        let cfg = &self.config;
        // Card is square: thumbnail fills entire card, price banner overlays bottom
        let card_size = Vec2::splat(cfg.card_width);

        let inner = ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(cfg.spacing);
            let mut hovered_idx: Option<usize> = None;
            let spinner = CachedSpinner::new(ui, 12.0, cfg.text_muted);
            let mut any_pending = false;

            for (card_idx, listing) in listings.iter().enumerate() {
                let is_filler = listing.gap_fill_count.is_some_and(|c| c > 0);
                let is_gap_mode = listing.gap_fill_count.is_some();
                let is_dimmed = is_gap_mode && !is_filler;

                let (rect, resp) = ui.allocate_exact_size(card_size, Sense::click());

                let card_hovered = resp.hovered();
                if card_hovered {
                    hovered_idx = Some(card_idx);
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }

                // Card background
                let bg = if card_hovered {
                    cfg.bg_hover_color
                } else {
                    cfg.bg_color
                };
                ui.painter().rect_filled(rect, cfg.rounding, bg);

                // Thumbnail fills entire card
                let visible = ui.clip_rect().intersects(rect);

                if let Some(ref url) = listing.image_url {
                    if visible {
                        let is_loaded = ui
                            .ctx()
                            .try_load_texture(
                                url,
                                egui::TextureOptions::default(),
                                egui::load::SizeHint::default(),
                            )
                            .is_ok_and(|poll| {
                                matches!(poll, egui::load::TexturePoll::Ready { .. })
                            });

                        if is_loaded {
                            let mut child_ui =
                                ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(
                                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                ));
                            let mut image = egui::Image::new(url.as_str())
                                .fit_to_exact_size(card_size)
                                .show_loading_spinner(false)
                                .corner_radius(cfg.rounding as u8);
                            if is_dimmed {
                                image =
                                    image.tint(Color32::from_rgba_unmultiplied(255, 255, 255, 100));
                            }
                            child_ui.add(image);
                        } else {
                            spinner.paint(ui, rect);
                            any_pending = true;
                        }
                    }
                } else {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "?",
                        egui::FontId::proportional(20.0),
                        cfg.text_muted,
                    );
                }

                // Price banner (bottom of card)
                let price_ada = listing.price_lovelace as f64 / 1_000_000.0;
                let banner_h = 18.0;
                let banner_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.min.x, rect.max.y - banner_h),
                    Vec2::new(cfg.card_width, banner_h),
                );
                ui.painter().rect_filled(
                    banner_rect,
                    egui::CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: cfg.rounding as u8,
                        se: cfg.rounding as u8,
                    },
                    Color32::from_rgba_premultiplied(15, 15, 25, 200),
                );
                let price_color = if is_dimmed {
                    Color32::from_rgb(96, 130, 80)
                } else {
                    cfg.accent_green
                };
                ui.painter().text(
                    banner_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{price_ada:.0} ADA"),
                    egui::FontId::monospace(10.0),
                    price_color,
                );

                // Gap-fill banner (above price banner)
                if let Some(count) = listing.gap_fill_count {
                    if count > 0 {
                        let gap_banner_rect = egui::Rect::from_min_size(
                            egui::pos2(rect.min.x, banner_rect.min.y - banner_h),
                            Vec2::new(cfg.card_width, banner_h),
                        );
                        ui.painter().rect_filled(
                            gap_banner_rect,
                            0,
                            Color32::from_rgba_premultiplied(158, 206, 106, 220),
                        );
                        let banner_text = format!("Fills {count}");
                        ui.painter().text(
                            gap_banner_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            banner_text,
                            egui::FontId::monospace(10.0),
                            Color32::from_rgb(26, 27, 38),
                        );
                    }
                }

                // Dim overlay for non-fillers
                if is_dimmed {
                    ui.painter().rect_filled(
                        rect,
                        cfg.rounding,
                        Color32::from_rgba_premultiplied(18, 19, 30, 120),
                    );
                }

                // Click opens Wayup
                if resp.clicked() {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(ref market_url) = listing.marketplace_url {
                        if let Some(window) = web_sys::window() {
                            let _ = window.open_with_url_and_target(market_url, "wayup");
                        }
                    }
                }

                // Tooltip with name + gap info
                if let Some(ref tooltip) = listing.gap_fill_tooltip {
                    resp.on_hover_text(format!("{}\n{price_ada:.0} ADA\n{tooltip}", listing.name));
                } else {
                    resp.on_hover_text(format!("{}\n{price_ada:.0} ADA", listing.name));
                }
            }

            if any_pending {
                CachedSpinner::request_repaint(ui);
            }

            hovered_idx
        });
        inner.inner
    }
}
