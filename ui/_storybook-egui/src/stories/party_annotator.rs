//! `PartyAnnotator` story — the curation layer's one input surface.
//!
//! Three states worth comparing side by side:
//!
//! 1. **a fresh wallet** — nothing known, default basis `asserted`, so the very
//!    first thing the form does is ask what you are standing on;
//! 2. **an unsourced claim** — content with no source. The warning appears in
//!    place, at the moment of writing, rather than in an export nobody reads
//!    until later. It is not blocked: blocking would push the guess into the
//!    label field where nothing can flag it;
//! 3. **a derived note** — the CIP-27 royalty address, which the walk worked
//!    out for itself. No source obligation, and it must not render like a
//!    human's guess.

use crate::TEXT_MUTED;
use egui_widgets::{AnnotationDraft, PartyAnnotator, PartyBasis};

pub struct PartyAnnotatorState {
    fresh: AnnotationDraft,
    guess: AnnotationDraft,
    derived: AnnotationDraft,
    last: String,
    seeded: bool,
}

impl Default for PartyAnnotatorState {
    fn default() -> Self {
        Self {
            fresh: AnnotationDraft::default(),
            guess: AnnotationDraft::default(),
            derived: AnnotationDraft::default(),
            last: "nothing saved yet".into(),
            seeded: false,
        }
    }
}

fn palette() -> Vec<(String, usize)> {
    [
        ("team", 9),
        ("artist", 4),
        ("exchange", 3),
        ("off-ramp", 2),
        ("contractor", 2),
        ("treasury", 1),
    ]
    .into_iter()
    .map(|(t, n)| (t.to_string(), n))
    .collect()
}

fn entities() -> Vec<String> {
    ["Dwess", "MEKKALABS", "Pillar"]
        .into_iter()
        .map(String::from)
        .collect()
}

pub fn show(ui: &mut egui::Ui, state: &mut PartyAnnotatorState) {
    if !state.seeded {
        state.seeded = true;
        state.guess.entity = "probably the artist".into();
        state.guess.tags = vec!["artist".into()];
        state.derived.label = "royalty (CIP-27)".into();
        state.derived.basis = PartyBasis::Derived;
        state.derived.source = "CIP-27 777.addr in the policy's own on-mint metadata".into();
    }

    ui.label(
        egui::RichText::new(
            "Entity is WHO is behind the wallet (several wallets share one, which is what makes \
             roll-up possible); label is what to call this one wallet. The basis is the point: a \
             human filling this in is ASSERTING, so that is the default, and an assertion with \
             no source is marked in place rather than blocked.",
        )
        .small()
        .color(TEXT_MUTED),
    );
    ui.add_space(8.0);

    let pal = palette();
    let ents = entities();

    egui::Grid::new("annotator-grid")
        .num_columns(3)
        .spacing([18.0, 0.0])
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("1 · a fresh wallet")
                        .small()
                        .color(TEXT_MUTED),
                );
                // FULL addresses: `IdPill` middle-elides for display and puts
                // the whole thing on the clipboard, so a pre-truncated fixture
                // would be demonstrating a copy button that yields junk.
                let r = PartyAnnotator::new(
                    "fresh",
                    "stake1uy0x3e9q7m4kzvahn2s0dcf8rj6twpl5xg3ydq8vn4he2gswfl6ha",
                    &mut state.fresh,
                )
                    .palette(&pal)
                    .entities(&ents)
                    .show(ui);
                if r.save {
                    state.last = "saved: fresh wallet".into();
                }
            });
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("2 · a claim with no source")
                        .small()
                        .color(TEXT_MUTED),
                );
                let r = PartyAnnotator::new(
                    "guess",
                    "stake1u9hech4kd0p2ntyq7gv3xzm8ejlw5aqr6sf9cbdu3tk0nvgs7gpt5q",
                    &mut state.guess,
                )
                    .palette(&pal)
                    .entities(&ents)
                    .dirty(true)
                    .show(ui);
                if r.save {
                    state.last = format!("saved: unsourced = {}", r.unsourced);
                }
            });
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("3 · derived by the walk")
                        .small()
                        .color(TEXT_MUTED),
                );
                let r = PartyAnnotator::new(
                    "derived",
                    "stake1ux3q5nw8jm2pv6ted0hgy4za7bcrs9fxk5lqn3vu8dm4t0gsldk5f2",
                    &mut state.derived,
                )
                    .palette(&pal)
                    .entities(&ents)
                    .dirty(true)
                    .show(ui);
                if r.save {
                    state.last = "saved: derived note".into();
                }
            });
            ui.end_row();
        });

    ui.add_space(8.0);
    ui.label(egui::RichText::new(&state.last).small().color(TEXT_MUTED));
}
