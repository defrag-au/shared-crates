//! Storybook demo for the TypeaheadSearch widget.
//!
//! Demonstrates the client-side technique: a precomputed flat option list is
//! filtered per keystroke via `filter_options`, then rendered with
//! keyboard-nav (up/down/enter) + click selection. (In production the
//! holder-map landing instead feeds server-ranked results straight in.)

use egui_widgets::{filter_options, ChipVariant, TypeaheadOption, TypeaheadSearch};

use crate::{ACCENT, TEXT_MUTED};

/// Story state persisted across frames via egui temp memory.
#[derive(Clone, Default)]
struct StoryState {
    query: String,
    highlight: usize,
    chosen: Option<String>,
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("TypeaheadSearch Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Search box with a keyboard-navigable result dropdown. Caller owns \
             the query + highlight; options are ranked server-side or via \
             `filter_options`. Up/Down to move, Enter or click to choose.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    let options = sample_options();
    let state_id = ui.id().with("typeahead_story");
    let mut st = ui
        .data_mut(|d| d.get_temp::<StoryState>(state_id))
        .unwrap_or_default();

    // Local filter over the precomputed flat list, then own the subset so it
    // can be handed to the widget as a slice.
    let shown: Vec<TypeaheadOption> = filter_options(&options, &st.query, 25)
        .into_iter()
        .cloned()
        .collect();

    egui::Frame::new()
        .fill(crate::BG_MAIN)
        .corner_radius(8.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(460.0));
            let resp =
                TypeaheadSearch::new("typeahead_story", &mut st.query, &shown, &mut st.highlight)
                    .placeholder("Search tokens by name or ticker…")
                    .empty_text("No tokens match that search")
                    .show(ui);

            if let Some(id) = resp.chosen {
                st.chosen = Some(id);
            }

            if let Some(chosen) = &st.chosen {
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(format!("Selected: {chosen}"))
                        .color(ACCENT)
                        .size(12.0),
                );
            }
        });

    ui.data_mut(|d| d.insert_temp(state_id, st));
}

fn sample_options() -> Vec<TypeaheadOption> {
    let seed: &[(&str, &str, &str, Option<(&str, ChipVariant)>)] = &[
        (
            "snek",
            "Snek",
            "$SNEK · 279c909f…",
            Some(("VERIFIED", ChipVariant::Success)),
        ),
        (
            "hosky",
            "Hosky",
            "$HOSKY · a0028f35…",
            Some(("VERIFIED", ChipVariant::Success)),
        ),
        (
            "min",
            "Minswap",
            "$MIN · 29d2227…",
            Some(("VERIFIED", ChipVariant::Success)),
        ),
        ("aliens", "Aliens", "$ALIENS · 16657df3…", None),
        ("angels", "Angels", "$ANGELS · b6a7467e…", None),
        ("supersnek", "Super Snek", "fair launch · 4f3a91c2…", None),
        (
            "copycat",
            "Snekk",
            "copy mint · 9912ab44…",
            Some(("RUG", ChipVariant::Danger)),
        ),
        (
            "wmt",
            "World Mobile Token",
            "$WMT · 1d7f33bd…",
            Some(("VERIFIED", ChipVariant::Success)),
        ),
        (
            "iag",
            "IAGON",
            "$IAG · 5d16cc1a…",
            Some(("VERIFIED", ChipVariant::Success)),
        ),
        ("book", "Book.io", "$BOOK · 0a4f6f9e…", None),
    ];

    seed.iter()
        .map(|(id, title, subtitle, badge)| {
            let mut opt = TypeaheadOption::new(*id, *title).subtitle(*subtitle);
            if let Some((label, variant)) = badge {
                opt = opt.badge(*label, *variant);
            }
            opt
        })
        .collect()
}
