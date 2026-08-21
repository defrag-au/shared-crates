//! `FlowStave` story — a front wallet's money story as a sequence chart.
//!
//! Fixture shaped like the real S2 case that motivated the widget: a wallet
//! the treasury funds in rounds, mint batches sparking into it minutes later,
//! and the assets forwarding on to a founder wallet within the hour — plus a
//! small side-payment to an ops wallet. In the transfers TABLE that cadence
//! took timestamp-juggling to see; here it is a visible repeating rhythm.
//!
//! **What to look for:** the fund → mint → forward cascade clusters tightly
//! (log-compressed gaps keep minutes visible next to idle days); arrows point
//! where the value went, blue toward the focal lane, orange away; mints are
//! diamonds, not arrows — created, not received. Scrub the spine: the
//! playhead line sweeps the chart and the future dims. Click an event to
//! move the playhead there.

use crate::TEXT_MUTED;
use egui_widgets::{FlowStave, Selection, SpineState, StaveEvent, StaveLane, StaveOrigin};

const T0: i64 = 1_785_950_000; // 2026-08-05-ish, matching the case
const H: i64 = 3_600;
const DAY: i64 = 86_400;

pub struct FlowStaveState {
    spine: Option<SpineState>,
    selection: Selection,
    /// The current subject — clicking a lane header refocuses onto it.
    focal: String,
    /// Where the reader came from, for the back button. The WIDGET only
    /// reports `clicked_lane`; this navigation stack is deliberately the
    /// caller's, because "back" must survive face switches the widget cannot
    /// see.
    history: Vec<String>,
    /// Last clicked event, for the detail card below the stave.
    detail: Option<usize>,
}

impl Default for FlowStaveState {
    fn default() -> Self {
        Self {
            spine: None,
            selection: Selection::default(),
            focal: FOCAL.to_string(),
            history: Vec::new(),
            detail: None,
        }
    }
}

const FOCAL: &str = "front-wallet";

fn lanes() -> Vec<StaveLane<'static>> {
    vec![
        StaveLane::new(FOCAL, 3),
        StaveLane::new("S2 treasury", 0),
        StaveLane::new("$founder", 0),
        StaveLane::new("ops (pervsn)", 1),
        StaveLane::new("collector", 3),
    ]
}

fn events() -> Vec<StaveEvent<'static>> {
    let mut out: Vec<StaveEvent<'static>> = Vec::new();
    let mut push = |timestamp: i64,
                    from: StaveOrigin<'static>,
                    to: &'static str,
                    label: &str,
                    items: i32,
                    magnitude: f32| {
        out.push(StaveEvent {
            timestamp,
            from,
            to,
            label: label.to_string(),
            items,
            magnitude,
            tx: "d84561d75cc51201aa00bb11cc22dd33ee44ff55",
        });
    };

    // Round 1: the treasury loads the front, part flows back, then the
    // mint→forward cadence begins.
    push(T0, StaveOrigin::Party("S2 treasury"), FOCAL, "5,000 ₳", 0, 1.0);
    push(
        T0 + 5 * 60,
        StaveOrigin::Party(FOCAL),
        "S2 treasury",
        "1,500 ₳",
        0,
        0.55,
    );
    for round in 0..5i64 {
        let base = T0 + 2 * DAY + round * DAY + (round % 3) * 3 * H;
        // A payment to the collector precedes each batch — the queued-mint
        // shape: pay one tx, receive in another.
        push(
            base - 40 * 60,
            StaveOrigin::Party(FOCAL),
            "collector",
            "1,050 ₳",
            0,
            0.6,
        );
        push(base, StaveOrigin::Mint, FOCAL, "12 assets", 12, 0.4);
        push(
            base + 25 * 60,
            StaveOrigin::Party(FOCAL),
            "$founder",
            "12 items",
            12,
            0.5,
        );
        // Top-ups from the treasury keep the front liquid.
        if round % 2 == 1 {
            push(
                base + 2 * H,
                StaveOrigin::Party("S2 treasury"),
                FOCAL,
                "1,638 ₳",
                0,
                0.8,
            );
        }
    }
    // A small ops payment, and one receipt nobody could attribute.
    push(
        T0 + 4 * DAY + 6 * H,
        StaveOrigin::Party(FOCAL),
        "ops (pervsn)",
        "25 ₳",
        0,
        0.2,
    );
    push(
        T0 + 6 * DAY,
        StaveOrigin::Unresolved,
        FOCAL,
        "847 ₳",
        0,
        0.5,
    );
    out.sort_by_key(|e| e.timestamp);
    out
}

