//! Kinetic variants for a token's life — prototypes, not widgets.
//!
//! The line-chart pass was judged (correctly) as playing it far too safe: it
//! averaged a per-transaction flow system into smooth curves and drew them with
//! no axes, no events and no emphasis. This explores the opposite end.
//!
//! ## What the data actually is
//!
//! The ledger holds signed per-(tx, party) deltas with a **conservation
//! invariant** — every transaction's deltas sum to the net mint. So supply is
//! **conserved mass migrating between reservoirs**: wallets, pools, vesting
//! scripts, unnamed scripts, and burn (the only true sink). Nothing is created
//! except at a mint. That is a physical system, and it should look like one.
//!
//! $Aliens is the subject because it has a story, where WRT is four years of
//! decay: 94% starts in a launchpad script, LP accumulates 0 → 60%, vesting
//! rises to 7.18% and then partly claims down to 5.98% — and **stays there,
//! past maturity**. A gate opened and most of it did not walk through.

use egui::{pos2, Color32, Pos2, Rect, Vec2};

use crate::stories::aliens_fixture::{COHORTS, SERIES};
use crate::{ACCENT, TEXT_MUTED};

const SUPPLY: f64 = 1_000_000_000.0;

/// Validated ordinal liquidity ramp, immovable → liquid.
fn cohort_color(name: &str) -> Color32 {
    match name {
        "burn" => Color32::from_rgb(0x3c, 0x5c, 0x94),
        "vesting" => Color32::from_rgb(0x4a, 0x72, 0xb5),
        "pool" => Color32::from_rgb(0x5b, 0x8a, 0xd6),
        "wallet" => Color32::from_rgb(0xa3, 0xc6, 0xfb),
        // Off-ramp: an absence of knowledge is not a rung on the ladder.
        _ => Color32::from_rgb(0xe0, 0xaf, 0x68),
    }
}

/// A moment where a cohort moved enough to be worth naming.
///
/// Derived by diffing consecutive samples rather than stored: the point is to
/// prove events are legible in what we already hold. A real widget would take
/// them from the movement columns, where the magnitude is exact.
struct Event {
    i: usize,
    /// Which cohort moved. Kept because it is what makes an event
    /// attributable — the marks are drawn cohort-agnostically today, but an
    /// event you cannot trace back to a cohort is just a spike.
    #[allow(dead_code)]
    cohort: usize,
    delta: f64,
}

fn events(threshold: f64) -> Vec<Event> {
    let mut out = Vec::new();
    for i in 1..SERIES.len() {
        for c in 0..COHORTS.len() {
            let d = (SERIES[i].1[c] - SERIES[i - 1].1[c]) as f64 / SUPPLY;
            if d.abs() >= threshold {
                out.push(Event {
                    i,
                    cohort: c,
                    delta: d,
                });
            }
        }
    }
    out
}

fn frame(ui: &mut egui::Ui, h: f32) -> Rect {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), egui::Sense::hover());
    ui.painter()
        .rect_filled(r, 2.0, Color32::from_rgb(0x11, 0x12, 0x1a));
    r
}

fn caption(ui: &mut egui::Ui, title: &str, note: &str) {
    ui.add_space(12.0);
    ui.label(egui::RichText::new(title).color(ACCENT).strong());
    ui.label(egui::RichText::new(note).color(TEXT_MUTED).small());
    ui.add_space(2.0);
}

