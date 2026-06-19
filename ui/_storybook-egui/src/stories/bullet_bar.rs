//! `BulletBar` story — value fill against a track with a target marker.

use egui_widgets::bullet_bar::BulletBar;
use egui_widgets::theme;

pub struct BulletBarState {
    pub value: f32,
    pub target: f32,
}

impl Default for BulletBarState {
    fn default() -> Self {
        Self {
            value: 0.62,
            target: 0.70,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut BulletBarState) {
    ui.label(
        egui::RichText::new("Bullet Bar")
            .color(theme::ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "A value fill against a track with a vertical target marker — the \
             classic \"am I hitting target?\" measure. Use it for rarity \
             actual-vs-target, coverage, budgets, or progress-to-goal.",
        )
        .color(theme::TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // Interactive single bar.
    ui.add(egui::Slider::new(&mut state.value, 0.0..=1.0).text("value"));
    ui.add(egui::Slider::new(&mut state.target, 0.0..=1.0).text("target"));
    ui.add_space(10.0);
    BulletBar::new(state.value, state.target)
        .label("Coverage")
        .show_percent(true)
        .good_within(theme::SUCCESS, 0.02)
        .show(ui);
    ui.add_space(20.0);

    // Small-multiples: per-value rarity targets (fill = actual share, tick = target).
    ui.label(
        egui::RichText::new("Per-value targets (rarity)")
            .color(theme::ACCENT)
            .strong(),
    );
    ui.add_space(6.0);
    for (label, actual, target) in [
        ("Common", 0.55_f32, 0.50_f32),
        ("Rare", 0.30, 0.35),
        ("Legendary", 0.08, 0.05),
        ("Mythic", 0.02, 0.01),
    ] {
        BulletBar::new(actual, target)
            .label(label)
            .show_percent(true)
            .good_within(theme::SUCCESS, 0.02)
            .height(12.0)
            .show(ui);
        ui.add_space(8.0);
    }
}
