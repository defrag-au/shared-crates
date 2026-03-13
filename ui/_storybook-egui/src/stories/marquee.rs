use crate::{ACCENT, TEXT_MUTED};

const BUY_COLOR: egui::Color32 = egui::Color32::from_rgb(68, 255, 68);
const SELL_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 68, 68);
const STATUS_COLOR: egui::Color32 = egui::Color32::from_rgb(68, 180, 255);

pub fn show(
    ui: &mut egui::Ui,
    marquee: &mut egui_widgets::Marquee,
    messages: &mut Vec<egui_widgets::MarqueeItem>,
) {
    // Controls
    ui.horizontal(|ui| {
        if ui.button("Add Landing").clicked() {
            messages.push(egui_widgets::MarqueeItem {
                text: format!(
                    "$alien{} LANDED \u{2014} 1.2K for 51 ADA",
                    messages.len() + 1
                ),
                color: BUY_COLOR,
            });
        }
        if ui.button("Add Departure").clicked() {
            messages.push(egui_widgets::MarqueeItem {
                text: format!(
                    "addr1q...x{} DEPARTED \u{2014} 800 for 35 ADA",
                    messages.len() + 1
                ),
                color: SELL_COLOR,
            });
        }
        if ui.button("Add Status").clicked() {
            messages.push(egui_widgets::MarqueeItem {
                text: "TRANSMISSION ESTABLISHED".into(),
                color: STATUS_COLOR,
            });
        }
        if ui.button("Clear All").clicked() {
            messages.clear();
        }
    });

    ui.label(
        egui::RichText::new(format!("{} messages", messages.len()))
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(8.0);

    // Marquee in a dark frame (simulating the status bar panel)
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(10, 10, 26))
        .inner_margin(egui::Margin::symmetric(8, 2))
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            marquee.show(ui, messages);
        });

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(egui::RichText::new("Test cases:").color(ACCENT).strong());
    ui.label("\u{2022} Single short message \u{2192} centers statically, no scroll");
    ui.label("\u{2022} Many messages \u{2192} smooth continuous scroll");
    ui.label("\u{2022} Add message during scroll \u{2192} no position jump");
    ui.label("\u{2022} Clear all \u{2192} marquee disappears");
}
