//! Storybook demo for the LabelledProgress widget.

use egui_widgets::labelled_progress::{LabelledProgress, ProgressState};
use egui_widgets::theme::TextSize;

use crate::{accent, bg, muted, secondary};

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("LabelledProgress Widget")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "A busy mark the size of the words beside it. For work whose \
             remaining time is not knowable — which is most chain work. Use \
             progress_bar when there IS a fraction.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(12.0);

    ui.horizontal_top(|ui| {
        panel(
            ui,
            "Every state",
            "The mark changes; the box it fills does not, so a step advancing \
             does not shift the words beside it.",
            |ui| {
                LabelledProgress::new("Waiting for an earlier step")
                    .state(ProgressState::Waiting)
                    .show(ui);
                LabelledProgress::new("Waiting for a block").show(ui);
                LabelledProgress::new("Submitted")
                    .state(ProgressState::Done)
                    .show(ui);
                LabelledProgress::new("Rejected by the node")
                    .state(ProgressState::Failed)
                    .show(ui);
            },
        );
        panel(
            ui,
            "Every type step",
            "The mark is sized from the LABEL's line height, so it matches at \
             any step without a number to tune. This is the whole widget.",
            |ui| {
                for (size, name) in [
                    (TextSize::Xs, "Xs"),
                    (TextSize::Sm, "Sm"),
                    (TextSize::Base, "Base"),
                    (TextSize::Md, "Md"),
                    (TextSize::Lg, "Lg"),
                    (TextSize::Xl, "Xl"),
                ] {
                    LabelledProgress::new(name).size(size).show(ui);
                }
            },
        );
    });

    ui.add_space(12.0);
    ui.label(
        egui::RichText::new(
            "For comparison, egui's own Spinner at the same nominal sizes. \
             Unsized it takes interact_size.y — floored at the 44pt tap target \
             under touch sizing; sized it draws radius = height/2 - 2, so a \
             request for 12 yields a circle of 8. Both are its arithmetic, not \
             the caller's, which is why every call site got it wrong.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(8.0);
    egui::Frame::new()
        .fill(bg(ui))
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui_widgets::theme::hairline(crate::highlight(ui)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label(
                    egui::RichText::new("egui::Spinner, unsized")
                        .color(muted(ui))
                        .size(13.0),
                );
            });
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(13.0));
                ui.label(
                    egui::RichText::new("egui::Spinner, .size(13.0)")
                        .color(muted(ui))
                        .size(13.0),
                );
            });
            ui.horizontal(|ui| {
                LabelledProgress::new("LabelledProgress, same text")
                    .size(TextSize::Md)
                    .show(ui);
            });
        });
}

fn panel(ui: &mut egui::Ui, title: &str, note: &str, body: impl FnOnce(&mut egui::Ui)) {
    // Explicit top-down: `allocate_ui` inherits the parent's layout, and these
    // sit inside a `horizontal_top`.
    ui.allocate_ui_with_layout(
        egui::vec2(330.0, ui.available_height()),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            egui::Frame::new()
                .fill(bg(ui))
                .corner_radius(6.0)
                .inner_margin(12.0)
                .stroke(egui_widgets::theme::hairline(crate::highlight(ui)))
                .show(ui, |ui| {
                    ui.set_min_width(330.0);
                    ui.label(
                        egui::RichText::new(title)
                            .color(secondary(ui))
                            .size(11.0)
                            .strong(),
                    );
                    ui.label(egui::RichText::new(note).color(muted(ui)).size(10.0));
                    ui.add_space(8.0);
                    body(ui);
                });
        },
    );
}
