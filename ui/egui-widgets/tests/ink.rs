//! `Token` / `Ink` — the "themed unless overridden" vocabulary.
//!
//! `Token::get` and `Token::name` are hand-written matches over `ColorTokens`,
//! which is the sort of thing that drifts silently: add a field, forget an arm,
//! and a widget quietly resolves the wrong colour forever. The match has no
//! wildcard so a *new* token fails to compile until it is named — but a
//! *mis-wired* arm (`AccentCyan => c.accent_blue`) compiles fine, and only a
//! test catches it. `every_token_resolves_to_its_own_field` is that test.

use egui::Color32;
use egui_widgets::theme::{ColorTokens, Ink, Series, Theme, Token, contrast_ratio, with_alpha};

/// Every shipped theme. `Theme::PRESETS` holds constructors, so adding a preset
/// enrols it here automatically.
fn presets() -> Vec<Theme> {
    Theme::PRESETS.iter().map(|p| p()).collect()
}

/// The pairing `Token::get` is supposed to implement, written out independently.
///
/// Deliberately a second copy rather than a loop over `Token::ALL` calling
/// `get`: comparing `get` against itself proves nothing. Adding a field to
/// `ColorTokens` breaks this destructuring, so the list cannot fall behind.
fn expected(c: &ColorTokens) -> Vec<(Token, Color32)> {
    let ColorTokens {
        bg_primary,
        bg_secondary,
        bg_highlight,
        text_primary,
        text_secondary,
        text_muted,
        accent_blue,
        accent_cyan,
        accent_green,
        accent_yellow,
        accent_orange,
        accent_red,
        accent_magenta,
        accent,
        success,
        warning,
        error,
        border,
    } = *c;
    vec![
        (Token::BgPrimary, bg_primary),
        (Token::BgSecondary, bg_secondary),
        (Token::BgHighlight, bg_highlight),
        (Token::TextPrimary, text_primary),
        (Token::TextSecondary, text_secondary),
        (Token::TextMuted, text_muted),
        (Token::AccentBlue, accent_blue),
        (Token::AccentCyan, accent_cyan),
        (Token::AccentGreen, accent_green),
        (Token::AccentYellow, accent_yellow),
        (Token::AccentOrange, accent_orange),
        (Token::AccentRed, accent_red),
        (Token::AccentMagenta, accent_magenta),
        (Token::Accent, accent),
        (Token::Success, success),
        (Token::Warning, warning),
        (Token::Error, error),
        (Token::Border, border),
    ]
}

#[test]
fn every_token_resolves_to_its_own_field() {
    for theme in &presets() {
        let pairs = expected(&theme.color);
        assert_eq!(
            pairs.len(),
            Token::ALL.len(),
            "{}: Token::ALL has {} entries, ColorTokens has {}",
            theme.name,
            Token::ALL.len(),
            pairs.len()
        );
        for (token, want) in pairs {
            assert_eq!(
                token.get(&theme.color),
                want,
                "{}: Token::{token:?} ({}) resolved to the wrong field",
                theme.name,
                token.name()
            );
        }
    }
}

#[test]
fn all_is_exhaustive_and_has_no_duplicates() {
    // `ALL` drives the token inspector and the contrast suite, so a missing or
    // repeated entry silently shrinks their coverage.
    let mut seen = std::collections::HashSet::new();
    for token in Token::ALL {
        assert!(seen.insert(token), "Token::{token:?} appears twice in ALL");
    }
    assert_eq!(seen.len(), Token::ALL.len());
}

#[test]
fn names_are_unique_and_match_the_field_spelling() {
    let mut seen = std::collections::HashSet::new();
    for token in Token::ALL {
        let name = token.name();
        assert!(
            seen.insert(name),
            "two tokens both call themselves {name:?}"
        );
        // `ColorTokens` fields are snake_case; a name that is not tells us the
        // string was typed rather than copied from the struct.
        assert!(
            name.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_'),
            "Token::{token:?}.name() = {name:?} is not a snake_case field name"
        );
    }
}

#[test]
fn token_arm_is_the_raw_value() {
    for theme in &presets() {
        for token in Token::ALL {
            assert_eq!(
                Ink::Token(token).resolve(theme),
                token.get(&theme.color),
                "{}: Ink::Token({token:?}) diverged from the token",
                theme.name
            );
        }
    }
}

