//! Phosphor icon gallery story — displays all available icons in a grid.

use egui::{Color32, RichText, Vec2};
use egui_widgets::icons::PhosphorIcon;

pub struct IconGalleryState {
    pub icon_size: f32,
    pub icon_color: [f32; 3],
}

impl Default for IconGalleryState {
    fn default() -> Self {
        Self {
            icon_size: 28.0,
            icon_color: [0.86, 0.86, 0.92],
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut IconGalleryState) {
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.icon_size, 12.0..=64.0).text("Size"));
        ui.color_edit_button_rgb(&mut state.icon_color);
    });
    ui.add_space(8.0);

    let color = Color32::from_rgb(
        (state.icon_color[0] * 255.0) as u8,
        (state.icon_color[1] * 255.0) as u8,
        (state.icon_color[2] * 255.0) as u8,
    );

    let col_width = state.icon_size + 80.0;
    let available = ui.available_width();
    let cols = ((available / col_width) as usize).max(1);

    egui::Grid::new("icon_grid")
        .num_columns(cols)
        .spacing(Vec2::new(12.0, 8.0))
        .show(ui, |ui| {
            for (i, icon) in PhosphorIcon::ALL.iter().enumerate() {
                ui.horizontal(|ui| {
                    icon.show(ui, state.icon_size, color);
                    ui.label(
                        RichText::new(icon.name())
                            .color(Color32::from_rgb(160, 160, 180))
                            .size(11.0),
                    );
                });
                if (i + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    // Demo: icons in context
    ui.heading("In Context");
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        PhosphorIcon::Sword.show(ui, 20.0, Color32::from_rgb(255, 100, 100));
        ui.label("ATK 5");
        ui.add_space(12.0);
        PhosphorIcon::Shield.show(ui, 20.0, Color32::from_rgb(100, 150, 255));
        ui.label("DEF 3");
        ui.add_space(12.0);
        PhosphorIcon::Heart.show(ui, 20.0, Color32::from_rgb(255, 80, 120));
        ui.label("HP 12");
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        PhosphorIcon::Lightning.show(ui, 20.0, Color32::from_rgb(255, 220, 50));
        ui.label("Speed 7");
        ui.add_space(12.0);
        PhosphorIcon::Coins.show(ui, 20.0, Color32::from_rgb(255, 200, 50));
        ui.label("1,250 Gold");
        ui.add_space(12.0);
        PhosphorIcon::Star.show(ui, 20.0, Color32::from_rgb(255, 200, 50));
        ui.label("Rare");
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        PhosphorIcon::Fire.show(ui, 20.0, Color32::from_rgb(255, 120, 30));
        ui.label("Burn Effect");
        ui.add_space(12.0);
        PhosphorIcon::Skull.show(ui, 20.0, Color32::from_rgb(180, 50, 50));
        ui.label("Defeated");
        ui.add_space(12.0);
        PhosphorIcon::Flag.show(ui, 20.0, Color32::from_rgb(50, 200, 100));
        ui.label("Captured");
    });
}
