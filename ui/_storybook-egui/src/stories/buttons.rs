use crate::{accent, bg, muted};
use egui_widgets::UiButtonExt;

pub fn show(ui: &mut egui::Ui) {
    ui.label("Hover over buttons to see the cursor change.");
    ui.add_space(12.0);

    ui.label(
        egui::RichText::new("add_clickable (pointer cursor)")
            .color(accent(ui))
            .strong(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_clickable(egui::Button::new("Default"));
        ui.add_clickable(
            egui::Button::new(egui::RichText::new("Accent").color(bg(ui)).strong())
                .fill(accent(ui))
                .corner_radius(4.0),
        );
        ui.add_clickable(egui::Button::new("Outlined").frame(true));
    });

    ui.add_space(16.0);

    ui.label(
        egui::RichText::new("add_clickable_sized")
            .color(accent(ui))
            .strong(),
    );
    ui.add_space(4.0);
    ui.add_clickable_sized(
        [200.0, 40.0],
        egui::Button::new(egui::RichText::new("200 x 40 Sized").color(bg(ui)).strong())
            .fill(accent(ui))
            .corner_radius(6.0),
    );

    ui.add_space(16.0);

    ui.label(
        egui::RichText::new("Normal ui.add (default cursor)")
            .color(muted(ui))
            .strong(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add(egui::Button::new("No pointer"));
        ui.add(egui::Button::new("Also no pointer"));
    });

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new("The UiButtonExt trait adds set_cursor_icon(PointingHand) on hover.")
            .color(muted(ui))
            .small(),
    );
}
