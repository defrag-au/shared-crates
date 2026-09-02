//! Candidate visualisations for "the history of a token", on REAL WRT data.
//!
//! An exploration surface, not a widget. The point is to look at forms side by
//! side before committing one to `egui-widgets`, using data with the awkward
//! shapes a real token has rather than a synthetic curve that flatters
//! whatever it is plotted with.
//!
//! What the WRT fixture actually contains, which decided what is drawn here:
//!
//! | | 2022 peak | 2026 | |
//! |---|---|---|---|
//! | pooled depth | 3,139,987 ADA | 393,911 | 12.5% of peak |
//! | spot | 0.689 ADA | 0.0217 | −97% |
//! | holders | 16,327 | 11,796 | peaked, then bled |
//! | composition | script 54 / wallet 46 | script 56 / wallet 44 | ~static |
//!
//! So the composition wave — the form the research pass expected to lead with,
//! because HODL waves are the familiar shape for this — is **nearly
//! featureless on this token**. It is drawn anyway, precisely so that can be
//! seen rather than argued about.

use egui::{pos2, Color32, Pos2, Rect, Vec2};

use crate::stories::wrt_fixture::{COHORTS, SERIES};
use crate::{ACCENT, TEXT_MUTED};

/// The validated ordinal liquidity ramp — one hue, immovable → liquid.
///
/// Ordinal rather than categorical because the cohorts are a ladder: swapping
/// two changes the meaning. Ran through the dataviz validator at `--ordinal`
/// on a dark surface; a first attempt failed the light-end contrast check at
/// 1.98:1, which is why the dark end sits where it does.
const LADDER: [Color32; 4] = [
    Color32::from_rgb(0x3c, 0x5c, 0x94), // burn — gone
    Color32::from_rgb(0x4a, 0x72, 0xb5), // pool
    Color32::from_rgb(0x5b, 0x8a, 0xd6), // script
    Color32::from_rgb(0xa3, 0xc6, 0xfb), // wallet — free
];

/// The unmeasured band sits OUTSIDE the ramp. "We have not identified this" is
/// a finding, not a rung, and on WRT it is 56% of supply.
const UNMEASURED: Color32 = Color32::from_rgb(0xe0, 0xaf, 0x68);

fn plot_frame(ui: &mut egui::Ui, height: f32) -> Rect {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        egui::Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, 2.0, Color32::from_rgb(0x14, 0x15, 0x1e));
    rect
}

fn caption(ui: &mut egui::Ui, title: &str, note: &str) {
    ui.add_space(10.0);
    ui.label(egui::RichText::new(title).color(ACCENT).strong());
    ui.label(egui::RichText::new(note).color(TEXT_MUTED).small());
    ui.add_space(2.0);
}

