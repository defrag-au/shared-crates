//! `OfferTile` — fixed-size picker tile with state-aware visual
//! treatment and a top-right quantity badge.
//!
//! Pattern lifted from the jpg-store-mirror's "My Offers" view
//! where each tile represents either an asset-specific offer (an
//! IIIF asset thumbnail) or a stacked collection-wide offer at a
//! given price (a "CO" placeholder + quantity). Same shape recurs
//! anywhere a UI lets the user click through inventory items
//! that have an active/dimmed state machine — wallet pickers,
//! cart-eligibility grids, claim queues.
//!
//! ## State machine
//!
//! - [`OfferTileState::Active`] — full colour. Tile responds to
//!   click; the consumer dispatches whatever action they want.
//! - [`OfferTileState::InCart`] — already queued for the next
//!   action (cart, swap, claim). Tile dims, click is suppressed
//!   so duplicate-add gating happens at the UI layer.
//! - [`OfferTileState::Spent`] — already actioned on-chain
//!   (cancelled, accepted, swapped). Same dim treatment as
//!   in-cart but the tooltip can disambiguate why.
//!
//! ## Content variants
//!
//! - [`OfferTileContent::Image`] — any `egui::ImageSource`. URLs,
//!   `egui::include_image!`, pre-loaded textures.
//! - [`OfferTileContent::Placeholder`] — centred text rendered
//!   in place of an image. Used when there's no specific asset
//!   to depict (collection offers, untyped queue items).
//!
//! ## Badge links
//!
//! [`OfferTile::badge_link`] gives the corner badge a destination:
//! it underlines on hover, shows the pointing hand, and opens the
//! URL in a new tab. The badge owns that click, so the tile's own
//! click — "act on this tile" — keeps the rest of its surface.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::offer_tile::{OfferTile, OfferTileContent, OfferTileState};
//!
//! let resp = OfferTile::image("5.0 ADA", egui::ImageSource::Uri(url.into()))
//!     .state(OfferTileState::Active)
//!     .badge("3 left")
//!     .tooltip("Click to add to cart")
//!     .show(ui);
//!
//! if resp.clicked() {
//!     // dispatch action
//! }
//! ```

use egui::{Color32, RichText, Sense, Ui, Vec2};

use crate::theme::{Ink, Space, SpaceExt, ThemeExt, Token};

/// Tile state — drives frame fill, image tint, and click gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OfferTileState {
    /// Available + responsive to clicks.
    #[default]
    Active,
    /// Already queued (e.g. in cart). Dimmed; click suppressed.
    InCart,
    /// Already spent on chain. Dimmed; click suppressed.
    Spent,
}

impl OfferTileState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// What fills the main square area of the tile.
pub enum OfferTileContent<'a> {
    /// IIIF / CDN URL or any `ImageSource`.
    Image(egui::ImageSource<'a>),
    /// Centred text glyph, used when the tile represents
    /// something abstract (e.g. "CO" for a collection offer
    /// stack with no specific target asset).
    Placeholder {
        /// The glyph text to render. Typically 1–3 chars.
        text: &'a str,
    },
}

/// Layout + theming knobs.
pub struct OfferTileConfig {
    pub tile_size: f32,
    pub frame_corner_radius: u8,
    pub image_corner_radius: u8,
    pub frame_inner_margin: i8,
    pub bg_active: Ink,
    pub bg_dimmed: Ink,
    /// Recessed fill behind a placeholder glyph. Painted inside
    /// the inner content area so placeholder tiles have visual
    /// weight matching image tiles (otherwise the glyph floats on
    /// the frame fill and the tile looks empty by comparison).
    pub placeholder_bg_active: Ink,
    pub placeholder_bg_dimmed: Ink,
    pub text_primary: Ink,
    pub text_muted: Ink,
    pub badge_active: Ink,
    pub badge_dimmed: Ink,
    pub price_size: f32,
    pub badge_size: f32,
    pub placeholder_glyph_size: f32,
    /// Alpha applied to the image when the tile is dimmed.
    pub dimmed_image_alpha: u8,
}

