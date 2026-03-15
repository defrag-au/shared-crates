use crate::{ACCENT, TEXT_MUTED};

/// Mutable state for the flip counter demo.
pub struct FlipCounterState {
    pub counter: egui_widgets::FlipCounter,
    pub value: u64,
    pub animating: bool,
    pub accumulated: f64,

    pub time_counter: egui_widgets::FlipCounter,
    pub seconds: u64,
    pub time_accumulated: f64,
    pub time_running: bool,
}

impl Default for FlipCounterState {
    fn default() -> Self {
        let mut counter = egui_widgets::FlipCounter::new(6)
            .text_color(egui_widgets::theme::TEXT_PRIMARY)
            .card_height(60.0);
        counter.set_value("12345");

        let mut time_counter = egui_widgets::FlipCounter::new(8)
            .text_color(egui_widgets::theme::ACCENT_CYAN)
            .card_height(50.0);
        time_counter.set_value("00:00:00");

        Self {
            counter,
            value: 12345,
            animating: false,
            accumulated: 0.0,

            time_counter,
            seconds: 0,
            time_accumulated: 0.0,
            time_running: false,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut FlipCounterState) {
    // --- Points counter ---
    ui.label(egui::RichText::new("Points Counter").color(ACCENT).strong());
    ui.label(
        egui::RichText::new("Split-flap style counter with flip animation on digit change")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        if ui
            .button(if state.animating { "Pause" } else { "Start" })
            .clicked()
        {
            state.animating = !state.animating;
        }
        if ui.button("Reset").clicked() {
            state.value = 0;
            state.accumulated = 0.0;
            state.animating = false;
            state.counter.set_value("0");
        }
        if ui.button("+100").clicked() {
            state.value += 100;
            state.counter.set_value(&format!("{}", state.value));
        }
        if ui.button("+1000").clicked() {
            state.value += 1000;
            state.counter.set_value(&format!("{}", state.value));
        }
    });

    if state.animating {
        let dt = ui.input(|i| i.stable_dt).min(0.1) as f64;
        state.accumulated += 5.2 * dt;
        while state.accumulated >= 1.0 {
            state.value += 1;
            state.accumulated -= 1.0;
        }
        state.counter.set_value(&format!("{}", state.value));
        ui.ctx().request_repaint();
    }

    ui.add_space(4.0);
    state.counter.show(ui);

    ui.add_space(24.0);

    // --- Timer ---
    ui.label(
        egui::RichText::new("Countdown Timer")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("Clock-style display with HH:MM:SS format")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        if ui
            .button(if state.time_running { "Pause" } else { "Start" })
            .clicked()
        {
            state.time_running = !state.time_running;
        }
        if ui.button("Reset").clicked() {
            state.seconds = 0;
            state.time_accumulated = 0.0;
            state.time_running = false;
            state.time_counter.set_value("00:00:00");
        }
    });

    if state.time_running {
        let dt = ui.input(|i| i.stable_dt).min(0.1) as f64;
        state.time_accumulated += dt;
        while state.time_accumulated >= 1.0 {
            state.seconds += 1;
            state.time_accumulated -= 1.0;
        }
        let h = state.seconds / 3600;
        let m = (state.seconds % 3600) / 60;
        let s = state.seconds % 60;
        state
            .time_counter
            .set_value(&format!("{h:02}:{m:02}:{s:02}"));
        ui.ctx().request_repaint();
    }

    ui.add_space(4.0);
    state.time_counter.show(ui);

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(egui::RichText::new("Test cases:").color(ACCENT).strong());
    ui.label("- Digits flip with top-half-down then bottom-half-in animation");
    ui.label("- Cards show split line across the middle");
    ui.label("- Top half slightly lighter than bottom half");
    ui.label("- +100/+1000 triggers multiple digit flips simultaneously");
    ui.label("- Timer colons rendered as static separators");
}
