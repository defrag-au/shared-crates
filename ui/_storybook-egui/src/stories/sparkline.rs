use crate::{ACCENT, TEXT_MUTED};

/// Mutable state for the sparkline demo.
pub struct SparklineState {
    pub data: Vec<f64>,
    pub tick: u32,
    pub animating: bool,
}

impl Default for SparklineState {
    fn default() -> Self {
        // Simulated accrual curve — starts slow, buffs kick in mid-way
        let data = vec![
            0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0, 50.0, 60.0, 72.0, 86.0,
            102.0, 120.0, 140.0, 162.0, 186.0, 212.0, 240.0, 260.0, 275.0, 288.0, 300.0,
        ];
        Self {
            data,
            tick: 0,
            animating: false,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut SparklineState) {
    // --- Basic sparkline ---
    ui.label(
        egui::RichText::new("Basic Sparkline")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("Hover to inspect individual data points")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(8.0);

    egui_widgets::Sparkline::new(&state.data)
        .label("Accrual History")
        .value_text(format!("{:.0} pts", state.data.last().unwrap_or(&0.0)))
        .height(50.0)
        .show(ui);

    ui.add_space(16.0);

    // --- With fill gradient ---
    ui.label(
        egui::RichText::new("With Fill Gradient")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);

    egui_widgets::Sparkline::new(&state.data)
        .label("Token Earnings")
        .value_text("300.0/hr")
        .height(60.0)
        .line_color(egui_widgets::theme::SUCCESS)
        .fill(egui::Color32::from_rgba_premultiplied(158, 206, 106, 30))
        .show_mean_line()
        .show(ui);

    ui.add_space(16.0);

    // --- Live animated sparkline ---
    ui.label(egui::RichText::new("Live Animation").color(ACCENT).strong());
    ui.label(
        egui::RichText::new("Simulates real-time accrual data")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        if ui
            .button(if state.animating { "Pause" } else { "Start" })
            .clicked()
        {
            state.animating = !state.animating;
        }
        if ui.button("Reset").clicked() {
            state.data = vec![0.0];
            state.tick = 0;
            state.animating = false;
        }
        if ui.button("Add Spike").clicked() {
            if let Some(last) = state.data.last() {
                state.data.push(last + 50.0);
            }
        }
    });

    if state.animating {
        state.tick += 1;
        if state.tick.is_multiple_of(6) {
            let last = state.data.last().copied().unwrap_or(0.0);
            // Simulate natural growth with some noise
            let noise = ((state.tick as f64 * 0.7).sin() * 3.0) + 1.0;
            let growth = 5.0 + noise;
            state.data.push(last + growth);
            // Keep last 50 points
            if state.data.len() > 50 {
                state.data.remove(0);
            }
        }
        ui.ctx().request_repaint();
    }

    ui.add_space(4.0);
    egui_widgets::Sparkline::new(&state.data)
        .height(40.0)
        .line_color(egui_widgets::theme::ACCENT_CYAN)
        .fill(egui::Color32::from_rgba_premultiplied(125, 207, 255, 20))
        .line_width(2.0)
        .show(ui);

    ui.add_space(16.0);

    // --- Flat line + minimal data ---
    ui.label(egui::RichText::new("Edge Cases").color(ACCENT).strong());
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Flat line:").color(TEXT_MUTED).small());
            egui_widgets::Sparkline::new(&[10.0, 10.0, 10.0, 10.0, 10.0])
                .height(30.0)
                .width(120.0)
                .show(ui);
        });
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("Single point:")
                    .color(TEXT_MUTED)
                    .small(),
            );
            egui_widgets::Sparkline::new(&[42.0])
                .height(30.0)
                .width(120.0)
                .show(ui);
        });
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Empty:").color(TEXT_MUTED).small());
            egui_widgets::Sparkline::new(&[])
                .height(30.0)
                .width(120.0)
                .show(ui);
        });
    });

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(egui::RichText::new("Test cases:").color(ACCENT).strong());
    ui.label("\u{2022} Hover \u{2192} crosshair + value tooltip at nearest point");
    ui.label("\u{2022} Mean line shown as dashed horizontal reference");
    ui.label("\u{2022} Fill gradient fades from line color to transparent at bottom");
    ui.label("\u{2022} Live mode: data scrolls, endpoint dot tracks latest value");
    ui.label("\u{2022} Spike button: sudden jump renders correctly");
    ui.label("\u{2022} Edge cases: flat line, single point, empty data");
}