#[test]
fn wash_goes_through_with_alpha_not_premultiplied() {
    // The bug this guards shipped once: `from_rgba_premultiplied` expects RGB
    // already scaled by alpha, so feeding it a palette colour blends additively
    // and renders far lighter than intended.
    for theme in &presets() {
        for token in Token::ALL {
            let base = token.get(&theme.color);
            assert_eq!(
                Ink::Wash(token, 0).resolve(theme),
                Color32::TRANSPARENT,
                "{}: a zero-alpha wash should be fully transparent",
                theme.name
            );
            for alpha in [1_u8, 26, 80, 160, 254, 255] {
                let got = Ink::Wash(token, alpha).resolve(theme);
                assert_eq!(
                    got,
                    with_alpha(base, alpha),
                    "{}: Wash({token:?}, {alpha}) is not the token at that alpha",
                    theme.name
                );
                assert_eq!(got.a(), alpha, "{}: Wash lost its alpha", theme.name);

                // And it is NOT what the wrong constructor produces. `Color32`
                // stores premultiplied bytes, so the distinction is not visible
                // in the channels — `from_rgba_premultiplied` simply keeps the
                // full-brightness channels it was handed, which then blend
                // additively and render far lighter than asked for. Comparing
                // the two constructors is what pins the right one down.
                let wrong = Color32::from_rgba_premultiplied(base.r(), base.g(), base.b(), alpha);
                // Only where the two CAN differ: at a high alpha the premultiply
                // rounds to a no-op and both constructors agree, which is
                // correct rather than a missed bug.
                let scaled = |v: u8| (f32::from(v) * f32::from(alpha) / 255.0).round() as u8;
                let distinguishable = scaled(base.r()) != base.r()
                    || scaled(base.g()) != base.g()
                    || scaled(base.b()) != base.b();
                if distinguishable {
                    assert_ne!(
                        got, wrong,
                        "{}: Wash({token:?}, {alpha}) matches the premultiplied \
                         constructor — the bug `with_alpha` exists to prevent",
                        theme.name
                    );
                    // The premultiplied reading is strictly the brighter one,
                    // which is exactly how the shipped bug presented.
                    assert!(
                        u32::from(wrong.r()) + u32::from(wrong.g()) + u32::from(wrong.b())
                            >= u32::from(got.r()) + u32::from(got.g()) + u32::from(got.b()),
                        "{}: expected the premultiplied reading to be brighter",
                        theme.name
                    );
                }
            }
        }
    }
}

#[test]
fn full_alpha_wash_is_the_plain_token() {
    for theme in &presets() {
        for token in Token::ALL {
            assert_eq!(
                Ink::Wash(token, 255).resolve(theme),
                Ink::Token(token).resolve(theme),
                "{}: an opaque wash should be the token itself",
                theme.name
            );
        }
    }
}

#[test]
fn on_picks_the_more_legible_end_of_the_ramp() {
    // `Ink::On(t)` exists so text over a semantic fill stays readable when the
    // theme changes the fill. The guarantee is comparative, not absolute: it
    // returns whichever end of the theme's own ramp contrasts more.
    for theme in &presets() {
        let c = &theme.color;
        for token in Token::ALL {
            let fill = token.get(c);
            let fg = Ink::On(token).resolve(theme);
            assert!(
                fg == c.bg_primary || fg == c.text_primary,
                "{}: On({token:?}) returned a colour from outside the ramp",
                theme.name
            );
            let other = if fg == c.bg_primary {
                c.text_primary
            } else {
                c.bg_primary
            };
            assert!(
                contrast_ratio(fg, fill) >= contrast_ratio(other, fill),
                "{}: On({token:?}) chose the less legible end",
                theme.name
            );
        }
    }
}

#[test]
fn fixed_is_returned_untouched_by_every_theme() {
    // The whole point of `Fixed` is that it escapes the theme. If a preset could
    // change it, the arm would be a lie and consumers relying on a brand colour
    // would drift.
    let brand = Color32::from_rgb(0x58, 0x65, 0xF2);
    for theme in &presets() {
        assert_eq!(Ink::Fixed(brand).resolve(theme), brand, "{}", theme.name);
    }
}

