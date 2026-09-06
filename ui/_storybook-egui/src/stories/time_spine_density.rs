//! `TimeSpine` density story — the same three years, drawn twice.
//!
//! Fixture shaped like a real NFT policy over three years at day buckets: a
//! four-day mint of ~6,700 transactions, an aftermarket that decays from ~60 a
//! day to single digits with a weekly rhythm, three trading spikes, a
//! two-month dead stretch, and a handful of burn days.
//!
//! **The thing to look at:** the top spine is [`egui_widgets::SpineLane::Marks`]
//! fed the way the policy view feeds it today — one hairline per transaction,
//! subsampled to a budget. Past a few per pixel it saturates: the mint, the
//! spikes and the quiet tail all paint the same solid bar, and the only thing
//! legible is the dead stretch. It shows when NOTHING happened.
//!
//! The bottom spine is [`egui_widgets::SpineLane::Density`] on the identical
//! counts: a waveform from the midline, one column per two pixels,
//! root-scaled against a robust ceiling so the mint burst and the frenzies
//! CLIP (drawn brighter) instead of flattening the aftermarket into a
//! hairline, with the mints and burns — the events that ARE discrete — kept
//! as marks over it. Same ruler, same playhead, same in/out hues; different
//! claim in the lane. Hover a column for its count.
//!
//! It is fuzzy, and that is the data: day-to-day variance on a thirty-a-day
//! collection is real, and a waveform is a form readers already know how to
//! read for loud, quiet and silent. Smoothing was considered and rejected
//! because it would attenuate exactly the one-day spikes the lane is for.

use crate::stories::capital_flow::month;
use crate::TEXT_MUTED;
use egui_widgets::{format_date, DensityBin, MarkKind, SpineState, TimeSpine};

const DAY: i64 = 86_400;
const DAYS: i64 = 3 * 365 + 1;
/// A Monday 00:00 UTC, so the ruler reads cleanly.
const T0: i64 = 1_693_180_800; // 2023-08-28

pub struct TimeSpineDensityState {
    as_marks: Option<SpineState>,
    as_density: Option<SpineState>,
    bins: Vec<DensityBin>,
    /// Mint and burn days, as the marks the density lane draws OVER the
    /// silhouette.
    events: Vec<(i64, MarkKind)>,
    /// The same history exploded into one mark per transaction and
    /// subsampled to the budget the policy view uses — what the lane shows
    /// today.
    marks: Vec<(i64, MarkKind)>,
}

impl Default for TimeSpineDensityState {
    fn default() -> Self {
        let (bins, events) = fixture();
        let marks = explode(&bins, &events);
        Self {
            as_marks: None,
            as_density: None,
            bins,
            events,
            marks,
        }
    }
}

