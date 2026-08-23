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
//
// The ramp is tiered for hierarchy but every tier must stay readable: most of
// this suite renders at 9-12px, where WCAG AA demands 4.5:1. Floors are
// enforced against all three backgrounds by tests/contrast.rs — if you dim a
// tier, that test is the negotiation, not your monitor.
// ============================================================================

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(192, 202, 245);
/// Tokyo Night `fg_dark` — ~6.9:1 on `BG_SECONDARY`.
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(169, 177, 214);
/// De-emphasis tier, but still AA at small sizes — ~5.0:1 on `BG_SECONDARY`.
/// (The previous `#565F89` sat at 2.2-2.8:1 and carried real copy.)
pub const TEXT_MUTED: Color32 = Color32::from_rgb(139, 149, 196);

// ============================================================================
// Accent colors (full palette — apps pick the aliases they prefer)
// ============================================================================

pub const ACCENT_BLUE: Color32 = Color32::from_rgb(122, 162, 247);
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(125, 207, 255);
pub const ACCENT_GREEN: Color32 = Color32::from_rgb(158, 206, 106);
pub const ACCENT_YELLOW: Color32 = Color32::from_rgb(224, 175, 104);
pub const ACCENT_ORANGE: Color32 = Color32::from_rgb(255, 158, 100);
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
/// Default border stroke color. Deliberately its own value: when this aliased
/// `BG_HIGHLIGHT` it sat at 1.24:1 against `BG_PRIMARY` and panel edges were
/// effectively invisible.
pub const BORDER: Color32 = Color32::from_rgb(65, 72, 104);

// ============================================================================
// Rarity rank coloring
// ============================================================================

/// Color a rarity rank by its percentile within a collection.
///
/// Returns a color that communicates scarcity at a glance:
/// - Gold for top 1%, amber for top 5%, cyan for top 10%, green for top 25%,
///   muted for everything else.
///
/// Used by offer slots, browse views, pricing panels — anywhere a `#rank`
/// label is displayed.
pub fn rarity_rank_color(rank: u32, total: u32) -> Color32 {
    if total == 0 {
        return TEXT_MUTED;
    }
    let pct = rank as f32 / total as f32;
    if pct <= 0.01 {
        Color32::from_rgb(255, 215, 0) // gold — top 1%
    } else if pct <= 0.05 {
        ACCENT_YELLOW // amber — top 5%
    } else if pct <= 0.10 {
        ACCENT_CYAN // cyan — top 10%
    } else if pct <= 0.25 {
        ACCENT_GREEN // green — top 25%
    } else {
        TEXT_MUTED
    }
}

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
    // These are dark-only palettes, but egui's web default follows
    // prefers-color-scheme and set_global_style only writes the ACTIVE theme's
    // style — pin dark first so a light-mode device gets the same app.
    ctx.set_theme(egui::ThemePreference::Dark);

    let mut style = (*ctx.global_style()).clone();

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

    // Default (unstyled) text is PRIMARY — with TEXT_SECONDARY here a plain
    // `ui.label()` rendered at 3.9:1 inside any BG_SECONDARY card. Widgets
    // opt IN to de-emphasis via TEXT_SECONDARY/TEXT_MUTED, not out of it.
    visuals.widgets.noninteractive.bg_fill = BG_SECONDARY;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);

    visuals.widgets.inactive.bg_fill = BG_SECONDARY;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);

    visuals.widgets.hovered.bg_fill = BG_HIGHLIGHT;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);

    visuals.widgets.active.bg_fill = BG_HIGHLIGHT;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);

    visuals.selection.bg_fill = Color32::from_rgba_premultiplied(122, 162, 247, 40);
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);

    // Derived states: egui's defaults (weak 0.6, disabled 0.5) would drop
    // even TEXT_PRIMARY below AA. 0.7 of TEXT_PRIMARY still clears ~4.5:1
    // on cards while reading as de-emphasised.
    visuals.weak_text_alpha = 0.7;
    visuals.disabled_alpha = 0.7;

    style.visuals = visuals;

    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);

    ctx.set_global_style(style);
}
