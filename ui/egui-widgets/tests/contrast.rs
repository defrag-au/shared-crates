//! Contrast floors for the theme palette.
//!
//! Most of this suite renders text at 9–12px, where WCAG AA requires 4.5:1.
//! Every tier of the text ramp must clear that on every background it can
//! land on — de-emphasis is expressed within the passing range, not by
//! dropping below it. If a palette change fails here, the test is the
//! negotiation point, not your monitor.

use egui::Color32;
use egui_widgets::theme;

/// sRGB channel linearization (WCAG 2.0).
fn linearize(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG 2.0 relative luminance.
fn luminance(c: Color32) -> f64 {
    0.2126 * linearize(c.r()) + 0.7152 * linearize(c.g()) + 0.0722 * linearize(c.b())
}

/// WCAG contrast ratio between two colors.
fn contrast(a: Color32, b: Color32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

const BACKGROUNDS: [(&str, Color32); 3] = [
    ("BG_PRIMARY", theme::BG_PRIMARY),
    ("BG_SECONDARY", theme::BG_SECONDARY),
    ("BG_HIGHLIGHT", theme::BG_HIGHLIGHT),
];

/// Composite a translucent colour over an opaque one. `Color32` stores
/// PREMULTIPLIED channels, so the source contributes its channels directly and
/// the destination is attenuated by the remaining alpha.
fn over(fg: Color32, bg: Color32) -> Color32 {
    let inv = (255 - fg.a()) as u32;
    let blend = |f: u8, b: u8| ((f as u32) + (b as u32) * inv / 255).min(255) as u8;
    Color32::from_rgb(
        blend(fg.r(), bg.r()),
        blend(fg.g(), bg.g()),
        blend(fg.b(), bg.b()),
    )
}

/// The style as an app actually gets it — selection colours live in
/// `configure_style`, not in the palette constants, so testing the constants
/// alone missed them entirely.
fn configured() -> egui::Visuals {
    let ctx = egui::Context::default();
    theme::configure_style(&ctx, theme::FontStrategy::proportional());
    ctx.style().visuals.clone()
}

#[test]
fn translucent_theme_colours_are_valid_premultiplied() {
    // `Color32::from_rgba_premultiplied` requires every channel <= alpha.
    // Violating it doesn't error — it blends additively, so a 16% tint renders
    // as a bright wash. That is how accent-on-accent shipped on the selected
    // tab, and this catches the whole class rather than the one instance.
    let v = configured();
    for (name, c) in [
        ("selection.bg_fill", v.selection.bg_fill),
        ("panel_fill", v.panel_fill),
        ("window_fill", v.window_fill),
        ("faint_bg_color", v.faint_bg_color),
        ("extreme_bg_color", v.extreme_bg_color),
    ] {
        let a = c.a();
        assert!(
            c.r() <= a && c.g() <= a && c.b() <= a,
            "{name} = rgba({}, {}, {}, {a}) is not valid premultiplied — a \
             channel exceeds alpha, so it will blend additively and render \
             lighter than intended",
            c.r(),
            c.g(),
            c.b(),
        );
    }
}

#[test]
fn selection_text_clears_wcag_aa_on_every_background() {
    // egui's `interact_selectable` assigns `fg_stroke` from
    // `selection.stroke`, so that colour is the SELECTED LABEL'S TEXT sitting
    // on `selection.bg_fill`. A selected tab is the most-clicked thing on a
    // surface and was the least readable.
    let v = configured();
    for (bg_name, bg) in BACKGROUNDS {
        let wash = over(v.selection.bg_fill, bg);
        let ratio = contrast(v.selection.stroke.color, wash);
        assert!(
            ratio >= 4.5,
            "selected text on the selection wash over {bg_name} is \
             {ratio:.2}:1 — below WCAG AA (4.5:1)"
        );
    }
}

#[test]
fn text_ramp_clears_wcag_aa_on_every_background() {
    let ramp = [
        ("TEXT_PRIMARY", theme::TEXT_PRIMARY),
        ("TEXT_SECONDARY", theme::TEXT_SECONDARY),
        ("TEXT_MUTED", theme::TEXT_MUTED),
    ];
    for (bg_name, bg) in BACKGROUNDS {
        for (fg_name, fg) in ramp {
            let ratio = contrast(fg, bg);
            assert!(
                ratio >= 4.5,
                "{fg_name} on {bg_name} is {ratio:.2}:1 — below WCAG AA (4.5:1)"
            );
        }
    }
}

#[test]
fn text_hierarchy_is_ordered() {
    assert!(
        luminance(theme::TEXT_PRIMARY) > luminance(theme::TEXT_SECONDARY),
        "TEXT_PRIMARY must be brighter than TEXT_SECONDARY"
    );
    assert!(
        luminance(theme::TEXT_SECONDARY) > luminance(theme::TEXT_MUTED),
        "TEXT_SECONDARY must be brighter than TEXT_MUTED"
    );
}

#[test]
fn accents_clear_wcag_aa_on_cards() {
    let accents = [
        ("ACCENT_BLUE", theme::ACCENT_BLUE),
        ("ACCENT_CYAN", theme::ACCENT_CYAN),
        ("ACCENT_GREEN", theme::ACCENT_GREEN),
        ("ACCENT_YELLOW", theme::ACCENT_YELLOW),
        ("ACCENT_ORANGE", theme::ACCENT_ORANGE),
        ("ACCENT_RED", theme::ACCENT_RED),
        ("ACCENT_MAGENTA", theme::ACCENT_MAGENTA),
    ];
    for (name, accent) in accents {
        let ratio = contrast(accent, theme::BG_SECONDARY);
        assert!(
            ratio >= 4.5,
            "{name} on BG_SECONDARY is {ratio:.2}:1 — below WCAG AA (4.5:1)"
        );
    }
}

#[test]
fn border_is_visible() {
    // Borders aren't text: they don't need 4.5:1, they need to exist.
    // BORDER == BG_HIGHLIGHT (1.24:1) was the old failure mode.
    let ratio = contrast(theme::BORDER, theme::BG_PRIMARY);
    assert!(
        ratio >= 1.5,
        "BORDER on BG_PRIMARY is {ratio:.2}:1 — panel edges are invisible"
    );
}

#[test]
fn default_chip_variant_is_readable() {
    let (fg, bg, _) = egui_widgets::chip::ChipVariant::Muted.palette();
    let ratio = contrast(fg, bg);
    assert!(
        ratio >= 4.5,
        "ChipVariant::Muted (the default) is {ratio:.2}:1 — below WCAG AA"
    );
}
