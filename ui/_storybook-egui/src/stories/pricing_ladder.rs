//! `PricingLadder` story — structural rungs, with sales drawn against them.

use egui_widgets::pricing_ladder::{self, Fill, LadderRung, LaneStyle, PricingLadderConfig};

use crate::{accent, muted};

/// `now` for the lanes — 2026-09-18, the day the fills were pulled. Fixed
/// rather than the wall clock so the story keeps saying the same thing; a
/// drifting right edge would slowly turn a live ladder into a dead one and
/// nobody would notice the fixture had rotted.
const NOW: i64 = 1_789_683_535;

/// Every Black Flag fill, measured 2026-09-18: `rung,unix,ada,label`.
///
/// A data file rather than a literal because it is 213 rows of measurement,
/// and burying the story's structure under it would make the story unreadable
/// for no gain — it is not source, it is evidence.
const FILLS_CSV: &str = include_str!("black_flag_fills.csv");

/// The rungs as the server resolves them: supply, ratio, ladder price, and the
/// demand model's own recency-weighted median with the group size it rests on.
#[rustfmt::skip]
const RUNGS: [(&str, u64, f64, f64, Option<f64>, usize); 13] = [
    ("Swab",          389,  1.00,   49.0, Some(23.0),   55),
    ("Deckhand",      320,  1.22,   59.6, Some(39.0),   46),
    ("Lookout",       280,  1.39,   68.1, Some(18.0),   24),
    ("Cook",          220,  1.77,   86.6, Some(78.2),   15),
    ("Sailmaker",     200,  1.95,   95.3, Some(58.0),   27),
    ("Carpenter",     180,  2.16,  105.9, Some(78.0),   15),
    ("Gunner",        160,  2.43,  119.1, Some(50.0),   12),
    ("Boatswain",     100,  3.89,  190.6, Some(190.0),   7),
    ("Quartermaster",  60,  6.48,  317.7, Some(85.2),    4),
    ("Navigator",      40,  9.72,  476.5, None,          0),
    ("First Mate",     30, 12.97,  635.4, None,          0),
    ("Captain",        15, 25.93, 1270.7, Some(1960.0),  4),
    ("Legendary",       6, 64.83, 3176.8, None,          0),
];

/// The real Black Flag `Rank` ladder — **every figure came off mainnet**.
///
/// The fills are raw market-ledger fills joined through live trait bitmaps:
/// 213 fills across 2,000 assets, zero unmatched, reproducing the server's
/// support exactly on 11 of 13 rungs. The two that don't are the point of the
/// "model and lane disagree" section.
///
/// Tempo is the point of all of it, so a fixture with invented timing would
/// flatter the lane exactly the way the block-train fixture once flattered its
/// widget at 20× real block size.
///
/// Note the 2.5 ADA fills on Deckhand, Quartermaster and Captain: 36 of the 37
/// sub-5-ADA offer fills across ALL policies are that same figure, which is
/// not a market distribution — almost certainly a decode artifact picking up a
/// deposit rather than a payout. They are left in because they are what the
/// engine currently sees, and they are why the scatter's y-domain is clamped
/// rather than fitted.
fn black_flag() -> Vec<LadderRung> {
    RUNGS
        .into_iter()
        .map(
            |(value, supply, ratio, target, realized, support)| LadderRung {
                value: value.to_string(),
                supply,
                ratio,
                target: Some(target),
                realized,
                support,
                fills: FILLS_CSV
                    .lines()
                    .filter_map(|l| {
                        let mut f = l.splitn(4, ',');
                        let rung = f.next()?;
                        if rung != value {
                            return None;
                        }
                        Some(Fill {
                            at: f.next()?.parse().ok()?,
                            price: f.next()?.parse().ok()?,
                            label: f.next().unwrap_or_default().to_string(),
                        })
                    })
                    .collect(),
            },
        )
        .collect()
}