/// One series as a line. `log` decides whether the y-scale is logarithmic —
/// the whole question for a series spanning 30×.
fn line(ui: &mut egui::Ui, rect: Rect, vals: &[f64], colour: Color32, log: bool) {
    // ⚠️ `ln(v + 1.0)` is NOT a log axis for data below 1. Spot ranges 0.02–0.69
    // ADA, and ln(1.02)..ln(1.69) is very nearly linear — the first version of
    // this drew a "log" price chart pixel-identical to the linear one.
    //
    // A real log axis floors at the smallest POSITIVE sample instead. That also
    // fixes the second trap: a leading zero (a token before its first pool)
    // would otherwise stretch the axis from -inf and squash the entire visible
    // range into the top few percent, which is what made depth look flat.
    let floor = vals
        .iter()
        .copied()
        .filter(|v| *v > 0.0)
        .fold(f64::MAX, f64::min);
    let pos = |v: f64| -> f64 {
        if log {
            v.max(floor).ln()
        } else {
            v
        }
    };
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for v in vals {
        lo = lo.min(pos(*v));
        hi = hi.max(pos(*v));
    }
    if !(hi > lo) {
        return;
    }
    let pts: Vec<Pos2> = vals
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = rect.left() + rect.width() * i as f32 / (vals.len() - 1) as f32;
            let y = rect.bottom() - ((pos(*v) - lo) / (hi - lo)) as f32 * rect.height();
            pos2(x, y)
        })
        .collect();
    // 2px, per the mark spec. Thin marks; the fill is not the point here.
    ui.painter().add(egui::Shape::line(
        pts,
        egui_widgets::theme::stroke(2.0, colour),
    ));
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Token history — candidate forms, real WRT data")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "240 points, 2022-03 → 2026-08. Exploration surface: pick a form, then build it.",
        )
        .color(TEXT_MUTED)
        .small(),
    );

    let depth: Vec<f64> = SERIES.iter().map(|r| r.3 as f64 / 1e6).collect();
    let spot: Vec<f64> = SERIES.iter().map(|r| r.4).collect();
    let holders: Vec<f64> = SERIES.iter().map(|r| r.2 as f64).collect();

    // ---- A. price, linear vs log --------------------------------------
    caption(
        ui,
        "A · spot price — LINEAR",
        "0.689 → 0.0217 ADA. The 2022 peak eats the axis; four years of the token's life read as zero.",
    );
    let r = plot_frame(ui, 90.0);
    line(ui, r, &spot, LADDER[3], false);

    caption(
        ui,
        "A′ · spot price — LOG",
        "same series, log y. The decay becomes a slope you can actually read — the conventional treatment for a 30× range.",
    );
    let r = plot_frame(ui, 90.0);
    line(ui, r, &spot, LADDER[3], true);

    // ---- B. depth ------------------------------------------------------
    caption(
        ui,
        "B · pooled ADA depth — LOG",
        "3,139,987 ADA peak (2022-08) → 393,911 now, 12.5%. Arguably THE series: it says whether a real market ever existed.",
    );
    let r = plot_frame(ui, 90.0);
    line(ui, r, &depth, Color32::from_rgb(0x7d, 0xcf, 0xff), true);

    // ---- C. holders ----------------------------------------------------
    caption(
        ui,
        "C · holders",
        "16,327 peak → 11,796. Peaked and bled — attrition is a signal a supply chart cannot show.",
    );
    let r = plot_frame(ui, 70.0);
    line(ui, r, &holders, Color32::from_rgb(0x9e, 0xce, 0x6a), false);

    // ---- D. composition wave -------------------------------------------
    caption(
        ui,
        "D · supply composition — 100% stacked (the HODL-wave form)",
        "The form the research expected to lead with. On WRT it is two flat bands: composition barely moved in four years.",
    );
    let rect = plot_frame(ui, 110.0);
    let supply: f64 = 100_000_000_000_000.0;
    let n = SERIES.len();
    let x_of = |i: usize| rect.left() + rect.width() * i as f32 / (n - 1) as f32;

    // ⚠️ ONE polygon per band, not one quad per segment. The first version drew
    // 239 separate quads per cohort and the anti-aliased seam between each pair
    // showed as vertical striping across the whole chart — the fill looked
    // hatched. Accumulate the running total, then walk the top edge forward and
    // the bottom edge back so each band is a single closed path.
    let mut lower = vec![0.0f32; n];
    for (c, name) in COHORTS.iter().enumerate() {
        let colour = if *name == "script" {
            UNMEASURED
        } else {
            LADDER[c]
        };
        let upper: Vec<f32> = (0..n)
            .map(|i| lower[i] + (SERIES[i].1[c] as f64 / supply) as f32)
            .collect();
        let mut path: Vec<Pos2> = (0..n)
            .map(|i| pos2(x_of(i), rect.bottom() - upper[i] * rect.height()))
            .collect();
        path.extend(
            (0..n)
                .rev()
                .map(|i| pos2(x_of(i), rect.bottom() - lower[i] * rect.height())),
        );
        // Not convex — a stacked band's outline generally is not — so this
        // needs the concave path fill, which `convex_polygon` would tessellate
        // wrongly.
        ui.painter().add(egui::Shape::Path(egui::epaint::PathShape {
            points: path,
            closed: true,
            fill: colour,
            stroke: egui::epaint::PathStroke::NONE,
        }));
        lower = upper;
    }

    ui.add_space(12.0);
    ui.separator();
    ui.label(
        egui::RichText::new("What to decide:")
            .color(ACCENT)
            .strong(),
    );
    ui.label("\u{2022} A vs A′ — does log earn its place? (a 30× range says yes)");
    ui.label("\u{2022} Is depth (B) the headline rather than price?");
    ui.label("\u{2022} Does the composition wave (D) earn its space on a token like this?");
    ui.label("\u{2022} Ordinal ramp reads as a ladder; the unmeasured band sits outside it");
}
