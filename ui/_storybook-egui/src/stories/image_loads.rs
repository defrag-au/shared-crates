//! Image loads story — the fetch scheduler, watched.
//!
//! Two shelves of real IIIF art (the Hodlcroft Pirates, split in half) on one
//! grid, at full size so a fetch takes long enough to see. Switch shelves and
//! watch the counts:
//!
//! - **Cancel on switch** drops the old shelf's pending loads at once. That
//!   is what collection-ownership does on navigation.
//! - **Leave it to demand** lets the grace period do it: nothing paints the
//!   old shelf, so its loads are cancelled a grace later.
//! - **Sticky** is the old `egui_extras` behaviour, where every load finishes.
//!   Switch back and forth quickly and the in-flight count stays pinned at the
//!   budget, all of it spent on art nobody is looking at.
//!
//! Scrolling the grid shows the same thing at a smaller scale: rows scrolled
//! past before they load are cancelled too. **Forget all** empties the caches
//! so you can watch it again.
//!
//! **Retain** is the other half, and the one that bounds memory rather than
//! bandwidth: completed art is kept until the cap, then the coldest is
//! released — bytes, decoded image and texture together. Wind it below what
//! fits on screen and the grid eats itself, refetching art as it repaints;
//! that thrashing is exactly what a cap set too low buys, and why the default
//! is several screens' worth. At `everything` it grows without limit, which is
//! what egui's own loaders do and what ran a tab to gigabytes.

use std::time::Duration;

use egui::{RichText, Sense, Vec2};
use egui_widgets::card_browser::{self, CardBrowserConfig};
use egui_widgets::image_loader::CachedSpinner;
use egui_widgets::image_loader::schedule::{Demand, LoadCounts, LoadPolicy, Retain};
use egui_widgets::theme::ThemeExt as _;
use image_core::ImageSize;

use crate::stories::card_browser::PIRATE_HEX;

const POLICY_ID: &str = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shelf {
    Front,
    Back,
}

