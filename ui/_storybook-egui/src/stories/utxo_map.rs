//! Storybook demo for the `UtxoMap` widget — a Voronoi terrain map of a wallet.
//!
//! This story did not exist until the colour migration went looking for one.
//! `UtxoMap` had a config field documented as "cyan at low opacity" that was
//! built with `from_rgba_premultiplied`, so it actually blended additively,
//! **overflowed the blue channel and clipped** — the sea rendered brighter than
//! the token it was derived from, the exact opposite of the comment. Nobody
//! noticed for as long as the widget shipped, because there was nothing to look
//! at. A widget with no story is a widget whose appearance is unfalsifiable.
//!
//! So the story is built around the two things a reader must be able to judge:
//!
//! - **land vs water** — free ADA against locked. If the sea stops reading as
//!   sea, this is where it shows.
//! - **territories** — policy colours come from the theme's `IdentityEnvelope`,
//!   which fixes lightness and varies only hue, so the *theme* decides whether a
//!   policy clears the surface rather than the policy's name deciding for it.
//!   Switch themes with many policies on screen and every territory should stay
//!   separable.

use egui_widgets::slider_group::{Fader, SliderGroup};
use egui_widgets::utxo_map::{UtxoCell, UtxoMapConfig, UtxoMapData, UtxoMapState};

use crate::{accent, muted};

// ============================================================================
// State
// ============================================================================

pub struct UtxoMapStoryState {
    pub map: UtxoMapState,
    pub preset: usize,
    pub size: f32,
    /// Last hover/click reported, so the story shows the widget's output too.
    pub last_action: String,
}

impl Default for UtxoMapStoryState {
    fn default() -> Self {
        Self {
            map: UtxoMapState::default(),
            preset: 0,
            size: 360.0,
            last_action: "—".into(),
        }
    }
}

// ============================================================================
// Demo data
// ============================================================================

const PRESET_NAMES: [&str; 5] = [
    "Collector",
    "Mostly ADA",
    "Fully locked",
    "Many policies",
    "Single UTxO",
];

/// A policy id that looks like one — 56 hex chars, derived from a seed so the
/// identity envelope has something realistic to hash.
fn policy(seed: u8) -> String {
    (0..56)
        .map(|i| {
            let n = (seed as usize).wrapping_mul(31).wrapping_add(i * 7) % 16;
            std::char::from_digit(n as u32, 16).unwrap()
        })
        .collect()
}

fn cell(utxo: usize, pol: Option<u8>, assets: u64, lovelace: u64) -> UtxoCell {
    UtxoCell {
        utxo_ref: format!("{:0>64}#{utxo}", utxo * 977),
        policy_id: pol.map(policy).unwrap_or_default(),
        asset_count: assets,
        lovelace_share: lovelace,
    }
}

/// A wallet with a handful of collections and some loose ADA — the ordinary case.
fn preset_collector() -> UtxoMapData {
    let mut cells = Vec::new();
    for (u, (pol, n, lv)) in [
        (Some(1), 4, 2_400_000),
        (Some(1), 2, 1_800_000),
        (Some(2), 7, 3_100_000),
        (Some(2), 1, 1_500_000),
        (Some(3), 3, 2_000_000),
        (None, 0, 14_000_000),
        (None, 0, 9_500_000),
    ]
    .into_iter()
    .enumerate()
    {
        cells.push(cell(u, pol, n, lv));
    }
    let locked = 2_400_000 + 1_800_000 + 3_100_000 + 1_500_000 + 2_000_000;
    UtxoMapData {
        total_lovelace: locked + 14_000_000 + 9_500_000,
        locked_lovelace: locked,
        cells,
    }
}

/// Almost all free ADA — the map should read as sea with a couple of islands.
fn preset_mostly_ada() -> UtxoMapData {
    let cells = vec![
        cell(0, Some(1), 1, 1_600_000),
        cell(1, None, 0, 40_000_000),
        cell(2, None, 0, 26_000_000),
        cell(3, None, 0, 18_000_000),
    ];
    UtxoMapData {
        total_lovelace: 1_600_000 + 40_000_000 + 26_000_000 + 18_000_000,
        locked_lovelace: 1_600_000,
        cells,
    }
}

