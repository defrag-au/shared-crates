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
    ui.add_space(20.0);

    // The case the optional target exists for. Real figures from a project's
    // spending measured against its published commitments (Mekka S2,
    // 2026-08-30, shares of the external raise).
    ui.label(
        egui::RichText::new("Measured vs advertised — MONEY (share of the raise)")
            .color(theme::ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Both categories were published, so both carry a target tick.",
        )
        .color(theme::TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    for (label, actual, target, detail) in [
        (
            "Hashpower",
            0.303_f32,
            Some(0.80_f32),
            "16,205 ₳ confirmed to the desk",
        ),
        (
            "Ops · tools · contractors",
            0.279,
            Some(0.20),
            "14,926 ₳ across 5 wallets",
        ),
    ] {
        BulletBar::with_target(actual, target)
            .label(label)
            .detail(detail)
            .show_percent(true)
            .good_within(theme::SUCCESS, 0.02)
            .height(12.0)
            .show(ui);
        ui.add_space(8.0);
    }
    ui.add_space(12.0);

    // A SEPARATE group, on a separate axis. See the note below: putting an
    // in-kind distribution on the money axis renders it as an empty bar.
    ui.label(
        egui::RichText::new("SUPPLY (share of units minted) — never advertised")
            .color(theme::ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "No allocation was ever published for either of these, so neither carries \
             a marker. A tick at 0% would assert a promise of zero; the finding is \
             stronger than that — this sits outside the published terms entirely.\n\n\
             Note the axis change. Both rows are denominated in ASSETS, and on the \
             money axis above the founder's 105-asset take renders as an empty bar, \
             because their ADA is 12 ₳ of min-UTxO carrier. Mixing units in one bar \
             group produces exactly the misreading the two-unit model exists to \
             prevent — so they are separate groups, with the unit in the heading.",
        )
        .color(theme::TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    for (label, actual, detail) in [
        ("Founder — free transfers", 105.0_f32 / 1143.0, "105 of 1,143, no consideration"),
        ("Team-held supply", 148.0 / 1143.0, "148 of 1,143 minted"),
    ] {
        BulletBar::untargeted(actual)
            .label(label)
            .detail(detail)
            .show_percent(true)
            .height(12.0)
            .show(ui);
        ui.add_space(8.0);
    }
}
