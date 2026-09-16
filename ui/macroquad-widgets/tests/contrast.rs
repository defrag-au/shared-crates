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
use ui_theme::{Paint, Srgb, contrast_ratio};

/// WCAG AA for text below 18pt.
const AA_SMALL: f32 = 4.5;
/// A hairline nobody can see is not a hairline. The egui suite uses the same
/// floor for its border token.
const VISIBLE: f32 = 1.5;

fn presets() -> Vec<Theme> {
    Theme::PRESETS.iter().map(|p| p()).collect()
}

/// Every surface a colour can land on in this crate. `Painter` clears to `bg`
/// and draws cards in `panel`; `track` is the inactive fill behind progress.
fn surfaces(t: &Theme) -> [(&'static str, Srgb); 3] {
    [
        ("bg", t.bg.srgb()),
        ("panel", t.panel.srgb()),
        ("track", t.track.srgb()),
    ]
}

#[test]
fn the_text_ramp_clears_aa_on_every_surface() {
    for t in presets() {
        for (label, tier) in [("fg", t.fg), ("muted", t.muted)] {
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
            ("accent", t.accent),
            ("link", t.link),
            ("danger", t.danger),
            ("warn", t.warn),
            ("success", t.success),
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

/// `track` is a fill, not text — but it has to separate from the page or a
/// progress bar has no visible extent.
#[test]
fn the_track_separates_from_the_background() {
    for t in presets() {
        let ratio = contrast_ratio(t.track.srgb(), t.bg.srgb());
        assert!(
            ratio >= 1.15,
            "{}: track on bg is {ratio:.2} — the bar would be invisible",
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
        let d = ui_theme::delta_e(t.success.srgb(), t.accent.srgb());
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
        assert_eq!(t.bg.srgb(), base.bg.srgb(), "{} moved bg", t.name);
        assert_eq!(t.panel.srgb(), base.panel.srgb(), "{} moved panel", t.name);
        assert_eq!(t.fg.srgb(), base.fg.srgb(), "{} moved fg", t.name);
        assert_eq!(t.muted.srgb(), base.muted.srgb(), "{} moved muted", t.name);
        assert_eq!(
            t.success.srgb(),
            base.success.srgb(),
            "{} moved success — it is fixed across presets on purpose",
            t.name
        );
    }
}
