//! `ListingGrid` — a responsive grid of marketplace listing cards, each with a
//! lazily-loaded image, price and trailing badges.
//!
//! Cards also carry their [`Buyability`]. A listed price is not the same claim
//! as "you can buy this": a marketplace listing is only purchasable if its
//! datum can be recovered and its contract is one we can drive. Showing a price
//! with no way to act on it — or worse, offering a buy that will fail — is the
//! failure this state exists to prevent.

use crate::corner_action::{Corner, CornerAction};
use crate::icons::PhosphorIcon;
use crate::image_loader::CachedSpinner;
use crate::theme;
use egui::{Color32, RichText, Sense, Vec2};

/// Whether a listing can actually be bought.
///
/// An enum rather than a `bool` because "not buyable" is never the whole
/// story — the reader needs to know *why*, and the reasons behave differently
/// (one is permanent, one resolves itself, one just needs a different action).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Buyability {
    /// The datum resolved and the contract is one we can drive.
    #[default]
    Buyable,
    /// Listed, but a purchase cannot be constructed.
    Blocked(BlockedReason),
    /// Already sitting in the cart — the action has been taken.
    InCart,
}

/// Why a listing cannot be bought.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockedReason {
    /// The datum is stored by hash and no preimage is available anywhere —
    /// not from the indexer, not from the transaction that created the
    /// listing. Nothing can recover it, so this is permanent.
    DatumUnavailable,
    /// The contract version has no reference script or redeemer registered,
    /// so the validator cannot be supplied. Resolvable by registering one.
    UnsupportedContract,
    /// One member of a multi-asset bundle: the price shown is the whole
    /// bundle's, and the escrow can only be spent as a unit.
    BundleMember,
}

impl BlockedReason {
    /// Short chip text. Kept to a couple of words — the card is small and the
    /// tooltip carries the explanation.
    pub fn label(self) -> &'static str {
        match self {
            BlockedReason::DatumUnavailable => "No datum",
            BlockedReason::UnsupportedContract => "Unsupported",
            BlockedReason::BundleMember => "Bundle",
        }
    }

    /// The full reason, for the hover tooltip.
    pub fn detail(self) -> &'static str {
        match self {
            BlockedReason::DatumUnavailable => {
                "This listing's datum is stored by hash and the preimage is not \
                 recoverable from chain or the indexer, so the payouts it requires \
                 cannot be reconstructed."
            }
            BlockedReason::UnsupportedContract => {
                "No reference script is registered for this contract version, so the \
                 validator cannot be supplied to a spending transaction."
            }
            BlockedReason::BundleMember => {
                "Part of a multi-asset bundle. The price shown is for the whole \
                 bundle, which must be bought as a unit."
            }
        }
    }

    /// Every variant, for stories and exhaustive review.
    pub const ALL: [BlockedReason; 3] = [
        BlockedReason::DatumUnavailable,
        BlockedReason::UnsupportedContract,
        BlockedReason::BundleMember,
    ];
}

/// What the reader did to the grid this frame.
#[derive(Clone, Debug, Default)]
pub struct ListingGridResponse {
    /// Index of the card under the pointer, if any.
    pub hovered: Option<usize>,
    /// Index of a card whose add-to-cart affordance was clicked.
    pub add_to_cart: Option<usize>,
    /// Index of a card body that was clicked (open / inspect).
    pub clicked: Option<usize>,
}

