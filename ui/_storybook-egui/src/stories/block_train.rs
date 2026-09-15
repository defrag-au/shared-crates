//! `BlockTrain` — a simulated feed you can speed up, break and reconnect, with a
//! transaction you can send and watch ride to its block.
//!
//! "Send a transaction" starts a rider that waits in the gap and lands in the
//! first block after it was sent. "Add a rival" puts a competing buy in the same
//! block, so the beaten state is visible beside the winning one.

use std::time::Duration;

use egui_widgets::block_train::{BlockTrain, RiderState, TrainRider};

use super::chain_fixture::{ChainSim, Scenario, controls};

pub struct BlockTrainStory {
    sim: ChainSim,
    sent_at_secs: Option<u64>,
    rival: bool,
    span_minutes: u16,
    hovered: Option<u64>,
}

impl Default for BlockTrainStory {
    fn default() -> Self {
        Self {
            sim: ChainSim::new(Scenario::Live),
            sent_at_secs: None,
            rival: false,
            span_minutes: 20,
            hovered: None,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut BlockTrainStory) {
    state.sim.advance(ui);

    ui.label(
        egui::RichText::new(
            "Blocks spaced by real time; the band at the right is the wait. Hover a bar \
             for its block. Try the quiet and rollback feeds.",
        )
        .color(crate::muted(ui))
        .size(11.0),
    );
    ui.add_space(8.0);
    let scenario_before = state.sim.scenario;
    controls(ui, &mut state.sim);
    if state.sim.scenario != scenario_before {
        state.sent_at_secs = None;
    }

    ui.horizontal(|ui| {
        if ui.button("Send a transaction").clicked() {
            state.sent_at_secs = Some(state.sim.now_secs());
        }
        ui.checkbox(&mut state.rival, "Add a rival");
        if ui.button("Clear").clicked() {
            state.sent_at_secs = None;
        }
        ui.separator();
        ui.label(egui::RichText::new("span").color(crate::muted(ui)));
        ui.add(egui::Slider::new(&mut state.span_minutes, 2..=60).suffix(" min"));
    });
    ui.add_space(12.0);

    let riders = riders(state);
    let train = BlockTrain::new(&state.sim.heartbeat, state.sim.now_ms())
        .span(Duration::from_secs(state.span_minutes as u64 * 60))
        .riders(&riders)
        .show(ui);
    state.hovered = train.hovered_height.or(state.hovered);

    ui.add_space(6.0);
    let caption = match state.hovered {
        Some(height) => format!("Last hovered: block {height}"),
        None => "Hover a block".to_string(),
    };
    ui.label(
        egui::RichText::new(caption)
            .color(crate::muted(ui))
            .size(10.0),
    );

    ui.add_space(18.0);
    ui.label(
        egui::RichText::new("Narrow, in a card")
            .color(crate::muted(ui))
            .size(11.0),
    );
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(crate::highlight(ui))
        .corner_radius(6)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(360.0);
            BlockTrain::new(&state.sim.heartbeat, state.sim.now_ms())
                .span(Duration::from_secs(8 * 60))
                .plot_height(36.0)
                .riders(&riders)
                .id_salt("narrow")
                .show(ui);
        });
}

/// The rider lands in the first block that began after it was sent.
fn riders(state: &BlockTrainStory) -> Vec<TrainRider> {
    let Some(sent) = state.sent_at_secs else {
        return Vec::new();
    };
    let landed = state
        .sim
        .heartbeat
        .beats()
        .find(|b| b.block_time_unix.is_some_and(|t| t > sent))
        .map(|b| b.height);
    let mine = TrainRider::new(
        "your buy",
        match landed {
            Some(height) => RiderState::InBlock { height },
            None => RiderState::Waiting,
        },
    );
    let mut out = vec![mine];
    if state.rival
        && let Some(height) = landed
    {
        out.push(TrainRider::new("their buy", RiderState::Beaten { height }));
    }
    out
}
