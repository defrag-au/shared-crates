//! Contrast floors for **every** theme the crate ships.
//!
//! Most of this suite renders text at 9–12px, where WCAG AA requires 4.5:1.
//! Every tier of the text ramp must clear that on every background it can
//! land on — de-emphasis is expressed within the passing range, not by
//! dropping below it. If a palette change fails here, the test is the
//! negotiation point, not your monitor.
//!
//! ## Why this runs over `Theme::PRESETS`
//!
//! It used to read the module constants, which meant it checked exactly one
//! palette. The moment a second preset existed, that was a hole: a new theme
//! could ship below AA and nothing would say so. The floors and the checks here
//! are unchanged — they are simply applied to every theme now, with the theme's
//! name in the failure message.
//!
//! **Adding a preset to `Theme::PRESETS` is what enrols it.** A palette not in
//! that list is not covered.

use egui::Color32;
use egui_widgets::theme::{self, Theme};

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

/// Every theme under test, by name.
fn presets() -> Vec<Theme> {
    Theme::PRESETS.iter().map(|p| p()).collect()
}

/// Every background a text colour can land on, for one theme.
fn backgrounds(t: &Theme) -> [(&'static str, Color32); 3] {
    [
        ("bg_primary", t.color.bg_primary),
        ("bg_secondary", t.color.bg_secondary),
        ("bg_highlight", t.color.bg_highlight),
    ]
}

/// The style as an app actually gets it — selection colours are derived in
/// `apply_style`, not stored in the tokens, so testing the tokens alone missed
/// them entirely.
fn configured(t: &Theme) -> egui::Visuals {
    let ctx = egui::Context::default();
    theme::install_theme(&ctx, t.clone());
    // `global_style`, not `ui.style()` — this is the context-wide style
    // `install_theme` writes.
    ctx.global_style().visuals.clone()
}

#[test]
fn translucent_theme_colours_are_valid_premultiplied() {
    // `Color32::from_rgba_premultiplied` requires every channel <= alpha.
    // Violating it doesn't error — it blends additively, so a 16% tint renders
    // as a bright wash. That is how accent-on-accent shipped on the selected
    // tab, and this catches the whole class rather than the one instance.
    for t in presets() {
        let v = configured(&t);
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
                "[{}] {name} = rgba({}, {}, {}, {a}) is not valid premultiplied — \
                 a channel exceeds alpha, so it will blend additively and render \
                 lighter than intended",
                t.name,
                c.r(),
                c.g(),
                c.b(),
            );
        }
    }
}

#[test]
fn selection_text_clears_wcag_aa_on_every_background() {
    // egui's `interact_selectable` assigns `fg_stroke` from
    // `selection.stroke`, so that colour is the SELECTED LABEL'S TEXT sitting
    // on `selection.bg_fill`. A selected tab is the most-clicked thing on a
    // surface and was the least readable.
    for t in presets() {
        let v = configured(&t);
        for (bg_name, bg) in backgrounds(&t) {
            let wash = over(v.selection.bg_fill, bg);
            let ratio = contrast(v.selection.stroke.color, wash);
            assert!(
                ratio >= 4.5,
                "[{}] selected text on the selection wash over {bg_name} is \
                 {ratio:.2}:1 — below WCAG AA (4.5:1)",
                t.name
            );
        }
    }
}

#[test]
fn text_ramp_clears_wcag_aa_on_every_background() {
    for t in presets() {
        let ramp = [
            ("text_primary", t.color.text_primary),
            ("text_secondary", t.color.text_secondary),
            ("text_muted", t.color.text_muted),
        ];
        for (bg_name, bg) in backgrounds(&t) {
            for (fg_name, fg) in ramp {
                let ratio = contrast(fg, bg);
                assert!(
                    ratio >= 4.5,
                    "[{}] {fg_name} on {bg_name} is {ratio:.2}:1 — below WCAG AA (4.5:1)",
                    t.name
                );
            }
        }
    }
}

#[test]
fn text_hierarchy_is_ordered() {
    for t in presets() {
        assert!(
            luminance(t.color.text_primary) > luminance(t.color.text_secondary),
            "[{}] text_primary must be brighter than text_secondary",
            t.name
        );
        assert!(
            luminance(t.color.text_secondary) > luminance(t.color.text_muted),
            "[{}] text_secondary must be brighter than text_muted",
            t.name
        );
    }
}

#[test]
fn accents_clear_wcag_aa_on_cards() {
    for t in presets() {
        let accents = [
            ("accent_blue", t.color.accent_blue),
            ("accent_cyan", t.color.accent_cyan),
            ("accent_green", t.color.accent_green),
            ("accent_yellow", t.color.accent_yellow),
            ("accent_orange", t.color.accent_orange),
            ("accent_red", t.color.accent_red),
            ("accent_magenta", t.color.accent_magenta),
            // The semantic tokens are separately settable, so they need checking
            // in their own right — a theme may point `success` somewhere the ramp
            // does not go.
            ("accent", t.color.accent),
            ("success", t.color.success),
            ("warning", t.color.warning),
            ("error", t.color.error),
        ];
        for (name, accent) in accents {
            let ratio = contrast(accent, t.color.bg_secondary);
            assert!(
                ratio >= 4.5,
                "[{}] {name} on bg_secondary is {ratio:.2}:1 — below WCAG AA (4.5:1)",
                t.name
            );
        }
    }
}

#[test]
fn border_is_visible() {
    // Borders aren't text: they don't need 4.5:1, they need to exist.
    // border == bg_highlight (1.24:1) was the old failure mode.
    for t in presets() {
        let ratio = contrast(t.color.border, t.color.bg_primary);
        assert!(
            ratio >= 1.5,
            "[{}] border on bg_primary is {ratio:.2}:1 — panel edges are invisible",
            t.name
        );
    }
}

#[test]
fn every_chip_variant_is_readable_in_every_theme() {
    // Was `default_chip_variant_is_readable`, and only checked `Muted`, because
    // `ChipVariant` carried six literal pairs and there was no theme to vary.
    // Now that the palette is derived from tokens, every variant is a claim
    // about every preset — so the same 4.5:1 floor runs across the matrix.
    //
    // This is the test that would have caught the real hazard in deriving them:
    // a theme with a pale `warning` and a white-ish `on()` pick.
    for t in presets() {
        for variant in egui_widgets::chip::ChipVariant::ALL {
            let (fg, bg, _) = variant.palette(&t);
            let ratio = contrast(fg, bg);
            assert!(
                ratio >= 4.5,
                "`{}` / `{variant:?}` is {ratio:.2}:1 — below WCAG AA",
                t.name
            );
        }
    }
}
