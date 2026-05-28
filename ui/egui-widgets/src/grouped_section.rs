//! Grouped section header — hero icon + title + verified badge +
//! subtitle + right-aligned bulk-action button, with caller-rendered
//! body below.
//!
//! Pattern lifted from the jpg-store-mirror's "My Offers" view
//! where offers are grouped by collection. Same shape recurs in
//! any "things grouped by parent entity" view: wallet by chain,
//! pool by DEX, listings by collection, etc.
//!
//! Stateless. The caller drives `is_verified` / `subtitle` / the
//! optional bulk action; the widget handles layout, theming, and
//! the verified-badge glyph.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::grouped_section::{GroupedSection, GroupedSectionAction};
//!
//! let action = GroupedSection::new("Black Flag")
//!     .hero_url(Some("https://.../hero.webp".to_owned()))
//!     .verified(true)
//!     .subtitle("12 unspent")
//!     .bulk_action(eligible_count > 1, format!("Add all ({eligible_count})"))
//!     .show(ui, |ui| {
//!         // body — render tiles, rows, whatever
//!     });
//!
//! if matches!(action, Some(GroupedSectionAction::BulkAction)) {
//!     // ...
//! }
//! ```

use egui::{RichText, Ui};

use crate::theme;
use crate::PhosphorIcon;

/// Click events the section can produce. Today only `BulkAction`
/// (the right-aligned header button); future variants could add
/// header-click for collapsing or hero-click for navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupedSectionAction {
    /// Right-aligned bulk-action button was clicked.
    BulkAction,
}

/// Layout + theming knobs for `GroupedSection`. Consumers
/// override colours via the field-mutator pattern; default values
/// match `egui_widgets::theme`.
pub struct GroupedSectionConfig {
    pub hero_size: f32,
    pub hero_corner_radius: u8,
    pub hero_placeholder_width: f32,
    pub title_size: f32,
    pub title_color: egui::Color32,
    pub subtitle_size: f32,
    pub subtitle_color: egui::Color32,
    pub badge_color: egui::Color32,
    pub bulk_button_color: egui::Color32,
    pub bulk_button_size: f32,
    pub gap_after_header: f32,
}

impl Default for GroupedSectionConfig {
    fn default() -> Self {
        Self {
            hero_size: 32.0,
            hero_corner_radius: 4,
            // Match the hero footprint when there's no image so
            // the title column lines up across mixed-state groups.
            hero_placeholder_width: 36.0,
            title_size: 13.0,
            title_color: theme::TEXT_PRIMARY,
            subtitle_size: 10.0,
            subtitle_color: theme::TEXT_MUTED,
            badge_color: theme::ACCENT_GREEN,
            bulk_button_color: theme::ACCENT_CYAN,
            bulk_button_size: 10.0,
            gap_after_header: 6.0,
        }
    }
}

/// Grouped section header + body. See module docs.
pub struct GroupedSection<'a> {
    title: &'a str,
    hero: Option<egui::ImageSource<'static>>,
    is_verified: bool,
    subtitle: Option<&'a str>,
    bulk_button: Option<(bool, String)>,
    config: GroupedSectionConfig,
}

impl<'a> GroupedSection<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            hero: None,
            is_verified: false,
            subtitle: None,
            bulk_button: None,
            config: GroupedSectionConfig::default(),
        }
    }

    /// URL to load as the section's hero icon. `None` reserves
    /// the same horizontal space as a rendered hero so titles
    /// line up across heterogenous groups. Convenience wrapper
    /// over `hero_image(ImageSource::Uri(...))` for the typical
    /// HTTP/IIIF case.
    pub fn hero_url(mut self, url: Option<String>) -> Self {
        self.hero = url.map(|s| egui::ImageSource::Uri(s.into()));
        self
    }

    /// Hero from any `egui::ImageSource` — supports URLs (via
    /// `ImageSource::Uri`), bytes embedded at compile time (via
    /// `egui::include_image!`), or a pre-loaded texture. Storybook
    /// uses this for embedded placeholder PNGs; production code
    /// typically prefers `hero_url(Some(...))`.
    pub fn hero_image(mut self, source: egui::ImageSource<'static>) -> Self {
        self.hero = Some(source);
        self
    }

    /// When true, paint a `CheckCircle` glyph next to the title.
    pub fn verified(mut self, is_verified: bool) -> Self {
        self.is_verified = is_verified;
        self
    }

    /// One-line subtitle painted below the title. Typical use:
    /// counts, status summaries.
    pub fn subtitle(mut self, subtitle: &'a str) -> Self {
        self.subtitle = Some(subtitle);
        self
    }

    /// Right-aligned bulk-action button. The first arg gates
    /// visibility (commonly `eligible_count > 1`); the second is
    /// the button label. The widget reports a `BulkAction` click
    /// via the return value of `show`.
    pub fn bulk_action(mut self, visible: bool, label: impl Into<String>) -> Self {
        self.bulk_button = Some((visible, label.into()));
        self
    }

    pub fn with_config(mut self, config: GroupedSectionConfig) -> Self {
        self.config = config;
        self
    }

    /// Render the section. `body` paints the caller's content
    /// below the header (offers, rows, charts, whatever).
    pub fn show<R>(
        self,
        ui: &mut Ui,
        body: impl FnOnce(&mut Ui) -> R,
    ) -> (Option<GroupedSectionAction>, R) {
        let cfg = &self.config;
        let mut action = None;

        ui.horizontal(|ui| {
            // Hero column — always reserve the same width so
            // titles align across image + placeholder rows.
            if let Some(hero) = self.hero {
                ui.add(
                    egui::Image::new(hero)
                        .fit_to_exact_size(egui::vec2(cfg.hero_size, cfg.hero_size))
                        .corner_radius(egui::CornerRadius::same(cfg.hero_corner_radius)),
                );
            } else {
                ui.add_space(cfg.hero_placeholder_width);
            }
            ui.add_space(8.0);

            // Title + subtitle column. The bulk-action button sits
            // inline after the title rather than far-right of the
            // panel — keeps it visually anchored to the content
            // it acts on, especially in wide layouts where a
            // right-aligned button drifts away from the title.
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(self.title)
                            .color(cfg.title_color)
                            .size(cfg.title_size)
                            .strong(),
                    );
                    if self.is_verified {
                        ui.label(PhosphorIcon::CheckCircle.rich_text(12.0, cfg.badge_color));
                    }
                    if let Some((visible, label)) = &self.bulk_button {
                        if *visible {
                            ui.add_space(8.0);
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(label)
                                            .color(cfg.bulk_button_color)
                                            .size(cfg.bulk_button_size),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                action = Some(GroupedSectionAction::BulkAction);
                            }
                        }
                    }
                });
                if let Some(subtitle) = self.subtitle {
                    ui.label(
                        RichText::new(subtitle)
                            .color(cfg.subtitle_color)
                            .size(cfg.subtitle_size),
                    );
                }
            });
        });

        ui.add_space(cfg.gap_after_header);
        let body_ret = body(ui);
        (action, body_ret)
    }
}