impl Shelf {
    fn assets(self) -> &'static [&'static str] {
        let half = PIRATE_HEX.len() / 2;
        match self {
            Self::Front => &PIRATE_HEX[..half],
            Self::Back => &PIRATE_HEX[half..],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OnSwitch {
    CancelPending,
    LeaveToDemand,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DemandChoice {
    Visible,
    Sticky,
}

pub struct ImageLoadsState {
    shelf: Shelf,
    on_switch: OnSwitch,
    demand: DemandChoice,
    budget: usize,
    grace_secs: f32,
    /// Completed images kept. Wind it below the number on screen and the grid
    /// visibly eats itself — released art refetches the moment it is painted
    /// again, which is what the cap is trading against.
    retain: usize,
}

impl Default for ImageLoadsState {
    fn default() -> Self {
        let policy = LoadPolicy::default();
        let grace_secs = match policy.demand {
            Demand::Visible { grace } => grace.as_secs_f32(),
            Demand::Sticky => 1.0,
        };
        Self {
            shelf: Shelf::Front,
            on_switch: OnSwitch::CancelPending,
            demand: DemandChoice::Visible,
            budget: policy.budget,
            grace_secs,
            retain: match policy.retain {
                Retain::Coldest { images } => images,
                Retain::Everything => 0,
            },
        }
    }
}

impl ImageLoadsState {
    fn policy(&self) -> LoadPolicy {
        LoadPolicy {
            budget: self.budget,
            demand: match self.demand {
                DemandChoice::Visible => Demand::Visible {
                    grace: Duration::from_secs_f32(self.grace_secs),
                },
                DemandChoice::Sticky => Demand::Sticky,
            },
            // Zero reads as "keep everything" on the slider — the far end of
            // the same axis rather than a separate switch to forget about.
            retain: match self.retain {
                0 => Retain::Everything,
                images => Retain::Coldest { images },
            },
        }
    }
}

/// The loader's handle — browser-only, like the loader itself.
#[cfg(target_arch = "wasm32")]
type Loads = egui_widgets::image_loader::fetch::ImageLoads;
#[cfg(not(target_arch = "wasm32"))]
type Loads = ();

#[cfg(target_arch = "wasm32")]
fn loads(ctx: &egui::Context) -> Option<Loads> {
    egui_widgets::image_loader::fetch::loads(ctx)
}
#[cfg(not(target_arch = "wasm32"))]
fn loads(_ctx: &egui::Context) -> Option<Loads> {
    None
}

pub fn show(ui: &mut egui::Ui, state: &mut ImageLoadsState) {
    let muted = ui.tokens().color.text_muted;
    let loads = loads(ui.ctx());

    let before = (state.shelf, state.policy());
    ui.horizontal_wrapped(|ui| {
        ui.label("Shelf");
        ui.selectable_value(&mut state.shelf, Shelf::Front, "Front 25");
        ui.selectable_value(&mut state.shelf, Shelf::Back, "Back 25");
        ui.separator();
        ui.label("On switch");
        ui.selectable_value(
            &mut state.on_switch,
            OnSwitch::CancelPending,
            "Cancel pending",
        );
        ui.selectable_value(
            &mut state.on_switch,
            OnSwitch::LeaveToDemand,
            "Leave it to demand",
        );
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Demand");
        ui.selectable_value(&mut state.demand, DemandChoice::Visible, "Visible");
        ui.selectable_value(&mut state.demand, DemandChoice::Sticky, "Sticky");
        ui.add_enabled(
            state.demand == DemandChoice::Visible,
            egui::Slider::new(&mut state.grace_secs, 0.0..=5.0).text("grace s"),
        );
        ui.add(egui::Slider::new(&mut state.budget, 1..=32).text("budget"));
        ui.add(
            egui::Slider::new(&mut state.retain, 0..=64)
                .text("retain")
                .custom_formatter(|n, _| {
                    if n < 1.0 {
                        "everything".to_owned()
                    } else {
                        format!("{n:.0}")
                    }
                }),
        );
        if ui.button("Forget all").clicked() {
            ui.ctx().forget_all_images();
        }
    });

    if let Some(loads) = &loads {
        apply(loads, state, before);
        draw_counts(ui, counts(loads));
    } else {
        ui.label(
            RichText::new("The fetch scheduler is browser-only; counts appear in the wasm build.")
                .color(muted),
        );
    }
    ui.add_space(8.0);

    let config = CardBrowserConfig::default();
    let side = 132.0;
    let mut loading = false;
    egui::ScrollArea::vertical()
        .id_salt("image_loads_grid")
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for hex in state.shelf.assets() {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
                    let url = image_core::iiif_asset_url(POLICY_ID, hex, ImageSize::Full);
                    loading |= card_browser::draw_thumbnail(ui, rect, Some(&url), &config);
                }
            });
        });
    if loading {
        CachedSpinner::request_repaint(ui);
    } else if loads.is_some() {
        // Cancellations land a grace after the last paint; keep the counts live.
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }
}

#[cfg(target_arch = "wasm32")]
fn apply(loads: &Loads, state: &ImageLoadsState, (shelf, policy): (Shelf, LoadPolicy)) {
    if state.policy() != policy {
        loads.set_policy(state.policy());
    }
    if state.shelf != shelf {
        match state.on_switch {
            OnSwitch::CancelPending => loads.cancel_pending(),
            OnSwitch::LeaveToDemand => {}
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn apply(_loads: &Loads, _state: &ImageLoadsState, _before: (Shelf, LoadPolicy)) {}

#[cfg(target_arch = "wasm32")]
fn counts(loads: &Loads) -> LoadCounts {
    loads.counts()
}
#[cfg(not(target_arch = "wasm32"))]
fn counts(_loads: &Loads) -> LoadCounts {
    LoadCounts::default()
}

fn draw_counts(ui: &mut egui::Ui, counts: LoadCounts) {
    let LoadCounts {
        queued,
        in_flight,
        ready,
        failed,
        cancelled,
    } = counts;
    let muted = ui.tokens().color.text_muted;
    ui.horizontal_wrapped(|ui| {
        for (label, value) in [
            ("queued", queued as u64),
            ("in flight", in_flight as u64),
            ("ready", ready as u64),
            ("failed", failed as u64),
            ("cancelled", cancelled),
        ] {
            ui.label(RichText::new(value.to_string()).strong().size(15.0));
            ui.label(RichText::new(label).color(muted));
            ui.add_space(10.0);
        }
    });
}
