//! Contrast floors for **every** preset this crate ships.
//!
//! The egui side has had this since its palette became themeable; this crate
//! never did, so its five presets were unchecked — and `muted` had drifted to a
//! value nobody had ever measured against a background that is *darker* than
//! egui's. That is the combination most likely to fail, and nothing could say
//! so.
//!
//! # Why it runs over `Theme::PRESETS`
//!
//! Checking one palette is a hole the moment a second exists: a new preset
//! could ship below AA and nothing would notice. **Adding a preset to
//! `Theme::PRESETS` is what enrols it.** A palette not in that list is not
//! covered.
//!
//! # The floors
//!
//! Text is drawn here at 11–16px, where WCAG AA wants 4.5:1. `muted` is a
//! de-emphasis tier, and de-emphasis is expressed *within* the passing range,
//! never by dropping below it. If a palette change fails here, the test is the
//! negotiation point, not your monitor.
//!
//! The maths is `ui_theme`'s, called rather than restated — the whole reason it
//! became public was that this arithmetic had been written four times across
//! three crates, in two precisions, and two copies had drifted apart.

use macroquad_widgets::Theme;
use ui_theme::{Paint, Srgb, Token, contrast_ratio};

/// WCAG AA for text below 18pt.
const AA_SMALL: f32 = 4.5;
/// A hairline nobody can see is not a hairline. The egui suite uses the same
/// floor for its border token.
const VISIBLE: f32 = 1.5;

fn presets() -> Vec<Theme> {
    Theme::PRESETS.iter().map(|p| p()).collect()
}

/// Every surface a colour can land on in this crate. `Painter` clears to
/// `bg_primary` and draws cards in `bg_secondary`; `bg_highlight` is the
/// inactive fill behind progress.
fn surfaces(t: &Theme) -> [(&'static str, Srgb); 3] {
    [
        ("bg_primary", t.color.bg_primary.srgb()),
        ("bg_secondary", t.color.bg_secondary.srgb()),
        ("bg_highlight", t.color.bg_highlight.srgb()),
    ]
}

#[test]
fn the_text_ramp_clears_aa_on_every_surface() {
    for t in presets() {
        for (label, tier) in [
            ("text_primary", t.color.text_primary),
            ("text_muted", t.color.text_muted),
        ] {
            for (surface_name, surface) in surfaces(&t) {
                let ratio = contrast_ratio(tier.srgb(), surface);
                assert!(
                    ratio >= AA_SMALL,
                    "{}: {label} on {surface_name} is {ratio:.2}, below {AA_SMALL}",
                    t.name
                );
            }
        }
    }
}

/// The status colours carry meaning on their own, so they have to be readable
/// against the surfaces they are drawn on — not merely distinguishable from
/// each other.
#[test]
fn every_status_colour_reads_on_every_surface() {
    for t in presets() {
        for (label, colour) in [
            ("accent", t.color.accent),
            ("accent_blue", t.color.accent_blue),
            ("error", t.color.error),
            ("warning", t.color.warning),
            ("success", t.color.success),
        ] {
            for (surface_name, surface) in surfaces(&t) {
                let ratio = contrast_ratio(colour.srgb(), surface);
                assert!(
                    ratio >= VISIBLE,
                    "{}: {label} on {surface_name} is {ratio:.2}, below {VISIBLE}",
                    t.name
                );
            }
        }
    }
}

/// `bg_highlight` is a fill, not text — but it has to separate from the page or
/// a progress bar has no visible extent.
#[test]
fn the_track_separates_from_the_background() {
    for t in presets() {
        let ratio = contrast_ratio(t.color.bg_highlight.srgb(), t.color.bg_primary.srgb());
        assert!(
            ratio >= 1.15,
            "{}: bg_highlight on bg_primary is {ratio:.2} — the bar would be \
             invisible",
            t.name
        );
    }
}

/// The reason `success` is teal rather than the accent: a preset swaps accent,
/// and a surface that already spends accent on something else (`block_train`
/// gives it to the newest block) would render "landed" in the same ink.
///
/// So they must stay tellable apart in EVERY preset, including `aqua`, whose
/// accent is closest to the teal.
#[test]
fn success_never_collides_with_the_accent_it_must_be_distinguishable_from() {
    for t in presets() {
        let d = ui_theme::delta_e(t.color.success.srgb(), t.color.accent.srgb());
        assert!(
            d >= 15.0,
            "{}: success vs accent is only ΔE {d:.1} — 'landed' would read as \
             'newest'",
            t.name
        );
    }
}

/// Presets differ in accent alone, so everything else must genuinely be shared.
/// If a preset ever forks the neutral base, this is what says so.
#[test]
fn presets_vary_the_accent_and_nothing_else() {
    let base = Theme::tokyo_night();
    for t in presets() {
        // Every token but `accent` must match the base. Driven off `Token::ALL`
        // rather than a hand-written list, so a token added to `ColorTokens` is
        // covered the day it exists — the same reasoning `PRESETS` uses above.
        for token in Token::ALL {
            if token == Token::Accent {
                continue;
            }
            assert_eq!(
                token.get(&t.color).srgb(),
                token.get(&base.color).srgb(),
                "{} moved {token:?} — presets vary the accent and nothing else",
                t.name
            );
        }
    }
}
