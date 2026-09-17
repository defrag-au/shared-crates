//! `PricingLadder` story — structural rungs, with sales drawn against them.

use egui_widgets::pricing_ladder::{self, LadderRung, PricingLadderConfig};

use crate::{accent, muted};

/// The real Black Flag `Rank` ladder: 13 rungs off a 49 ADA Swab anchor. Kept
/// as measured rather than rounded, because the interesting rows are the ugly
/// ones — Quartermaster trading at a quarter of its ladder price on four
/// sales, and the three top rungs that have never traded at all.
fn black_flag() -> Vec<LadderRung> {
    let rows: [(&str, u64, f64, f64, Option<f64>, usize); 13] = [
        ("Swab", 389, 1.00, 49.0, Some(23.0), 55),
        ("Deckhand", 320, 1.22, 59.6, Some(39.0), 46),
        ("Lookout", 280, 1.39, 68.1, Some(18.0), 24),
        ("Cook", 220, 1.77, 86.6, Some(78.2), 15),
        ("Sailmaker", 200, 1.95, 95.3, Some(58.0), 27),
        ("Carpenter", 180, 2.16, 105.9, Some(78.0), 15),
        ("Gunner", 160, 2.43, 119.1, Some(50.0), 12),
        ("Boatswain", 100, 3.89, 190.6, Some(190.0), 7),
        ("Quartermaster", 60, 6.48, 317.7, Some(85.2), 4),
        ("Navigator", 40, 9.72, 476.5, None, 0),
        ("First Mate", 30, 12.97, 635.4, None, 0),
        ("Captain", 15, 25.93, 1270.7, Some(1960.0), 4),
        ("Legendary", 6, 64.83, 3176.8, None, 0),
    ];
    rows.into_iter()
        .map(|(value, supply, ratio, target, realized, support)| LadderRung {
            value: value.to_string(),
            supply,
            ratio,
            target: Some(target),
            realized,
            support,
        })
        .collect()
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Pricing Ladder")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "A collection priced structurally: every rung a fixed multiple of the \
             anchor's price, derived from supply. Sales are drawn against the \
             ladder as evidence, never folded into it.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    pricing_ladder::show(
        ui,
        &black_flag(),
        &PricingLadderConfig {
            category: "Rank".into(),
            sentinel: Some("Swab".into()),
            highlight: Some("Carpenter".into()),
            id_salt: "story_black_flag".into(),
            ..Default::default()
        },
    );

    ui.add_space(20.0);
    ui.label(
        egui::RichText::new("Before anything has traded")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "The case a sales chart cannot draw at all. The ladder exists the \
             moment the collection does, so the rungs price even with an empty \
             book — and with no anchor resolved yet, the price column says so \
             rather than showing a zero.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    let unanchored: Vec<LadderRung> = black_flag()
        .into_iter()
        .take(4)
        .map(|r| LadderRung {
            target: None,
            realized: None,
            support: 0,
            ..r
        })
        .collect();
    pricing_ladder::show(
        ui,
        &unanchored,
        &PricingLadderConfig {
            category: "Rank".into(),
            sentinel: Some("Swab".into()),
            id_salt: "story_unanchored".into(),
            ..Default::default()
        },
    );

    ui.add_space(20.0);
    ui.label(
        egui::RichText::new("Counts without medians")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "The shape a lean public payload has: how many sales back each rung, \
             but not what they went for. Only the rung that genuinely never \
             traded says \"no sales\" — the rest leave the measure blank and let \
             the count speak, rather than claiming nothing sold beside an n of 55.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    let counts_only: Vec<LadderRung> = black_flag()
        .into_iter()
        .take(4)
        .chain(black_flag().into_iter().filter(|r| r.support == 0).take(1))
        .map(|r| LadderRung {
            realized: None,
            ..r
        })
        .collect();
    pricing_ladder::show(
        ui,
        &counts_only,
        &PricingLadderConfig {
            category: "Rank".into(),
            sentinel: Some("Swab".into()),
            id_salt: "story_counts_only".into(),
            ..Default::default()
        },
    );
}