impl Default for OfferTileConfig {
    fn default() -> Self {
        Self {
            tile_size: 110.0,
            frame_corner_radius: 6,
            image_corner_radius: 4,
            frame_inner_margin: 4,
            bg_active: Ink::Token(Token::BgSecondary),
            bg_dimmed: Ink::Token(Token::BgPrimary),
            // The primary background sits a shade away from the secondary one,
            // giving the placeholder area an inset look against the active
            // frame. The dimmed frame is already `BgPrimary`, so its
            // placeholder needs a different surface or the content area
            // vanishes into it — a partial step towards `BgSecondary` reads as
            // distinct without guessing at "one shade darker", which no token
            // names and which inverts between a dark and a light theme.
            placeholder_bg_active: Ink::Token(Token::BgPrimary),
            placeholder_bg_dimmed: Ink::Wash(Token::BgSecondary, 90),
            text_primary: Ink::Token(Token::TextPrimary),
            text_muted: Ink::Token(Token::TextMuted),
            badge_active: Ink::Token(Token::AccentCyan),
            badge_dimmed: Ink::Token(Token::TextMuted),
            price_size: 11.0,
            badge_size: 14.0,
            placeholder_glyph_size: 36.0,
            dimmed_image_alpha: 96,
        }
    }
}

impl OfferTileConfig {
    /// The space one tile occupies, frame and price included.
    ///
    /// Public so a caller laying out a grid of these can work out where a tile
    /// WOULD land before drawing it, and skip the ones that fall outside the
    /// viewport. egui has no damage tracking and culls only at tessellation,
    /// so an off-screen tile is otherwise built in full every frame — its text
    /// laid out, its image requested. A capture of one such grid showed 175
    /// image loads and 345 text layouts in a single 33ms frame.
    ///
    /// A method rather than a constant because it is derived from two knobs a
    /// caller may have changed, and the same arithmetic copied into the caller
    /// is the kind that drifts silently once one of them moves.
    pub fn outer_size(&self) -> Vec2 {
        Vec2::new(
            self.tile_size + 8.0,
            self.tile_size + self.price_size + 12.0,
        )
    }
}

/// Picker tile widget. Sized fixed at `config.tile_size + price`
/// so a `horizontal_wrapped` layout with multiple tiles aligns
/// cleanly across rows.
pub struct OfferTile<'a> {
    content: OfferTileContent<'a>,
    state: OfferTileState,
    price: String,
    badge: Option<String>,
    badge_url: Option<String>,
    tooltip: Option<String>,
    config: OfferTileConfig,
}

impl<'a> OfferTile<'a> {
    /// Image-content tile. `price` is a pre-formatted display
    /// string (e.g. `"5.0 ADA"`, `"5.0 ADA (+2)"`); the widget
    /// doesn't unit-convert lovelace.
    pub fn image(price: impl Into<String>, image: egui::ImageSource<'a>) -> Self {
        Self::new(OfferTileContent::Image(image), price)
    }

    /// Placeholder-text tile (e.g. `"CO"` for a stacked
    /// collection-offer tile with no specific target asset).
    pub fn placeholder(price: impl Into<String>, label: &'a str) -> Self {
        Self::new(OfferTileContent::Placeholder { text: label }, price)
    }

    fn new(content: OfferTileContent<'a>, price: impl Into<String>) -> Self {
        Self {
            content,
            state: OfferTileState::Active,
            price: price.into(),
            badge: None,
            badge_url: None,
            tooltip: None,
            config: OfferTileConfig::default(),
        }
    }

    pub fn state(mut self, state: OfferTileState) -> Self {
        self.state = state;
        self
    }

    /// Top-right corner badge. Typical content: stack quantity
    /// (`"8×"`, `"5 left"`) or multi-asset hint (`"+2"`).
    pub fn badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    /// Corner badge that is also a link to the place it names (a
    /// marketplace page, say): clicking it opens `url` in a new tab
    /// and does not activate the tile.
    ///
    /// `text` is what the badge reads and what the hover tooltip is
    /// phrased from ("Open Wayup in a new tab").
    pub fn badge_link(mut self, text: impl Into<String>, url: impl Into<String>) -> Self {
        self.badge = Some(text.into());
        self.badge_url = Some(url.into());
        self
    }

    /// Hover tooltip text. State-disambiguating callers pass a
    /// different string per state so the tooltip explains *why*
    /// a tile is dimmed (in cart vs. spent).
    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn with_config(mut self, config: OfferTileConfig) -> Self {
        self.config = config;
        self
    }