/// A single listing card to display in the grid.
///
/// `Default` exists so a browse-only caller can construct one with
/// `..Default::default()` and stay unaffected as display-only fields are added.
#[derive(Clone, Default)]
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
    /// When set, this listing is one member of a multi-asset bundle of this
    /// size; `price_lovelace` is then the **whole-bundle total**. Rendered with
    /// a distinct "Bundle ×N" banner.
    pub bundle_size: Option<u32>,
    /// Whether this listing can be bought, and if not why. Defaults to
    /// [`Buyability::Buyable`] so browse-only callers need not care.
    pub buyability: Buyability,
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
            text_primary: crate::theme::TEXT_PRIMARY,
            text_muted: crate::theme::TEXT_MUTED,
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

    /// Draw the listing grid, reporting what the reader did this frame.
    pub fn show(&self, ui: &mut egui::Ui, listings: &[ListingCard]) -> ListingGridResponse {
        if listings.is_empty() {
            ui.label(
                RichText::new("No listings found")
                    .color(self.config.text_muted)
                    .size(11.0),
            );
            return ListingGridResponse::default();
        }

        let cfg = &self.config;
        // Card is square: thumbnail fills entire card, price banner overlays bottom
        let card_size = Vec2::splat(cfg.card_width);

        let inner = ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(cfg.spacing);
            let mut hovered_idx: Option<usize> = None;
            let mut add_to_cart_idx: Option<usize> = None;
            let mut clicked_idx: Option<usize> = None;
            let spinner = CachedSpinner::new(ui, 12.0, cfg.text_muted);
            let mut any_pending = false;

            for (card_idx, listing) in listings.iter().enumerate() {
                let is_filler = listing.gap_fill_count.is_some_and(|c| c > 0);
                let is_gap_mode = listing.gap_fill_count.is_some();
                let is_dimmed = is_gap_mode && !is_filler;

                let (rect, resp) = ui.allocate_exact_size(card_size, Sense::click());

                // `contains_pointer()`, NOT `hovered()`. This card hosts a
                // hover-revealed `CornerAction`, which is a later widget
                // occupying part of this same rect — and `hovered()` respects
                // occlusion, so the moment the pointer reaches that button the
                // CARD stops being "hovered". The button is only drawn while
                // the card is hovered, so it would vanish from under the
                // cursor, reappear the next frame, and never survive long
                // enough to complete a press→release. The add-to-cart was
                // unclickable for exactly this reason.
                //
                // `contains_pointer()` is geometric and cannot be stolen by a
                // child, which is what a hover-reveal control needs.
                let card_hovered = resp.contains_pointer();
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

                // Gap-fill banner (above the price banner). Suppressed on a
                // blocked listing — "fills 3 gaps" is a reason to buy, and
                // advertising it on something that cannot be bought competes
                // with the reason it cannot.
                let show_gap_banner = !matches!(listing.buyability, Buyability::Blocked(_));
                if let Some(count) = listing.gap_fill_count.filter(|c| *c > 0 && show_gap_banner) {
                    let gap_banner_rect = egui::Rect::from_min_size(
                        egui::pos2(rect.min.x, banner_rect.min.y - banner_h),
                        Vec2::new(cfg.card_width, banner_h),
                    );
                    ui.painter().rect_filled(
                        gap_banner_rect,
                        0,
                        Color32::from_rgba_premultiplied(158, 206, 106, 220),
                    );
                    ui.painter().text(
                        gap_banner_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("Fills {count}"),
                        egui::FontId::monospace(10.0),
                        Color32::from_rgb(26, 27, 38),
                    );
                }

                // Bundle banner (top of card) — independent of the gap-fill
                // banner so a bundle that ALSO fills gaps shows both. Flags that
                // the price below is the whole-bundle total, not a single asset.
                if let Some(n) = listing.bundle_size {
                    let bundle_rect =
                        egui::Rect::from_min_size(rect.min, Vec2::new(cfg.card_width, banner_h));
                    ui.painter().rect_filled(
                        bundle_rect,
                        egui::CornerRadius {
                            nw: cfg.rounding as u8,
                            ne: cfg.rounding as u8,
                            sw: 0,
                            se: 0,
                        },
                        Color32::from_rgba_premultiplied(224, 175, 104, 230),
                    );
                    ui.painter().text(
                        bundle_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("Bundle x{n}"),
                        egui::FontId::monospace(10.0),
                        Color32::from_rgb(26, 27, 38),
                    );
                }

                // Dim overlay for non-fillers
                if is_dimmed {
                    ui.painter().rect_filled(
                        rect,
                        cfg.rounding,
                        Color32::from_rgba_premultiplied(18, 19, 30, 120),
                    );
                }

                // Buyability treatment.
                //
                // A blocked listing is knocked back and labelled: it stays
                // visible (it IS the floor, and hiding it would misreport the
                // book) but must not read as actionable. A card already in the
                // cart is ringed rather than dimmed — it is not unavailable,
                // it is claimed.
                match listing.buyability {
                    Buyability::Buyable => {
                        // Add-to-cart lives on the card it acts on, and only
                        // appears under the pointer so a dense grid stays a
                        // grid of images rather than a grid of buttons.
                        if card_hovered {
                            let action = CornerAction::new(PhosphorIcon::Plus)
                                .corner(Corner::TopRight)
                                .tooltip("Add to cart")
                                .show(ui, rect, ("add-to-cart", listing.unit.as_str()));
                            if action.clicked() {
                                add_to_cart_idx = Some(card_idx);
                            }
                        }
                    }
                    Buyability::InCart => {
                        ui.painter().rect_stroke(
                            rect,
                            cfg.rounding,
                            egui::Stroke::new(2.0_f32, theme::ACCENT_GREEN),
                            egui::StrokeKind::Inside,
                        );
                        CornerAction::new(PhosphorIcon::Check)
                            .corner(Corner::TopRight)
                            .tooltip("In cart")
                            .show(ui, rect, ("in-cart", listing.unit.as_str()));
                    }
                    Buyability::Blocked(reason) => {
                        ui.painter().rect_filled(
                            rect,
                            cfg.rounding,
                            Color32::from_rgba_premultiplied(18, 19, 30, 150),
                        );
                        // Sits immediately above the price, in the card's
                        // status strip — NOT at the top, which the "Bundle ×N"
                        // banner already owns. Putting it there hid the banner
                        // on any bundle blocked for some other reason, which a
                        // screenshot caught and the code did not.
                        let chip_h = 14.0;
                        let chip_rect = egui::Rect::from_min_size(
                            egui::pos2(rect.min.x + 3.0, banner_rect.min.y - chip_h - 2.0),
                            Vec2::new(cfg.card_width - 6.0, chip_h),
                        );
                        ui.painter().rect_filled(
                            chip_rect,
                            3.0,
                            Color32::from_rgba_premultiplied(60, 30, 40, 230),
                        );
                        ui.painter().text(
                            chip_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            reason.label(),
                            egui::FontId::proportional(9.0),
                            theme::ACCENT_RED,
                        );
                    }
                }

                // Click opens Wayup
                if resp.clicked() {
                    clicked_idx = Some(card_idx);
                    #[cfg(target_arch = "wasm32")]
                    if let Some(ref market_url) = listing.marketplace_url {
                        if let Some(window) = web_sys::window() {
                            let _ = window.open_with_url_and_target(market_url, "wayup");
                        }
                    }
                }

                // Tooltip with name + gap/bundle info
                let price_line = match listing.bundle_size {
                    Some(n) => format!("{price_ada:.0} ADA (bundle of {n}, total)"),
                    None => format!("{price_ada:.0} ADA"),
                };
                // A blocked listing explains itself on hover. The chip has room
                // for two words; the reason needs a sentence.
                let blocked_line = match listing.buyability {
                    Buyability::Blocked(reason) => format!("\n\n{}", reason.detail()),
                    _ => String::new(),
                };
                if let Some(ref tooltip) = listing.gap_fill_tooltip {
                    resp.on_hover_text(format!(
                        "{}\n{price_line}\n{tooltip}{blocked_line}",
                        listing.name
                    ));
                } else {
                    resp.on_hover_text(format!("{}\n{price_line}{blocked_line}", listing.name));
                }
            }

            if any_pending {
                CachedSpinner::request_repaint(ui);
            }

            ListingGridResponse {
                hovered: hovered_idx,
                add_to_cart: add_to_cart_idx,
                clicked: clicked_idx,
            }
        });
        inner.inner
    }
}
