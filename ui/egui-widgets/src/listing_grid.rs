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
            card_width: 140.0,
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
        // Fixed card height: thumbnail + padding + 3 text lines
        let card_height = cfg.thumbnail_size + 6.0 + 4.0 + 14.0 + 14.0 + 12.0 + 6.0;

        let inner = ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(cfg.spacing);
            let mut hovered_idx: Option<usize> = None;
            let spinner = CachedSpinner::new(ui, 12.0, cfg.text_muted);
            let mut any_pending = false;

            for (card_idx, listing) in listings.iter().enumerate() {
                let is_filler = listing.gap_fill_count.is_some_and(|c| c > 0);
                let is_gap_mode = listing.gap_fill_count.is_some();
                let is_dimmed = is_gap_mode && !is_filler;

                let card_size = Vec2::new(cfg.card_width, card_height);
                let (rect, _resp) = ui.allocate_exact_size(card_size, Sense::hover());

                // Card background
                let card_hovered = _resp.hovered();
                if card_hovered {
                    hovered_idx = Some(card_idx);
                }
                let bg = if card_hovered {
                    cfg.bg_hover_color
                } else {
                    cfg.bg_color
                };
                ui.painter().rect_filled(rect, cfg.rounding, bg);

                // Green border on gap-fillers
                if is_filler {
                    ui.painter().rect_stroke(
                        rect,
                        cfg.rounding,
                        egui::Stroke::new(2.0, cfg.accent_green),
                        egui::StrokeKind::Inside,
                    );
                }

                // Thumbnail area — use a child ui so ui.add(Image) works for the loader
                let thumb_rect = egui::Rect::from_min_size(
                    rect.min + Vec2::new((cfg.card_width - cfg.thumbnail_size) / 2.0, 6.0),
                    Vec2::splat(cfg.thumbnail_size),
                );

                // Only render the image widget if the card is visible — off-screen
                // pending images drive spinner repaints and cause lag.
                let visible = ui.clip_rect().intersects(thumb_rect);

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
                                ui.new_child(egui::UiBuilder::new().max_rect(thumb_rect).layout(
                                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                ));
                            let mut image = egui::Image::new(url.as_str())
                                .fit_to_exact_size(Vec2::splat(cfg.thumbnail_size))
                                .show_loading_spinner(false)
                                .corner_radius(4);
                            if is_dimmed {
                                image =
                                    image.tint(Color32::from_rgba_unmultiplied(255, 255, 255, 100));
                            }
                            child_ui.add(image);
                        } else {
                            ui.painter()
                                .rect_filled(thumb_rect, 4.0, cfg.bg_hover_color);
                            spinner.paint(ui, thumb_rect);
                            any_pending = true;
                        }
                    } else {
                        ui.painter()
                            .rect_filled(thumb_rect, 4.0, cfg.bg_hover_color);
                    }
                } else {
                    ui.painter()
                        .rect_filled(thumb_rect, 4.0, cfg.bg_hover_color);
                    ui.painter().text(
                        thumb_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "?",
                        egui::FontId::proportional(20.0),
                        cfg.text_muted,
                    );
                }

                // Gap-fill banner (bottom of thumbnail, full width)
                if let Some(count) = listing.gap_fill_count {
                    if count > 0 {
                        let banner_text =
                            format!("Fills {count} gap{}", if count == 1 { "" } else { "s" });
                        let banner_h = 18.0;
                        let banner_rect = egui::Rect::from_min_size(
                            egui::pos2(thumb_rect.min.x, thumb_rect.max.y - banner_h),
                            Vec2::new(cfg.thumbnail_size, banner_h),
                        );
                        ui.painter().rect_filled(
                            banner_rect,
                            egui::CornerRadius {
                                nw: 0,
                                ne: 0,
                                sw: 4,
                                se: 4,
                            },
                            Color32::from_rgba_premultiplied(158, 206, 106, 220),
                        );
                        ui.painter().text(
                            banner_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            banner_text,
                            egui::FontId::monospace(10.0),
                            Color32::from_rgb(26, 27, 38),
                        );
                    }
                }

                // Dim overlay for non-fillers (semi-transparent dark wash over entire card)
                if is_dimmed {
                    ui.painter().rect_filled(
                        rect,
                        cfg.rounding,
                        Color32::from_rgba_premultiplied(18, 19, 30, 120),
                    );
                }

                // External link icon (top-right corner of card)
                if listing.marketplace_url.is_some() {
                    let icon_size = 16.0;
                    let icon_rect = egui::Rect::from_min_size(
                        egui::pos2(rect.max.x - icon_size - 4.0, rect.min.y + 4.0),
                        Vec2::splat(icon_size),
                    );
                    let icon_resp = ui.interact(
                        icon_rect,
                        ui.id().with(("ext_link", &listing.unit)),
                        Sense::click(),
                    );
                    let icon_color = if icon_resp.hovered() {
                        cfg.accent_green
                    } else {
                        cfg.text_muted
                    };
                    // Unicode arrow-up-right-from-square
                    ui.painter().text(
                        icon_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "\u{2197}",
                        egui::FontId::proportional(12.0),
                        icon_color,
                    );

                    if icon_resp.clicked() {
                        #[cfg(target_arch = "wasm32")]
                        if let Some(ref market_url) = listing.marketplace_url {
                            if let Some(window) = web_sys::window() {
                                let _ = window.open_with_url_and_target(market_url, "wayup");
                            }
                        }
                    }
                    icon_resp.on_hover_text("View on Wayup");
                }

                // Text below thumbnail — painter-based for precise positioning
                let text_x = rect.min.x + 6.0;
                let text_w = cfg.card_width - 12.0;
                let name_y = thumb_rect.max.y + 4.0;

                // Dimmed text colors for non-filler cards
                let name_color = if is_dimmed {
                    Color32::from_rgb(96, 104, 128)
                } else {
                    cfg.text_primary
                };
                let price_color = if is_dimmed {
                    Color32::from_rgb(96, 130, 80)
                } else {
                    cfg.accent_green
                };
                let market_color = if is_dimmed {
                    Color32::from_rgb(60, 65, 80)
                } else {
                    cfg.text_muted
                };

                // Name (clipped to card width)
                let name_rect =
                    egui::Rect::from_min_size(egui::pos2(text_x, name_y), Vec2::new(text_w, 14.0));
                ui.painter().with_clip_rect(name_rect).text(
                    name_rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    &listing.name,
                    egui::FontId::proportional(11.0),
                    name_color,
                );

                // Price
                let price_ada = listing.price_lovelace as f64 / 1_000_000.0;
                ui.painter().text(
                    egui::pos2(text_x, name_y + 14.0),
                    egui::Align2::LEFT_TOP,
                    format!("{price_ada:.0} ADA"),
                    egui::FontId::monospace(11.0),
                    price_color,
                );

                // Marketplace
                ui.painter().text(
                    egui::pos2(text_x, name_y + 28.0),
                    egui::Align2::LEFT_TOP,
                    &listing.marketplace,
                    egui::FontId::proportional(9.0),
                    market_color,
                );

                if let Some(ref tooltip) = listing.gap_fill_tooltip {
                    _resp.on_hover_text(format!("{}\n{tooltip}", listing.name));
                } else {
                    _resp.on_hover_text(&listing.name);
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
