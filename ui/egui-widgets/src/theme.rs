//! Tokyo Night Dark theme — shared palette and style configuration.
//!
//! All defrag egui frontends share this color palette. Individual apps
//! can customize font strategy via [`FontStrategy`] when calling
//! [`configure_style`].

use egui::{Color32, FontId, Stroke, TextStyle, Visuals};

// ============================================================================
// Background colors
// ============================================================================

pub const BG_PRIMARY: Color32 = Color32::from_rgb(26, 27, 38);
pub const BG_SECONDARY: Color32 = Color32::from_rgb(36, 40, 59);
pub const BG_HIGHLIGHT: Color32 = Color32::from_rgb(41, 46, 66);

// ============================================================================
// Text colors
// ============================================================================

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(192, 202, 245);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(120, 130, 170);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(86, 95, 137);

// ============================================================================
// Accent colors (full palette — apps pick the aliases they prefer)
// ============================================================================

pub const ACCENT_BLUE: Color32 = Color32::from_rgb(122, 162, 247);
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(125, 207, 255);
pub const ACCENT_GREEN: Color32 = Color32::from_rgb(158, 206, 106);
pub const ACCENT_YELLOW: Color32 = Color32::from_rgb(224, 175, 104);
pub const ACCENT_RED: Color32 = Color32::from_rgb(247, 118, 142);
pub const ACCENT_MAGENTA: Color32 = Color32::from_rgb(187, 154, 247);

// ============================================================================
// Semantic aliases (convenience for common patterns)
// ============================================================================

/// Primary call-to-action accent.
pub const ACCENT: Color32 = ACCENT_BLUE;
/// Positive / success status.
pub const SUCCESS: Color32 = ACCENT_GREEN;
/// Warning / caution status.
pub const WARNING: Color32 = ACCENT_YELLOW;
/// Error / danger status.
pub const ERROR: Color32 = ACCENT_RED;
/// Default border stroke color.
pub const BORDER: Color32 = BG_HIGHLIGHT;

// ============================================================================
// Style configuration
// ============================================================================

/// Controls whether the app uses monospace or proportional fonts.
pub enum FontStrategy {
    /// All text styles use monospace (dashboard feel).
    Monospace {
        body: f32,
        small: f32,
        heading: f32,
        button: f32,
    },
    /// Body/heading use proportional, monospace for code.
    Proportional {
        body: f32,
        small: f32,
        heading: f32,
        button: f32,
        monospace: f32,
    },
}

impl FontStrategy {
    /// Monospace preset matching collection-ownership defaults.
    pub fn monospace() -> Self {
        Self::Monospace {
            body: 13.0,
            small: 11.0,
            heading: 16.0,
            button: 13.0,
        }
    }

    /// Proportional preset matching rewards defaults.
    pub fn proportional() -> Self {
        Self::Proportional {
            body: 14.0,
            small: 12.0,
            heading: 20.0,
            button: 14.0,
            monospace: 13.0,
        }
    }
}

/// Apply the Tokyo Night Dark theme to an egui context.
///
/// Pass a [`FontStrategy`] to control font rendering. Call once at startup.
pub fn configure_style(ctx: &egui::Context, fonts: FontStrategy) {
    let mut style = (*ctx.style()).clone();

    match fonts {
        FontStrategy::Monospace {
            body,
            small,
            heading,
            button,
        } => {
            style
                .text_styles
                .insert(TextStyle::Body, FontId::monospace(body));
            style
                .text_styles
                .insert(TextStyle::Small, FontId::monospace(small));
            style
                .text_styles
                .insert(TextStyle::Heading, FontId::monospace(heading));
            style
                .text_styles
                .insert(TextStyle::Button, FontId::monospace(button));
            style
                .text_styles
                .insert(TextStyle::Monospace, FontId::monospace(body));
        }
        FontStrategy::Proportional {
            body,
            small,
            heading,
            button,
            monospace,
        } => {
            style
                .text_styles
                .insert(TextStyle::Body, FontId::proportional(body));
            style
                .text_styles
                .insert(TextStyle::Small, FontId::proportional(small));
            style
                .text_styles
                .insert(TextStyle::Heading, FontId::proportional(heading));
            style
                .text_styles
                .insert(TextStyle::Button, FontId::proportional(button));
            style
                .text_styles
                .insert(TextStyle::Monospace, FontId::monospace(monospace));
        }
    }

    let mut visuals = Visuals::dark();
    visuals.panel_fill = BG_PRIMARY;
    visuals.window_fill = BG_SECONDARY;
    visuals.extreme_bg_color = BG_PRIMARY;
    visuals.faint_bg_color = BG_SECONDARY;

    visuals.widgets.noninteractive.bg_fill = BG_SECONDARY;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);

    visuals.widgets.inactive.bg_fill = BG_SECONDARY;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);

    visuals.widgets.hovered.bg_fill = BG_HIGHLIGHT;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);

    visuals.widgets.active.bg_fill = BG_HIGHLIGHT;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

    visuals.selection.bg_fill = Color32::from_rgba_premultiplied(122, 162, 247, 40);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    style.visuals = visuals;

    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);

    ctx.set_style(style);
}
