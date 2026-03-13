use crate::ACCENT;

/// Real distribution data from Aliens snapshot.
fn aliens_bands() -> Vec<egui_widgets::DistBand> {
    vec![
        egui_widgets::DistBand {
            label: "Circulating".into(),
            value: 707_700_000.0,
            color: egui::Color32::from_rgb(68, 255, 68),
        },
        egui_widgets::DistBand {
            label: "Mothership".into(),
            value: 250_400_000.0,
            color: egui::Color32::from_rgb(0, 180, 200),
        },
        egui_widgets::DistBand {
            label: "Vested".into(),
            value: 31_400_000.0,
            color: egui::Color32::from_rgb(34, 180, 34),
        },
        egui_widgets::DistBand {
            label: "Treasury".into(),
            value: 3_500_000.0,
            color: egui::Color32::from_rgb(255, 180, 40),
        },
        egui_widgets::DistBand {
            label: "Burned".into(),
            value: 930_100.0,
            color: egui::Color32::from_rgb(60, 60, 70),
        },
    ]
}

pub fn show(ui: &mut egui::Ui, chart: &mut egui_widgets::DistributionChart) {
    ui.label(
        egui::RichText::new("Real Aliens distribution data")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("Click chart to toggle aligned / cascading arcs")
            .color(crate::TEXT_MUTED)
            .small(),
    );
    ui.add_space(8.0);

    // Chart in a dark frame (simulating the side panel)
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(15, 25, 20))
        .inner_margin(12.0)
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.set_width(200.0); // Match the aliens side panel width

            let bands = aliens_bands();

            let chart_radius = (ui.available_width() / 2.0).min(80.0);
            ui.vertical_centered(|ui| {
                chart
                    .radius(chart_radius)
                    .ring_thickness(6.0)
                    .ring_gap(4.0)
                    .show(ui, &bands);
            });

            ui.add_space(8.0);

            for band in &bands {
                if band.value > 0.0 {
                    egui_widgets::legend_row(
                        ui,
                        band.color,
                        &band.label,
                        &egui_widgets::format_chart_value(band.value),
                    );
                }
            }
        });

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(egui::RichText::new("Test cases:").color(ACCENT).strong());
    ui.label("\u{2022} Outermost ring = Circulating (largest, ~71%)");
    ui.label("\u{2022} Innermost ring = Burned (smallest, ~0.1%)");
    ui.label("\u{2022} Click chart \u{2192} animated toggle between aligned and cascading");
    ui.label("\u{2022} Hover any ring arc \u{2192} tooltip with label, amount, %");
    ui.label("\u{2022} Faint orbit track visible behind each arc");
}
