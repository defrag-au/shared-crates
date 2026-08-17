//! `CapitalFlow` story — Mekka S1: 445,417 ADA banked from the mint, and where
//! it went.
//!
//! Real proportions from the treasury analysis (45.1% off-ramp / 28.3% own ops /
//! 22.1% community / 2.3% holder rewards), spread over the deployment window.
//! The 2.3% band is the point: holder rewards were not in the published
//! 80/15/5 split at all, and on a chart you watch it appear.
//!
//! Deployment also crosses the raise line — the treasury banked 445,417 and
//! deployed 577,950, because royalties and other income moved through the same
//! wallets. That crossing is a finding, which is why the widget does not clamp.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{
    capital_bands, capital_legend, cumulative_at, Acquisition, Arrival, CapitalFlow, FlowEvent,
    HolderFormation, MintArrivals,
};

pub(crate) const RAISED: i128 = 445_417_000_000;
const DAY: i64 = 86_400;
/// 2025-08-01 — roughly when S1 deployment begins.
const T0: i64 = 1_754_006_400;

pub struct CapitalFlowState {
    pub playhead: i64,
    pub playing: bool,
}

impl Default for CapitalFlowState {
    fn default() -> Self {
        Self {
            // i64::MAX is clamped to the series end on first frame — the story
            // opens finished, and play rewinds to show how it got there.
            playhead: i64::MAX,
            playing: false,
        }
    }
}

pub(crate) fn ada(v: i128) -> String {
    let a = v as f64 / 1e6;
    if a >= 1_000.0 {
        format!("{:.0}k ADA", a / 1_000.0)
    } else {
        format!("{a:.0} ADA")
    }
}

/// Days since epoch → `YYYY-MM`. The chart spans a year, so a month is the
/// resolution a reader can actually hold.
pub(crate) fn month(ts: i64) -> String {
    let (mut y, mut d) = (1970i64, ts / DAY);
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let len = if leap { 366 } else { 365 };
        if d < len {
            break;
        }
        d -= len;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let ml = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0;
    while d >= ml[m] {
        d -= ml[m];
        m += 1;
    }
    format!("{y}-{:02}", m + 1)
}

/// Who received assets, and when. Mekka S1 shape: 5,000 assets landing over the
/// mint, concentrated at the top — the largest holder group ended on 13.5% of
/// rarity-weighted supply.
fn acquisitions() -> Vec<Acquisition<'static>> {
    let mut out = Vec::new();
    // A long tail of ordinary buyers arriving across the mint window.
    for i in 0..160u32 {
        let day = (i % 40) as i64;
        out.push(Acquisition::new(
            T0 + day * 3 * DAY / 2,
            Box::leak(format!("buyer-{i:03}").into_boxed_str()),
            3 + (i % 5) as i64,
        ));
    }
    // The concentrated end: a handful of wallets taking far more, arriving
    // early. This is what makes the bottom band swell.
    for (i, n) in [(0u32, 340i64), (1, 210), (2, 160), (3, 120), (4, 90)] {
        out.push(Acquisition::new(
            T0 + (i as i64) * DAY,
            Box::leak(format!("whale-{i}").into_boxed_str()),
            n,
        ));
    }
    out.sort_by_key(|a| a.timestamp);
    out
}

/// Deployment events, spread across the window in the observed proportions.
pub(crate) fn events() -> Vec<FlowEvent<'static>> {
    // (destination, share of total deployed, first month, last month)
    const PLAN: &[(&str, f64, i64, i64)] = &[
        ("off-ramp", 0.451, 0, 11),
        ("own ops wallets", 0.283, 0, 11),
        ("community / team", 0.221, 1, 10),
        ("holder rewards", 0.023, 1, 9),
        ("script / DEX", 0.022, 2, 8),
    ];
    const DEPLOYED: i128 = 577_950_000_000;

    let mut out = Vec::new();
    for (name, share, first, last) in PLAN {
        let total = (DEPLOYED as f64 * share) as i128;
        let n = (last - first + 1).max(1);
        for k in 0..n {
            out.push(FlowEvent::new(
                T0 + (first + k) * 30 * DAY,
                name,
                total / n as i128,
            ));
        }
    }
    out.sort_by_key(|e| e.timestamp);
    out
}

