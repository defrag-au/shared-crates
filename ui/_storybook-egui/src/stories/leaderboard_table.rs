//! Storybook demo for the LeaderboardTable widget.

use egui_widgets::{ChipVariant, LeaderboardRow, LeaderboardTable};

use crate::{ACCENT, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("LeaderboardTable Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Dense, virtual-scrolled ranked table: rank, identity (accent for \
             handles), optional semantic badge, pre-formatted value, and share. \
             Used for token holders, top traders, wallet rankings.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    let rows = sample_rows();

    egui::Frame::new()
        .fill(crate::BG_MAIN)
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
        .show(ui, |ui| {
            // Bounded height so the virtual scroll is exercised in the story.
            ui.allocate_ui(egui::vec2(ui.available_width().min(620.0), 320.0), |ui| {
                LeaderboardTable::new(&rows)
                    .headers("Holder", "Amount")
                    .id_salt("leaderboard_story")
                    .show(ui);
            });
        });
}

fn sample_rows() -> Vec<LeaderboardRow> {
    let seed: &[(&str, bool, Option<(&str, ChipVariant)>, &str, f64)] = &[
        (
            "DEX Pool",
            false,
            Some(("LP", ChipVariant::Info)),
            "148.45M",
            32.60,
        ),
        ("stake1ux09\u{2026}c07f88", false, None, "146.39M", 32.15),
        ("$redbear781", true, None, "6.83M", 1.50),
        ("$westoz", true, None, "6.74M", 1.48),
        (
            "$vault290",
            true,
            Some(("STAKED", ChipVariant::Warning)),
            "5.85M",
            1.28,
        ),
        ("$curiousfutures", true, None, "5.37M", 1.18),
        ("$no.step.on.snek", true, None, "4.00M", 0.88),
        (
            "Vesting Contract",
            false,
            Some(("VEST", ChipVariant::Tag)),
            "3.50M",
            0.77,
        ),
        (
            "$jpb",
            true,
            Some(("STAKED", ChipVariant::Warning)),
            "3.11M",
            0.68,
        ),
        (
            "burn",
            false,
            Some(("BURN", ChipVariant::Danger)),
            "2.10M",
            0.46,
        ),
    ];

    // Pad out so the virtual scroll has plenty of rows.
    let mut rows: Vec<LeaderboardRow> = seed
        .iter()
        .enumerate()
        .map(
            |(i, (label, accent, badge, value, percent))| LeaderboardRow {
                rank: i + 1,
                label: (*label).to_string(),
                accent: *accent,
                badge: badge.map(|(l, v)| (l.to_string(), v)),
                value: (*value).to_string(),
                value_detail: badge
                    .filter(|(l, _)| *l == "LP")
                    .map(|_| "Free: 124.21M\nLP: 24.24M".to_string()),
                percent: *percent,
                copy_value: Some(format!("stake1uexample{:02}", i + 1)),
            },
        )
        .collect();

    for i in rows.len()..120 {
        rows.push(LeaderboardRow {
            rank: i + 1,
            label: format!("stake1u{i:02}\u{2026}xyz{i:02}"),
            accent: false,
            badge: None,
            value: format!("{}K", 900 - i * 5),
            value_detail: None,
            percent: (0.20 - i as f64 * 0.001).max(0.01),
            copy_value: Some(format!("stake1u{i:02}realaddrxyz{i:02}")),
        });
    }
    rows
}