/// No free ADA at all — the opposite extreme, and the one that would hide a
/// broken water colour completely.
fn preset_fully_locked() -> UtxoMapData {
    let cells: Vec<UtxoCell> = (0..8)
        .map(|i| cell(i, Some((i % 4) as u8 + 1), (i as u64 % 5) + 1, 2_200_000))
        .collect();
    UtxoMapData {
        total_lovelace: 8 * 2_200_000,
        locked_lovelace: 8 * 2_200_000,
        cells,
    }
}

/// Enough distinct policies to stress the identity envelope — this is the
/// preset to switch themes on.
fn preset_many_policies() -> UtxoMapData {
    let cells: Vec<UtxoCell> = (0..18)
        .map(|i| cell(i, Some(i as u8 + 1), (i as u64 % 6) + 1, 1_300_000))
        .chain((0..3).map(|i| cell(100 + i, None, 0, 6_000_000)))
        .collect();
    let locked = 18 * 1_300_000;
    UtxoMapData {
        total_lovelace: locked + 3 * 6_000_000,
        locked_lovelace: locked,
        cells,
    }
}

/// The degenerate case: one cell. Voronoi over a single point has no edges, and
/// the widget must still draw something rather than dividing by zero.
fn preset_single() -> UtxoMapData {
    UtxoMapData {
        cells: vec![cell(0, Some(9), 3, 5_000_000)],
        total_lovelace: 5_000_000,
        locked_lovelace: 5_000_000,
    }
}

fn preset_data(i: usize) -> UtxoMapData {
    match i {
        1 => preset_mostly_ada(),
        2 => preset_fully_locked(),
        3 => preset_many_policies(),
        4 => preset_single(),
        _ => preset_collector(),
    }
}

// ============================================================================
// Story
// ============================================================================

pub fn show(ui: &mut egui::Ui, state: &mut UtxoMapStoryState) {
    ui.label(
        egui::RichText::new(
            "Each cell is one (UTxO, policy) pair. Land is ADA locked in \
             asset-bearing UTxOs, water is free ADA. Territories are policies, \
             coloured by the theme's identity envelope — fixed lightness, hue \
             varies — so switching theme must never leave a policy unreadable.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Preset:").color(muted(ui)).small());
        for (i, name) in PRESET_NAMES.iter().enumerate() {
            let text = if state.preset == i {
                egui::RichText::new(*name).color(accent(ui)).strong()
            } else {
                egui::RichText::new(*name).color(muted(ui))
            };
            if ui.selectable_label(state.preset == i, text).clicked() {
                state.preset = i;
            }
        }
    });
    crate::controls(ui, |ui| {
        SliderGroup::new()
            .fader(Fader::new("Size", &mut state.size, 200.0..=520.0).suffix("px"))
            .show(ui);
    });

    ui.add_space(8.0);

    let data = preset_data(state.preset);
    let free = data.total_lovelace.saturating_sub(data.locked_lovelace);
    let pct = |v: u64| {
        if data.total_lovelace == 0 {
            0.0
        } else {
            v as f64 / data.total_lovelace as f64 * 100.0
        }
    };

    let config = UtxoMapConfig {
        size: egui::Vec2::splat(state.size),
        ..Default::default()
    };
    let resp = config.show(ui, &data, &mut state.map);
    if let Some(action) = resp.action {
        state.last_action = format!("{action:?}");
    }

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(format!(
            "{} cells · land {:.0}% ({:.1} ADA locked) · water {:.0}% ({:.1} ADA free)",
            data.cells.len(),
            pct(data.locked_lovelace),
            data.locked_lovelace as f64 / 1e6,
            pct(free),
            free as f64 / 1e6,
        ))
        .color(muted(ui))
        .small(),
    );
    ui.label(
        egui::RichText::new(format!("last action: {}", state.last_action))
            .color(muted(ui))
            .small(),
    );

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new("What to check:")
            .color(accent(ui))
            .strong(),
    );
    for line in [
        "'Mostly ADA' — the sea must read as sea, not as the panel background.",
        "'Fully locked' — no water at all; nothing should look missing.",
        "'Many policies' — switch theme here: every territory stays separable.",
        "'Single UTxO' — one cell, no Voronoi edges, still draws.",
        "Hover a land cell for its UTxO; the reported action appears above.",
    ] {
        ui.label(
            egui::RichText::new(format!("  • {line}"))
                .color(muted(ui))
                .small(),
        );
    }
}