/// Same shape as `acquisitions`, as per-asset arrivals for the dot field.
pub(crate) fn arrivals() -> Vec<Arrival<'static>> {
    acquisitions()
        .into_iter()
        .map(|a| Arrival::new(a.timestamp, a.holder, a.count.max(0) as u32))
        .collect()
}

pub fn show(ui: &mut egui::Ui, state: &mut CapitalFlowState) {
    ui.label(egui::RichText::new("Capital Flow").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "They raised X — watch where it went. Drag the timeline or press play.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(10.0);

    let events = events();
    let bands = capital_bands(&events);
    let (t0, t1) = (
        events.first().map(|e| e.timestamp).unwrap_or(T0),
        events.last().map(|e| e.timestamp).unwrap_or(T0 + DAY),
    );
    state.playhead = state.playhead.clamp(t0, t1);

    ui.horizontal(|ui| {
        if ui
            .button(if state.playing { "⏸" } else { "▶" })
            .on_hover_text("play / pause")
            .clicked()
        {
            state.playing = !state.playing;
            // Replay from the start rather than sitting at the end.
            if state.playing && state.playhead >= t1 {
                state.playhead = t0;
            }
        }
        if ui.button("⏮").on_hover_text("back to the start").clicked() {
            state.playhead = t0;
            state.playing = false;
        }
        if ui.button("⏭").on_hover_text("jump to the end").clicked() {
            state.playhead = t1;
            state.playing = false;
        }
        ui.label(
            egui::RichText::new(month(state.playhead))
                .monospace()
                .size(11.0),
        );
    });

    if state.playing {
        // Fixed step per frame, not wall-clock: the run is then reproducible,
        // and a screenshot of frame N is the same picture every time.
        state.playhead = (state.playhead + (t1 - t0) / 240).min(t1);
        if state.playhead >= t1 {
            state.playing = false;
        }
        ui.ctx().request_repaint();
    }

    let resp = CapitalFlow::new(&events, &bands, RAISED, state.playhead, &ada, &month)
        .height(280.0)
        .show(ui);
    if let Some(t) = resp.scrubbed_to {
        state.playhead = t;
        state.playing = false;
    }

    ui.add_space(6.0);
    let at = cumulative_at(&events, &bands, state.playhead);
    capital_legend(ui, &bands, &at, &ada);

    // ── the other face, on the same playhead ───────────────────────────
    ui.add_space(14.0);
    ui.label(
        egui::RichText::new("…and who became a holder")
            .color(ACCENT)
            .strong(),
    );
    let acq = acquisitions();
    HolderFormation::new(&acq, state.playhead, &month)
        .supply(2_000)
        .height(190.0)
        .show(ui);

    // ── every asset, as a dot arriving with its holder ─────────────────
    ui.add_space(14.0);
    ui.label(
        egui::RichText::new("…one asset at a time")
            .color(ACCENT)
            .strong(),
    );
    let arr = arrivals();
    MintArrivals::new(&arr, state.playhead)
        .flight(4 * DAY)
        .height(300.0)
        .show(ui);

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(
            "Every dot is one asset, every pile one holder, laid out in arrival order. Dot k \
             always sits in the same place in its pile, so piles grow outward and never \
             reshuffle — press play and watch the whales form.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.label(
        egui::RichText::new(
            "Two faces of one mint on one playhead: money leaving above, people arriving below.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.label(
        egui::RichText::new(
            "Watch the thin band appear: holder rewards took 2.3% of mint funds — a category \
             the published 80/15/5 split does not contain.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.label(
        egui::RichText::new(
            "The stack crosses the raise line because royalties and other income moved through \
             the same wallets. Clamping to the raise would hide exactly that.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
}
