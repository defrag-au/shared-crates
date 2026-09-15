//! `ChainTempo` — the tempo row over a simulated hour, with the pulse and train
//! it is meant to sit beside.
//!
//! The reconnect feed is the interesting one here: the gap figure skips the
//! break in heights rather than reporting one enormous wait.

use std::time::Duration;

use egui_widgets::block_pulse::BlockPulse;
use egui_widgets::block_train::BlockTrain;
use egui_widgets::chain_tempo::ChainTempo;

use super::chain_fixture::{ChainSim, Scenario, controls};

pub struct ChainTempoStory {
    sim: ChainSim,
}

impl Default for ChainTempoStory {
    fn default() -> Self {
        Self {
            sim: ChainSim::new(Scenario::Live),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut ChainTempoStory) {
    state.sim.advance(ui);

    ui.label(
        egui::RichText::new(
            "Every figure that has an expectation shows it. The epoch bar is the only \
             countdown: epochs end on a fixed slot, blocks do not.",
        )
        .color(crate::muted(ui))
        .size(11.0),
    );
    ui.add_space(8.0);
    controls(ui, &mut state.sim);
    ui.add_space(12.0);

    let (heartbeat, now) = (&state.sim.heartbeat, state.sim.now_ms());
    egui::Frame::new()
        .fill(crate::highlight(ui))
        .corner_radius(6)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width().min(760.0));
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Cardano mainnet")
                        .strong()
                        .color(crate::ink(ui)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    BlockPulse::new(heartbeat, now).id_salt("tempo").show(ui);
                });
            });
            ui.add_space(8.0);
            BlockTrain::new(heartbeat, now)
                .span(Duration::from_secs(30 * 60))
                .id_salt("tempo")
                .show(ui);
            ui.add_space(10.0);
            ChainTempo::new(heartbeat, now).show(ui);
        });
}
