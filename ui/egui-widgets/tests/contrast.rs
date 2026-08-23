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
