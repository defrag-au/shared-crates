use crate::{ACCENT, TEXT_MUTED};

/// Mutable state for the seven-segment demo.
pub struct SevenSegmentState {
    pub counter_value: u64,
    pub animating: bool,
    pub accumulated: f64,
}

impl Default for SevenSegmentState {
    fn default() -> Self {
        Self {
            counter_value: 12345,
            animating: false,
            accumulated: 0.0,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut SevenSegmentState) {
    // --- Basic display ---
    ui.label(
        egui::RichText::new("Integer Counter")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("Clean seven-segment digits for point counters and holder counts")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(8.0);

    egui_widgets::SevenSegmentDisplay::new("12345").show(ui);

    ui.add_space(16.0);

    // --- Time display ---
    ui.label(egui::RichText::new("Time Display").color(ACCENT).strong());
    ui.label(
        egui::RichText::new("Colon separator for countdown timers and durations")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

    egui_widgets::SevenSegmentDisplay::new("23:59:48")
        .color(egui_widgets::theme::ACCENT_CYAN)
        .digit_height(36.0)
        .show(ui);

    ui.add_space(16.0);

    // --- Color variants ---
    ui.label(egui::RichText::new("Color Variants").color(ACCENT).strong());
    ui.add_space(4.0);

    egui_widgets::SevenSegmentDisplay::new("1247")
        .color(egui_widgets::theme::SUCCESS)
        .digit_height(36.0)
        .show(ui);
    ui.add_space(8.0);

    egui_widgets::SevenSegmentDisplay::new("88888")
        .color(egui_widgets::theme::WARNING)
        .digit_height(36.0)
        .show(ui);
    ui.add_space(8.0);

    egui_widgets::SevenSegmentDisplay::new("-42")
        .color(egui_widgets::theme::ERROR)
        .digit_height(36.0)
        .show(ui);

    ui.add_space(16.0);

    // --- Animated counter ---
    ui.label(egui::RichText::new("Live Counter").color(ACCENT).strong());
    ui.label(
        egui::RichText::new("Simulates accrual ticking up at 5.2 per second")
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
            state.counter_value = 0;
            state.accumulated = 0.0;
            state.animating = false;
        }
    });

    if state.animating {
        state.accumulated += 5.2 * ui.input(|i| i.stable_dt) as f64;
        while state.accumulated >= 1.0 {
            state.counter_value += 1;
            state.accumulated -= 1.0;
        }
        ui.ctx().request_repaint();
    }

    ui.add_space(4.0);

    let display_text = format!("{}", state.counter_value);
    egui_widgets::SevenSegmentDisplay::new(&display_text)
        .color(egui_widgets::theme::ACCENT_CYAN)
        .digit_height(48.0)
        .show(ui);

    ui.add_space(16.0);

    // --- Large scoreboard ---
    ui.label(
        egui::RichText::new("Large Scoreboard")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);

    egui_widgets::SevenSegmentDisplay::new("1247")
        .color(egui_widgets::theme::SUCCESS)
        .digit_height(64.0)
        .show(ui);

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(egui::RichText::new("Test cases:").color(ACCENT).strong());
    ui.label("- Digits 0-9 render with correct segments lit");
    ui.label("- Colons render as two stacked square dots");
    ui.label("- Minus sign renders as middle segment only");
    ui.label("- Dim unlit segments visible for context");
    ui.label("- Animated counter ticks whole numbers smoothly");
}
