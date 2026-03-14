use crate::{ACCENT, TEXT_MUTED};

/// Mutable state for the interactive progress bar demo.
pub struct ProgressBarState {
    pub fraction: f32,
    pub countdown_elapsed: f64,
    pub countdown_total: f64,
    pub animating: bool,
}

impl Default for ProgressBarState {
    fn default() -> Self {
        Self {
            fraction: 0.65,
            countdown_elapsed: 42_000.0,
            countdown_total: 86_400.0,
            animating: false,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut ProgressBarState) {
    // --- Basic progress bar ---
    ui.label(
        egui::RichText::new("Basic Progress Bar")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("Drag the slider to change progress")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

    ui.add(egui::Slider::new(&mut state.fraction, 0.0..=1.0).text("fraction"));
    ui.add_space(8.0);

    egui_widgets::ProgressBar::new(state.fraction)
        .label("Upload Progress")
        .detail(format!("{}%", (state.fraction * 100.0).round() as u32))
        .show(ui);

    ui.add_space(16.0);

    // --- With percentage overlay ---
    ui.label(
        egui::RichText::new("With Percentage Overlay")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);

    egui_widgets::ProgressBar::new(state.fraction)
        .show_percentage()
        .height(20.0)
        .show(ui);

    ui.add_space(16.0);

    // --- Countdown style ---
    ui.label(
        egui::RichText::new("Countdown Timer")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("Simulates a buff expiry countdown")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

    if ui
        .button(if state.animating { "Pause" } else { "Animate" })
        .clicked()
    {
        state.animating = !state.animating;
    }
    if ui.button("Reset").clicked() {
        state.countdown_elapsed = 0.0;
        state.animating = false;
    }

    if state.animating {
        state.countdown_elapsed += ui.input(|i| i.stable_dt) as f64 * 3600.0; // 1hr per real second
        if state.countdown_elapsed >= state.countdown_total {
            state.countdown_elapsed = state.countdown_total;
            state.animating = false;
        }
        ui.ctx().request_repaint();
    }

    let remaining_secs = (state.countdown_total - state.countdown_elapsed).max(0.0);
    let remaining_text = egui_widgets::format_duration(remaining_secs as u64);

    egui_widgets::ProgressBar::countdown(state.countdown_elapsed, state.countdown_total)
        .label("speed_boost")
        .detail(format!("{remaining_text} remaining"))
        .fill_color(egui_widgets::theme::WARNING)
        .height(14.0)
        .show(ui);

    ui.add_space(16.0);

    // --- Color variants ---
    ui.label(egui::RichText::new("Color Variants").color(ACCENT).strong());
    ui.add_space(4.0);

    egui_widgets::ProgressBar::new(0.85)
        .label("Health")
        .fill_color(egui_widgets::theme::SUCCESS)
        .height(12.0)
        .show(ui);
    ui.add_space(4.0);

    egui_widgets::ProgressBar::new(0.35)
        .label("Shield")
        .fill_color(egui_widgets::theme::ACCENT)
        .height(12.0)
        .show(ui);
    ui.add_space(4.0);

    egui_widgets::ProgressBar::new(0.12)
        .label("Danger")
        .fill_color(egui_widgets::theme::ERROR)
        .height(12.0)
        .show(ui);

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(egui::RichText::new("Test cases:").color(ACCENT).strong());
    ui.label("\u{2022} Drag slider \u{2192} bar updates in real time");
    ui.label("\u{2022} Hover bar \u{2192} tooltip shows percentage");
    ui.label("\u{2022} Countdown: animate \u{2192} fills from right to left");
    ui.label("\u{2022} At 0% and 100% \u{2192} corners render correctly");
}
