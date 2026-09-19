//! `BlockPulse` — every feed state side by side, then in a status strip.
//!
//! Each column runs its own simulated feed, so the live one really pops and its
//! ring really fills while the quiet and offline ones really do not. That is the
//! claim the widget makes, so the story shows it rather than asserting it.

use egui_widgets::block_pulse::{BlockPulse, PulseDetail};
use egui_widgets::theme::TextSize;

use super::chain_fixture::{ChainSim, Scenario};

pub struct BlockPulseStory {
    sims: Vec<ChainSim>,
}

impl Default for BlockPulseStory {
    fn default() -> Self {
        Self {
            sims: Scenario::ALL.into_iter().map(ChainSim::new).collect(),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut BlockPulseStory) {
    for sim in &mut state.sims {
        sim.advance(ui);
    }

    ui.label(
        egui::RichText::new(
            "Hover a pulse for the block and what the state means. The ring is the \
             chance a block has landed, never a countdown.",
        )
        .color(crate::muted(ui))
        .size(11.0),
    );
    ui.add_space(12.0);

    egui::Grid::new("block_pulse_states")
        .num_columns(3)
        .spacing([24.0, 14.0])
        .show(ui, |ui| {
            for label in ["feed", "full", "compact"] {
                ui.label(
                    egui::RichText::new(label)
                        .color(crate::muted(ui))
                        .size(10.0),
                );
            }
            ui.end_row();
            for (i, sim) in state.sims.iter().enumerate() {
                ui.label(
                    egui::RichText::new(sim.scenario.label())
                        .color(crate::secondary(ui))
                        .size(11.0),
                );
                BlockPulse::new(&sim.heartbeat, sim.now_ms())
                    .id_salt(("full", i))
                    .show(ui);
                BlockPulse::new(&sim.heartbeat, sim.now_ms())
                    .detail(PulseDetail::Compact)
                    .id_salt(("compact", i))
                    .show(ui);
                ui.end_row();
            }
        });

    ui.add_space(18.0);
    ui.label(
        egui::RichText::new("In a status strip, where it is meant to live")
            .color(crate::muted(ui))
            .size(11.0),
    );
    ui.add_space(4.0);
    let live = &state.sims[0];
    egui::Frame::new()
        .fill(crate::highlight(ui))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width().min(720.0));
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("hodlcroft")
                        .strong()
                        .color(crate::ink(ui)),
                );
                ui.label(egui::RichText::new("market").color(crate::muted(ui)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // A right-to-left parent: the pulse pins its own reading
                    // order, which this placement is here to prove.
                    BlockPulse::new(&live.heartbeat, live.now_ms())
                        .text(TextSize::Sm)
                        .id_salt("strip")
                        .show(ui);
                });
            });
        });
}
