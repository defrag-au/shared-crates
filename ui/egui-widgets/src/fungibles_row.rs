//! Fungibles row — single horizontal row for a Cardano Native Token holding.
//!
//! Compact text format (no thumbnail, since most CNTs don't have one we can
//! render cheaply). Layout: optional icon · display_name · ticker chip
//! (optional) · stretch · quantity (right-aligned) · optional value.
//!
//! Designed for wallet-viewer's CNT list. Generic enough for any
//! "fungible holdings list" use case.
//!
//! Stateless.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::fungibles_row::FungiblesRow;
//!
//! FungiblesRow::new("IAGON", "3,365.23")
//!     .ticker(Some("IAG"))
//!     .show(ui);
//! ```

use egui::{CornerRadius, FontId, RichText, Sense, Ui, Vec2};

use crate::PhosphorIcon;
use crate::theme::{Ink, InkExt, Space, SpaceExt, ThemeExt, Token};

/// Styling knobs.
pub struct FungiblesRowConfig {
    pub icon_size: f32,
    pub icon_color: Ink,
    pub name_size: f32,
    pub name_color: Ink,
    pub ticker_size: f32,
    pub ticker_text_color: Ink,
    pub ticker_bg: Ink,
    pub ticker_corner_radius: u8,
    pub ticker_padding_x: f32,
    pub ticker_padding_y: f32,
    pub qty_size: f32,
    pub qty_color: Ink,
    pub value_size: f32,
    pub value_color: Ink,
    pub row_padding_y: f32,
    pub show_default_icon: bool,
}

impl Default for FungiblesRowConfig {
    fn default() -> Self {
        Self {
            icon_size: 14.0,
            icon_color: Ink::Token(Token::TextSecondary),
            name_size: 13.0,
            name_color: Ink::Token(Token::TextPrimary),
            ticker_size: 9.0,
            ticker_text_color: Ink::Token(Token::TextSecondary),
            ticker_bg: Ink::Token(Token::BgHighlight),
            ticker_corner_radius: 6,
            ticker_padding_x: 5.0,
            ticker_padding_y: 1.0,
            qty_size: 13.0,
            qty_color: Ink::Token(Token::TextPrimary),
            value_size: 11.0,
            value_color: Ink::Token(Token::TextSecondary),
            row_padding_y: 4.0,
            show_default_icon: true,
        }
    }
}

/// Builder for a fungibles row.
pub struct FungiblesRow<'a> {
    display_name: &'a str,
    quantity_text: &'a str,
    ticker: Option<&'a str>,
    value_text: Option<&'a str>,
    config: FungiblesRowConfig,
}

impl<'a> FungiblesRow<'a> {
    /// Create a new row. `quantity_text` is pre-formatted (callers know
    /// the decimals; this widget never does the math itself).
    pub fn new(display_name: &'a str, quantity_text: &'a str) -> Self {
        Self {
            display_name,
            quantity_text,
            ticker: None,
            value_text: None,
            config: FungiblesRowConfig::default(),
        }
    }

    /// Optional ticker chip (e.g. "IAG", "SNEK") next to the name.
    pub fn ticker(mut self, ticker: Option<&'a str>) -> Self {
        self.ticker = ticker;
        self
    }

    /// Optional small ADA value below or beside the quantity.
    pub fn value_text(mut self, value_text: Option<&'a str>) -> Self {
        self.value_text = value_text;
        self
    }

    pub fn with_config(mut self, config: FungiblesRowConfig) -> Self {
        self.config = config;
        self
    }

    pub fn show(self, ui: &mut Ui) {
        let cfg = &self.config;
        ui.add_space(cfg.row_padding_y);
        ui.horizontal(|ui| {
            if cfg.show_default_icon {
                PhosphorIcon::Coins.show(ui, cfg.icon_size, cfg.icon_color.of(ui));
                ui.gap(Space::Sm);
            }
            ui.label(
                RichText::new(self.display_name)
                    .size(cfg.name_size)
                    .color(cfg.name_color.of(ui))
                    .strong(),
            );
            if let Some(t) = self.ticker {
                ui.gap(Space::Sm);
                draw_chip(ui, t, cfg);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(v) = self.value_text {
                    ui.label(
                        RichText::new(v)
                            .size(cfg.value_size)
                            .color(cfg.value_color.of(ui)),
                    );
                    ui.gap(Space::Base);
                }
                ui.label(
                    RichText::new(self.quantity_text)
                        .size(cfg.qty_size)
                        .color(cfg.qty_color.of(ui))
                        .monospace(),
                );
            });
        });
        ui.add_space(cfg.row_padding_y);
    }
}

fn draw_chip(ui: &mut Ui, label: &str, cfg: &FungiblesRowConfig) {
    let t = ui.tokens();
    let ticker_text = cfg.ticker_text_color.resolve(&t);
    let ticker_bg = cfg.ticker_bg.resolve(&t);
    let font = FontId::proportional(cfg.ticker_size);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, ticker_text);
    let size = Vec2::new(
        galley.size().x + cfg.ticker_padding_x * 2.0,
        galley.size().y + cfg.ticker_padding_y * 2.0,
    );
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            CornerRadius::same(cfg.ticker_corner_radius),
            ticker_bg,
        );
        let pos = egui::pos2(
            rect.min.x + cfg.ticker_padding_x,
            rect.min.y + cfg.ticker_padding_y,
        );
        painter.galley(pos, galley, ticker_text);
    }
}