    /// Render the tile and return its `Response`. Consumers check
    /// `response.clicked()` (which only fires when the tile is
    /// active — dimmed tiles ignore clicks).
    pub fn show(self, ui: &mut Ui) -> egui::Response {
        let cfg = &self.config;
        // Resolved once: a `Default` config names its tokens, it cannot hold
        // values.
        let t = ui.tokens();
        let dimmed = !self.state.is_active();
        let frame_fill = if dimmed {
            cfg.bg_dimmed.resolve(&t)
        } else {
            cfg.bg_active.resolve(&t)
        };
        let badge_color = if dimmed {
            cfg.badge_dimmed.resolve(&t)
        } else {
            cfg.badge_active.resolve(&t)
        };
        let text_color = if dimmed {
            cfg.text_muted.resolve(&t)
        } else {
            cfg.text_primary.resolve(&t)
        };
        let placeholder_bg_active = cfg.placeholder_bg_active.resolve(&t);
        let placeholder_bg_dimmed = cfg.placeholder_bg_dimmed.resolve(&t);

        // Outer container — fixed size so wrapping rows align.
        // Height = tile + price label + a hair of spacing.
        let outer_size = cfg.outer_size();

        let inner = ui.allocate_ui_with_layout(
            outer_size,
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                egui::Frame::default()
                    .inner_margin(egui::Margin::same(cfg.frame_inner_margin))
                    .corner_radius(egui::CornerRadius::same(cfg.frame_corner_radius))
                    .fill(frame_fill)
                    .show(ui, |ui| {
                        let content_rect = match &self.content {
                            OfferTileContent::Image(image) => {
                                let mut img = egui::Image::new(image.clone())
                                    .fit_to_exact_size(Vec2::splat(cfg.tile_size))
                                    .corner_radius(egui::CornerRadius::same(
                                        cfg.image_corner_radius,
                                    ));
                                if dimmed {
                                    img =
                                        img.tint(Color32::from_white_alpha(cfg.dimmed_image_alpha));
                                }
                                ui.add(img).rect
                            }
                            OfferTileContent::Placeholder { text } => {
                                let (rect, _) = ui.allocate_exact_size(
                                    Vec2::splat(cfg.tile_size),
                                    Sense::hover(),
                                );
                                let painter = ui.painter_at(rect);
                                // Recessed fill — the placeholder
                                // counterpart of the image-tile's
                                // image content. Without it the
                                // glyph floats on the frame and
                                // the tile reads as empty.
                                let placeholder_bg = if dimmed {
                                    placeholder_bg_dimmed
                                } else {
                                    placeholder_bg_active
                                };
                                painter.rect_filled(
                                    rect,
                                    egui::CornerRadius::same(cfg.image_corner_radius),
                                    placeholder_bg,
                                );
                                painter.text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    text,
                                    egui::FontId::proportional(cfg.placeholder_glyph_size),
                                    text_color,
                                );
                                rect
                            }
                        };

                        ui.gap(Space::Xs);
                        ui.label(
                            RichText::new(&self.price)
                                .color(text_color)
                                .size(cfg.price_size)
                                .strong(),
                        );

                        // The badge's anchor rect, for `show` to paint once
                        // the link's hover state is known to it.
                        content_rect
                    })
                    .inner
            },
        );

        // Whole-tile click sense + cursor feedback. Clicks only
        // count when the tile is active; the consumer treats a
        // `clicked()` on a dimmed tile as a no-op via the state
        // check.
        let response = inner.response.interact(Sense::click());

        // Corner badge, painted as an overlay rather than allocated
        // inline so image tiles don't reserve extra vertical space for
        // it. Registered after the tile's click sense: a widget added
        // later wins the hit-test tie, so a badge link takes the click
        // in its corner and the tile keeps the rest of its surface.
        let mut badge_link: Option<egui::Response> = None;
        if let Some(badge_text) = &self.badge {
            let galley = ui.painter().layout_no_wrap(
                badge_text.clone(),
                egui::FontId::proportional(cfg.badge_size),
                badge_color,
            );
            let anchor = inner.inner.right_top() + Vec2::new(-6.0, 6.0);
            let badge_rect = egui::Rect::from_min_size(
                egui::pos2(anchor.x - galley.size().x, anchor.y),
                galley.size(),
            );

            let mut underline = None;
            if let Some(url) = self.badge_url.as_deref() {
                let link = ui.interact(
                    badge_rect.expand(2.0),
                    response.id.with("badge"),
                    Sense::click(),
                );
                if link.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    underline = Some(egui::Stroke::new(1.0, badge_color));
                }
                if link.clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
                badge_link = Some(link);
            }

            let mut shape = egui::epaint::TextShape::new(badge_rect.min, galley, badge_color);
            if let Some(stroke) = underline {
                shape = shape.with_underline(stroke);
            }
            ui.painter().add(shape);
        }

        if self.state.is_active() && response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        // While the pointer is on the badge link, the tile's tooltip would
        // describe the cart action a click there does not take, so the badge
        // takes the tooltip slot instead (egui shows one tooltip per layer).
        if let Some(link) = badge_link
            && link.hovered()
            && let Some(badge_text) = &self.badge
        {
            link.on_hover_text(format!("Open {badge_text} in a new tab"));
            return response;
        }

        if let Some(tooltip) = self.tooltip {
            response.on_hover_text(tooltip)
        } else {
            response
        }
    }
}
