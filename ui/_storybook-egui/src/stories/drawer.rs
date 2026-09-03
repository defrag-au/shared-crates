//! Story: `Drawer` — the narrow-layout answer to a side panel.
//!
//! Best reviewed at a real phone viewport, where the widget's whole reason for
//! existing is visible:
//!
//! ```sh
//! node ui/_storybook-egui/tools/cdp-shot.mjs \
//!   "http://127.0.0.1:8095/?nav=0#/drawer" 390 844 .tmp/drawer.png
//! ```
//!
//! The thing to check is the **scrim gap**: the drawer must not reach the far
//! edge, because that sliver of dimmed page is the only affordance telling a
//! reader the drawer is temporary and that tapping past it goes back.

use egui_widgets::{Breakpoint, Drawer, DrawerSide};

use crate::{ACCENT, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Drawer").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "An edge-anchored slide-over with a scrim, for the narrow layout of \
             a surface that has a side panel when it is wide. Scrim tap or \
             Escape dismisses; width clamps to the viewport so it can never be \
             the thing that overflows.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    let bp = Breakpoint::from_ui(ui);
    ui.label(
        egui::RichText::new(format!(
            "viewport: {:.0}pt — {}",
            ui.ctx().content_rect().width(),
            bp.label()
        ))
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let id = ui.id();
    let mut left_open = ui
        .data_mut(|d| d.get_temp::<bool>(id.with("left")))
        .unwrap_or(false);
    let mut right_open = ui
        .data_mut(|d| d.get_temp::<bool>(id.with("right")))
        .unwrap_or(false);

    ui.horizontal_wrapped(|ui| {
        if ui.button("Open from the left").clicked() {
            left_open = true;
        }
        if ui.button("Open from the right").clicked() {
            right_open = true;
        }
    });

    Drawer::new("story_left")
        .side(DrawerSide::Left)
        .width(320.0)
        .show(ui, &mut left_open, |ui| {
            ui.label(egui::RichText::new("Filters").strong());
            ui.separator();
            for name in ["Background", "Body", "Eyes", "Hat", "Mouth"] {
                ui.label(name);
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Tap the dimmed page, or press Escape, to close.")
                    .color(TEXT_MUTED)
                    .small(),
            );
        });

    Drawer::new("story_right")
        .side(DrawerSide::Right)
        .width(280.0)
        .show(ui, &mut right_open, |ui| {
            ui.label(egui::RichText::new("Detail").strong());
            ui.separator();
            ui.label("An inspector, an actions list — anything that belongs beside the content when there is room for it.");
        });

    ui.data_mut(|d| {
        d.insert_temp(id.with("left"), left_open);
        d.insert_temp(id.with("right"), right_open);
    });

    ui.add_space(16.0);
    ui.label(
        egui::RichText::new(
            "The caller picks drawer-or-panel from Breakpoint; the widget does \
             not decide that itself, so a drawer that is ALWAYS a drawer stays \
             possible.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
}