/// Deterministic noise in `[0, 1)` — no rand dependency in a story.
fn noise(seed: i64) -> f64 {
    let mut x = (seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    x ^= x >> 29;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 32;
    (x % 10_000) as f64 / 10_000.0
}

/// Three years of a policy's life, one bin per day.
fn fixture() -> (Vec<DensityBin>, Vec<(i64, MarkKind)>) {
    let mint_days: [u64; 4] = [2_200, 3_400, 900, 250];
    let burn_days: [(i64, u64); 6] = [(130, 3), (131, 1), (410, 2), (800, 4), (801, 2), (802, 1)];
    let mut bins = Vec::with_capacity(DAYS as usize);
    let mut events = Vec::new();
    for d in 0..DAYS {
        let t = T0 + d * DAY;
        let mut count: f64 = if (d as usize) < mint_days.len() {
            events.push((t + DAY / 2, MarkKind::In));
            mint_days[d as usize] as f64
        } else {
            // Decaying baseline with a weekly rhythm — weekends are quieter.
            let base = 60.0 * (-(d as f64) / 400.0).exp() + 6.0;
            let weekly = 1.0 + 0.35 * ((d as f64) * std::f64::consts::TAU / 7.0).sin();
            base * weekly * (0.6 + 0.8 * noise(d))
        };
        // Trading spikes: a delist, a floor sweep, a late frenzy.
        for (day, mult, width) in [(45, 8.0, 2), (300, 5.0, 1), (700, 12.0, 3)] {
            if (d - day).abs() <= width {
                count *= mult / (1.0 + (d - day).abs() as f64);
            }
        }
        // The dead stretch — two months in which nothing at all moved.
        if (520..585).contains(&d) {
            count = 0.0;
        }
        if let Some(&(_, burns)) = burn_days.iter().find(|(day, _)| *day == d) {
            events.push((t + DAY / 2, MarkKind::Out));
            count += burns as f64;
        }
        let count = count.round() as u64;
        if count > 0 {
            bins.push(DensityBin {
                start: t,
                span: DAY,
                count,
            });
        }
    }
    (bins, events)
}

/// One mark per transaction, spread evenly inside its day, subsampled to the
/// budget the policy view uses — a faithful reproduction of the saturated
/// lane, not a strawman.
fn explode(bins: &[DensityBin], events: &[(i64, MarkKind)]) -> Vec<(i64, MarkKind)> {
    const BUDGET: usize = 1_500;
    let total: u64 = bins.iter().map(|b| b.count).sum();
    let scale = (BUDGET as f64 / total.max(1) as f64).min(1.0);
    let mut out = Vec::new();
    for b in bins {
        let k = ((b.count as f64 * scale).round() as usize).max(1);
        let kind = events
            .iter()
            .find(|(t, _)| *t >= b.start && *t < b.start + b.span)
            .map(|(_, k)| *k)
            .unwrap_or(MarkKind::Neutral);
        let step = b.span as f64 / k as f64;
        for i in 0..k {
            let t = b.start as f64 + (i as f64 + 0.5) * step;
            // The directional kind goes on the first mark of the day only —
            // one mint tx among many transfers is still one mint.
            out.push((t as i64, if i == 0 { kind } else { MarkKind::Neutral }));
        }
    }
    out
}

pub fn show(ui: &mut egui::Ui, state: &mut TimeSpineDensityState) {
    let domain = (T0, T0 + DAYS * DAY);
    let total: u64 = state.bins.iter().map(|b| b.count).sum();
    let tick = |t: i64, spacing: i64| {
        if spacing >= DAY * 10 {
            month(t)
        } else {
            format_date(t)
        }
    };

    ui.label(
        egui::RichText::new(format!(
            "{total} transactions over three years, drawn twice. Top: one mark per \
             transaction, as the policy view draws it today. Bottom: the same counts as \
             a density silhouette, mints and burns kept as marks. Hover a column."
        ))
        .small()
        .color(TEXT_MUTED),
    );
    ui.add_space(8.0);

    ui.label(egui::RichText::new("as marks — saturated").strong());
    let spine = state
        .as_marks
        .get_or_insert_with(|| SpineState::new(domain));
    TimeSpine::new(spine)
        .format_tick(&tick)
        .marks(&state.marks)
        .height(48.0)
        .brushing(false)
        .show(ui);

    ui.add_space(14.0);

    ui.label(egui::RichText::new("as density — the same counts").strong());
    let spine = state
        .as_density
        .get_or_insert_with(|| SpineState::new(domain));
    TimeSpine::new(spine)
        .format_tick(&tick)
        .density(&state.bins, &state.events)
        .height(48.0)
        .brushing(false)
        .show(ui);

    ui.add_space(14.0);

    ui.label(egui::RichText::new("as density — taller lane, zoom with scroll").strong());
    // A third, taller, so the log scale has room: the policy view can afford
    // 64 where the wallet feed runs at 48.
    let spine = state.as_density.as_mut().expect("just inserted");
    TimeSpine::new(spine)
        .format_tick(&tick)
        .density(&state.bins, &state.events)
        .height(72.0)
        .brushing(false)
        .show_play(false)
        .show(ui);
}
