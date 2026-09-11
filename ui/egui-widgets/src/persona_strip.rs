//! Persona strip — italic one-liner describing a wallet (or any tagged
//! entity), with an optional row of small tag chips beneath.
//!
//! Designed for wallet-viewer's persona summary ("ADA-leaning Large with
//! blue-chip taste") but generic — any "give me a tagged tagline" view
//! works: collection persona, holder personality, etc.
//!
//! Stateless. Pure rendering.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::persona_strip::PersonaStrip;
//!
//! PersonaStrip::new("ADA-leaning Large with blue-chip taste")
//!     .tags(&["ada_maximalist", "large", "specialist"])
//!     .show(ui);
//! ```

use egui::{Color32, CornerRadius, FontId, RichText, Sense, Ui, Vec2};

use crate::theme::ThemeExt;

/// Styling knobs.
pub struct PersonaStripConfig {
    pub headline_size: f32,
    /// `None` asks the theme at render time — a `Default` has no `Ui` to ask.
    pub headline_color: Option<Color32>,
    pub chip_text_size: f32,
    pub chip_text_color: Option<Color32>,
    pub chip_bg: Option<Color32>,
    pub chip_corner_radius: u8,
    pub chip_padding_x: f32,
    pub chip_padding_y: f32,
    pub chip_gap: f32,
    pub headline_to_chips_gap: f32,
}

impl Default for PersonaStripConfig {
    fn default() -> Self {
        Self {
            headline_size: 13.0,
            headline_color: None,
            chip_text_size: 10.0,
            chip_text_color: None,
            chip_bg: None,
            chip_corner_radius: 8,
            chip_padding_x: 6.0,
            chip_padding_y: 2.0,
            chip_gap: 4.0,
            headline_to_chips_gap: 4.0,
        }
    }
}

/// The persona strip builder.
pub struct PersonaStrip<'a> {
    headline: &'a str,
    tags: &'a [&'a str],
    config: PersonaStripConfig,
}

impl<'a> PersonaStrip<'a> {
    pub fn new(headline: &'a str) -> Self {
        Self {
            headline,
            tags: &[],
            config: PersonaStripConfig::default(),
        }
    }

    /// Optional list of tag labels to render as chips beneath the headline.
    pub fn tags(mut self, tags: &'a [&'a str]) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_config(mut self, config: PersonaStripConfig) -> Self {
        self.config = config;
        self
    }

    pub fn show(self, ui: &mut Ui) {
        let cfg = &self.config;

        if !self.headline.is_empty() {
            ui.label(
                RichText::new(self.headline)
                    .italics()
                    .size(cfg.headline_size)
                    .color(
                        cfg.headline_color
                            .unwrap_or(ui.tokens().color.text_secondary),
                    ),
            );
        }

        if !self.tags.is_empty() {
            ui.add_space(cfg.headline_to_chips_gap);
            ui.horizontal_wrapped(|ui| {
                for tag in self.tags {
                    draw_chip(ui, tag, cfg);
                    ui.add_space(cfg.chip_gap);
                }
            });
        }
    }
}

fn draw_chip(ui: &mut Ui, label: &str, cfg: &PersonaStripConfig) {
    let t = ui.tokens();
    let chip_text = cfg.chip_text_color.unwrap_or(t.color.text_secondary);
    let chip_bg = cfg.chip_bg.unwrap_or(t.color.bg_highlight);
    let font = FontId::proportional(cfg.chip_text_size);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), chip_text);
    let size = Vec2::new(
        galley.size().x + cfg.chip_padding_x * 2.0,
        galley.size().y + cfg.chip_padding_y * 2.0,
    );
    let (rect, _resp) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(cfg.chip_corner_radius), chip_bg);
        let text_pos = egui::pos2(
            rect.min.x + cfg.chip_padding_x,
            rect.min.y + cfg.chip_padding_y,
        );
        painter.galley(text_pos, galley, chip_text);
    }
}