#[test]
fn series_arm_reads_the_encoding_palette_not_the_chrome_one() {
    for theme in &presets() {
        let s = &theme.series;
        assert_eq!(Ink::Series(Series::Inbound).resolve(theme), s.inbound());
        assert_eq!(Ink::Series(Series::Outbound).resolve(theme), s.outbound());
        for i in 0..8 {
            assert_eq!(Ink::Series(Series::Nth(i)).resolve(theme), s.nth(i));
        }
        for ring in 0..3 {
            assert_eq!(
                Ink::Series(Series::Class(ring)).resolve(theme),
                s.class(ring)
            );
        }
    }
}

#[test]
fn inbound_and_outbound_are_distinguishable() {
    // A dot meaning "arrived" and one meaning "left" are the two colours a
    // reader must never confuse, and three widgets now default to them via
    // `Ink::Series`.
    for theme in &presets() {
        let (i, o) = (theme.series.inbound(), theme.series.outbound());
        assert_ne!(
            i, o,
            "{}: inbound and outbound are the same colour",
            theme.name
        );
    }
}

#[test]
fn token_reports_the_chrome_token_and_nothing_else() {
    // `Ink::token()` is what lets a test assert a widget's defaults go through
    // the theme, so it must not claim a token for the two arms that have none.
    for token in Token::ALL {
        assert_eq!(Ink::Token(token).token(), Some(token));
        assert_eq!(Ink::Wash(token, 40).token(), Some(token));
        assert_eq!(Ink::On(token).token(), Some(token));
    }
    assert_eq!(Ink::Series(Series::Inbound).token(), None);
    assert_eq!(Ink::Fixed(Color32::RED).token(), None);
}

#[test]
fn is_fixed_flags_exactly_the_escaping_arm() {
    assert!(Ink::Fixed(Color32::RED).is_fixed());
    assert!(!Ink::Token(Token::Accent).is_fixed());
    assert!(!Ink::Wash(Token::Accent, 40).is_fixed());
    assert!(!Ink::On(Token::Accent).is_fixed());
    assert!(!Ink::Series(Series::Inbound).is_fixed());
}

#[test]
fn from_impls_route_to_the_arms_a_caller_means() {
    // Setters take `impl Into<Ink>` so `.color(Token::Error)` and
    // `.color(Color32::RED)` both compile. If these ever crossed over, every
    // call site would silently change meaning.
    assert_eq!(Ink::from(Token::Error), Ink::Token(Token::Error));
    assert_eq!(Ink::from(Color32::RED), Ink::Fixed(Color32::RED));
    assert_eq!(Ink::from(Series::Outbound), Ink::Series(Series::Outbound));
}

#[test]
fn token_sugar_matches_the_long_form() {
    for token in Token::ALL {
        assert_eq!(token.wash(40), Ink::Wash(token, 40));
        assert_eq!(token.on(), Ink::On(token));
    }
}

#[test]
fn no_two_presets_are_indistinguishable() {
    // The failure this crate kept hitting is a theme axis that is fully wired,
    // fully tested and entirely invisible. A preset has to differ from every
    // other on *something* a widget reads, or switching to it changes nothing.
    //
    // Deliberately not "must differ in colour": `tokyo night mono` shares the
    // whole chrome palette with `tokyo night` and differs only in the encoding
    // palette — same UI, monochrome charts — which is the point of having the
    // two palettes separate. Asserting per-token difference would have outlawed
    // exactly the design this crate is built around.
    let all = presets();
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            let axes = [
                ("color", a.color != b.color),
                ("text", a.text != b.text),
                ("density", a.density != b.density),
                ("spacing", a.spacing != b.spacing),
                ("geometry", a.geometry != b.geometry),
                ("motion", a.motion != b.motion),
                ("series", a.series != b.series),
            ];
            let differing: Vec<&str> = axes
                .iter()
                .filter(|(_, d)| *d)
                .map(|(name, _)| *name)
                .collect();
            assert!(
                !differing.is_empty(),
                "{} and {} are identical on every axis — switching between them \
                 can change nothing",
                a.name,
                b.name
            );
            assert_ne!(a, b, "{} and {} compare equal", a.name, b.name);
        }
    }
}

#[test]
fn at_least_one_preset_moves_the_chrome_palette() {
    // Weaker than per-pair, but it still catches the case that matters: every
    // widget default now resolves through `Token`, so if no preset moved the
    // chrome palette at all, none of that wiring would be observable.
    let all = presets();
    let base = &all[0];
    assert!(
        all[1..].iter().any(|t| Token::ALL
            .iter()
            .any(|k| k.get(&t.color) != k.get(&base.color))),
        "no preset differs from {} on any chrome token",
        base.name
    );
}