/// Stacked bands as ONE polygon each — never per-segment quads, which show
/// their anti-aliased seams as vertical striping.
fn bands(ui: &mut egui::Ui, rect: Rect, xs: &[f32]) {
    let n = SERIES.len();
    let mut lower = vec![0.0f32; n];
    for (c, name) in COHORTS.iter().enumerate() {
        let upper: Vec<f32> = (0..n)
            .map(|i| lower[i] + (SERIES[i].1[c] as f64 / SUPPLY) as f32)
            .collect();
        let mut path: Vec<Pos2> = (0..n)
            .map(|i| pos2(xs[i], rect.bottom() - upper[i] * rect.height()))
            .collect();
        path.extend(
            (0..n)
                .rev()
                .map(|i| pos2(xs[i], rect.bottom() - lower[i] * rect.height())),
        );
        ui.painter().add(egui::Shape::Path(egui::epaint::PathShape {
            points: path,
            closed: true,
            fill: cohort_color(name),
            stroke: egui::epaint::PathStroke::NONE,
        }));
        lower = upper;
    }
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Token kinetic — variants on $Aliens")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Supply is CONSERVED MASS moving between reservoirs. 200 points, 2026-02 → 2026-08.",
        )
        .color(TEXT_MUTED)
        .small(),
    );

    let n = SERIES.len();
    let evs = events(0.02);

    // ---- V1 · linear time, events marked ------------------------------
    caption(
        ui,
        "V1 · clock time, with events named",
        "The safe form plus punctuation. Note how much width the quiet tail takes.",
    );
    let rect = frame(ui, 130.0);
    let xs: Vec<f32> = (0..n)
        .map(|i| rect.left() + rect.width() * i as f32 / (n - 1) as f32)
        .collect();
    bands(ui, rect, &xs);
    for e in &evs {
        let x = xs[e.i];
        ui.painter().line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            egui_widgets::theme::hairline(Color32::from_rgba_premultiplied(255, 255, 255, 60)),
        );
        let up = e.delta > 0.0;
        ui.painter().circle_filled(
            pos2(x, rect.top() + 6.0),
            3.0,
            if up {
                Color32::WHITE
            } else {
                Color32::from_rgb(0xe0, 0xaf, 0x68)
            },
        );
    }

    // ---- V2 · event-warped time ---------------------------------------
    caption(
        ui,
        "V2 · time allocated by ACTIVITY, not by clock",
        "Each step's width scales with how much moved. The launch breathes; dormancy compresses to a seam.",
    );
    let rect = frame(ui, 130.0);
    // Width per step = a floor plus the total absolute movement in that step.
    // Clock time gives 87% of the canvas to nothing happening; this gives space
    // to where the token's life actually was.
    let mut w: Vec<f32> = (1..n)
        .map(|i| {
            let moved: f64 = (0..COHORTS.len())
                .map(|c| ((SERIES[i].1[c] - SERIES[i - 1].1[c]) as f64 / SUPPLY).abs())
                .sum();
            0.25 + moved as f32 * 40.0
        })
        .collect();
    let total: f32 = w.iter().sum();
    for x in &mut w {
        *x = *x / total * rect.width();
    }
    let mut warped = vec![rect.left()];
    for x in &w {
        warped.push(warped.last().unwrap() + x);
    }
    bands(ui, rect, &warped);
    for e in &evs {
        let x = warped[e.i];
        ui.painter().line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            egui_widgets::theme::hairline(Color32::from_rgba_premultiplied(255, 255, 255, 70)),
        );
    }

    // ---- V3 · reservoirs at an instant, with the future ahead ----------
    caption(
        ui,
        "V3 · reservoirs at the playhead — mass, not a curve",
        "Each reservoir sized by holding. The vesting tank carries what is PAST MATURITY and still sitting there.",
    );
    let rect = frame(ui, 150.0);
    let last = &SERIES[n - 1];
    let pad = 14.0;
    let cell = (rect.width() - pad * (COHORTS.len() + 1) as f32) / COHORTS.len() as f32;
    for (c, name) in COHORTS.iter().enumerate() {
        let frac = (last.1[c] as f64 / SUPPLY) as f32;
        let x = rect.left() + pad + c as f32 * (cell + pad);
        let h = (rect.height() - 44.0) * frac.max(0.004);
        let tank = Rect::from_min_size(pos2(x, rect.bottom() - 22.0 - h), Vec2::new(cell, h));
        ui.painter().rect_filled(tank, 2.0, cohort_color(name));
        ui.painter().text(
            pos2(x, rect.bottom() - 18.0),
            egui::Align2::LEFT_TOP,
            format!("{name}  {:.2}%", 100.0 * frac),
            egui::FontId::proportional(11.0),
            Color32::from_rgb(0xa9, 0xb1, 0xd6),
        );
    }
    ui.painter().text(
        pos2(rect.left() + pad, rect.top() + 6.0),
        egui::Align2::LEFT_TOP,
        "conserved: the tanks always sum to supply — mass moves, it is never created",
        egui::FontId::proportional(11.0),
        Color32::from_rgb(0x6b, 0x73, 0x94),
    );

    ui.add_space(12.0);
    ui.separator();
    ui.label(
        egui::RichText::new("What these are testing:")
            .color(ACCENT)
            .strong(),
    );
    ui.label("\u{2022} V1 — do named events rescue the safe form, or just decorate it?");
    ui.label("\u{2022} V2 — does activity-warped time make the launch legible? (it is ~10% of clock time)");
    ui.label(
        "\u{2022} V3 — does mass-in-tanks read better than a curve, and does conservation show?",
    );
    ui.label(
        "\u{2022} None of these ANIMATE yet — that is the point of the next pass, and where a shader stack would earn its place.",
    );
}