pub fn show(ui: &mut egui::Ui, state: &mut FlowStaveState) {
    let events = events();
    let lo = events.first().map(|e| e.timestamp).unwrap_or(T0) - DAY / 2;
    let hi = events.last().map(|e| e.timestamp).unwrap_or(T0) + DAY / 2;
    let spine = state.spine.get_or_insert_with(|| SpineState::new((lo, hi)));

    ui.label(
        egui::RichText::new(
            "One wallet's story as a SEQUENCE CHART: the focal lane in the centre, \
             counterparties fanning out by ring class, time running downward with \
             log-compressed gaps — a five-minute cascade stays a visible cluster while \
             an idle week stays a bounded gap, and the true clock is printed in the \
             gutter so the compression never has to be trusted.\n\n\
             Direction is an ARROW: blue toward the focal wallet, orange away. A mint is \
             a DIAMOND on the receiving lane — created here, not received from anyone — \
             and an unresolved payer arrives from the chart's edge. Asset movements wear \
             a count chip; every arrow carries its own unit label, so ADA, tokens and \
             items keep their identity on one chart.\n\n\
             Scrub the spine: the playhead sweeps the chart and the future dims. This \
             fixture is the real S2 shape — treasury loads a front, the front pays the \
             collector, batches mint in, assets forward to a founder minutes later.",
        )
        .small()
        .color(TEXT_MUTED),
    );
    ui.add_space(4.0);

    let tick = |t: i64, _s: i64| egui_widgets::format_date(t);
    egui_widgets::TimeSpine::new(spine)
        .format_tick(&tick)
        .height(52.0)
        .show(ui);
    ui.add_space(4.0);

    // ── navigation: click a lane header to refocus; back to return ────────
    ui.horizontal(|ui| {
        let back = ui
            .add_enabled(!state.history.is_empty(), egui::Button::new("← back"))
            .on_hover_text("return to the previous subject");
        if back.clicked() {
            if let Some(prev) = state.history.pop() {
                state.focal = prev;
            }
        }
        ui.label(
            egui::RichText::new(format!("subject: {}", state.focal))
                .small()
                .color(TEXT_MUTED),
        );
    });

    let lanes = lanes();
    let focal = state.focal.clone();
    let r = FlowStave::new(&focal, &lanes, &events, spine, &mut state.selection)
        .height(410.0)
        .show(ui);

    if let Some(i) = r.clicked {
        spine.set_playhead(events[i].timestamp);
        state.detail = Some(i);
    }
    // Row click = detail, demoed the way the app composes it: the widget
    // reports the index, the host renders whatever "more" means to it.
    if let Some(e) = state.detail.and_then(|i| events.get(i)) {
        let from = match e.from {
            StaveOrigin::Party(p) => p,
            StaveOrigin::Mint => "mint · created here",
            StaveOrigin::Unresolved => "unresolved payer",
        };
        ui.add_space(4.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("event").small().color(TEXT_MUTED));
                ui.label(format!("{from} → {}: {}", e.to, e.label));
                if ui.small_button("×").clicked() {
                    state.detail = None;
                }
            });
            egui_widgets::IdPill::new("tx", e.tx)
                .layout(egui_widgets::IdPillLayout::Inline)
                .show(ui);
        });
    }
    // The widget reports intent; the story owns the stack. Same division the
    // app will use.
    if let Some(next) = r.clicked_lane {
        state.history.push(state.focal.clone());
        state.focal = next;
    }
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(format!(
            "{} events on the stave · {} outside the brush · click an event to move the \
             playhead · click a lane name to make it the subject",
            r.events_shown, r.events_clipped
        ))
        .small()
        .color(TEXT_MUTED),
    );
}