pub struct PricingLadderState {
    /// Accordion: one rung open at a time, and clicking the open one shuts it.
    /// The widget reports the click; this policy is the host's.
    open: Option<String>,
}

impl Default for PricingLadderState {
    /// Opens on Captain, because a story whose headline feature is shut is a
    /// story that does not show it. Captain is the right one: four fills, so
    /// the detail is short, and they run 2.5 → 250 → 600 → 1,960, which is both
    /// the appreciation the lane hints at and the 2.5 ADA artifact in one list.
    fn default() -> Self {
        Self {
            open: Some("Captain".to_string()),
        }
    }
}

fn config(id: &str, lane: LaneStyle, open: &Option<String>) -> PricingLadderConfig {
    PricingLadderConfig {
        category: "Rank".into(),
        sentinel: Some("Swab".into()),
        id_salt: id.into(),
        now: NOW,
        lane,
        expanded: open.clone(),
        anchor: Some(49.0),
        anchor_source: Some("listing floor".into()),
        ..Default::default()
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut PricingLadderState) {
    ui.label(
        egui::RichText::new("Pricing Ladder")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "A collection priced structurally: every rung a fixed multiple of the \
             anchor's price, derived from supply. Sales are drawn against the \
             ladder as evidence, never folded into it. Click a rung to see the \
             arithmetic and what actually traded. Real Black Flag data, \
             measured 2026-09-18.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    let resp = pricing_ladder::show(
        ui,
        &black_flag(),
        &PricingLadderConfig {
            highlight: Some("Carpenter".into()),
            ..config("story_scatter", LaneStyle::PriceScatter, &state.open)
        },
    );
    if let Some(v) = resp.clicked {
        state.open = if state.open.as_deref() == Some(v.as_str()) {
            None
        } else {
            Some(v)
        };
    }

    ui.add_space(18.0);
    ui.label(
        egui::RichText::new("One instrument, both questions")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Height is a multiple of the rung's OWN ladder price, so a 2× move \
             looks the same on a 49 ADA rung and a 3,176 ADA one, and the colour \
             flips where a fill cleared above it. Almost everything is below: on \
             this collection the ladder price is a ceiling nobody reaches, which \
             makes Sailmaker's two blue marks and Captain's newest fill worth \
             looking at. Read Quartermaster and there is nothing in the right \
             half at all.",
        )
        .color(muted(ui))
        .small(),
    );

    ui.add_space(18.0);
    ui.label(
        egui::RichText::new("Where the model and the lane disagree")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Navigator and First Mate each traded twice. The demand model gates \
             its median at a minimum group size, so it reports support 0 for \
             both — indistinguishable from never having traded. The count column \
             shows what was OBSERVED, so these say 2; the median column stays \
             empty because the model will not stand behind a number on n=2. \
             Only Legendary, with an empty lane, says \"no sales\".",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(8.0);

    let disputed: Vec<LadderRung> = black_flag()
        .into_iter()
        .filter(|r| matches!(r.value.as_str(), "Navigator" | "First Mate" | "Legendary"))
        .collect();
    let resp = pricing_ladder::show(
        ui,
        &disputed,
        &config("story_disputed", LaneStyle::PriceScatter, &state.open),
    );
    if let Some(v) = resp.clicked {
        state.open = if state.open.as_deref() == Some(v.as_str()) {
            None
        } else {
            Some(v)
        };
    }

    ui.add_space(18.0);
    ui.label(
        egui::RichText::new("Time only, for comparison")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "LaneStyle::Ticks — when, but not at what, so the bullet measure \
             earns its place back. Compact, and the only option for a ladder \
             whose rungs have no price to measure against.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(8.0);
    let resp = pricing_ladder::show(
        ui,
        &black_flag(),
        &config("story_ticks", LaneStyle::Ticks, &state.open),
    );
    if let Some(v) = resp.clicked {
        state.open = if state.open.as_deref() == Some(v.as_str()) {
            None
        } else {
            Some(v)
        };
    }
}
