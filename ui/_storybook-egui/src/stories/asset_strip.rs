//! Storybook demo for the AssetStrip widget — horizontally stacked
//! asset thumbnails with progressive overlap.

use cardano_assets::AssetId;
use egui_widgets::asset_strip::{self, AssetStripConfig, AssetStripItem};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

const POLICY_ID: &str = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

const ALL_PIRATES: &[(&str, &str)] = &[
    ("5069726174653834", "Pirate84"),
    ("506972617465323733", "Pirate273"),
    ("50697261746531303430", "Pirate1040"),
    ("506972617465333830", "Pirate380"),
    ("506972617465313432", "Pirate142"),
    ("506972617465393336", "Pirate936"),
    ("50697261746531323736", "Pirate1276"),
    ("506972617465373835", "Pirate785"),
    ("50697261746531393133", "Pirate1913"),
    ("50697261746531363133", "Pirate1613"),
    ("506972617465333138", "Pirate318"),
    ("506972617465313839", "Pirate189"),
];

fn make_item(hex: &str, name: &str) -> AssetStripItem {
    AssetStripItem {
        asset_id: AssetId::new_unchecked(POLICY_ID.into(), hex.into()),
        display_name: name.into(),
    }
}

pub struct AssetStripStoryState {
    pub items: Vec<AssetStripItem>,
    pub last_action: String,
    next_add_idx: usize,
}

impl Default for AssetStripStoryState {
    fn default() -> Self {
        // Start with 4 items to show moderate overlap
        let items: Vec<AssetStripItem> = ALL_PIRATES[..4]
            .iter()
            .map(|(hex, name)| make_item(hex, name))
            .collect();
        Self {
            items,
            last_action: String::new(),
            next_add_idx: 4,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut AssetStripStoryState) {
    ui.label(
        egui::RichText::new("AssetStrip Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Horizontally stacked asset thumbnails with progressive overlap. \
             Hover to lift a card, click to remove. Add items to see overlap increase.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    // Controls
    ui.horizontal(|ui| {
        if ui.button("Add item").clicked() && state.next_add_idx < ALL_PIRATES.len() {
            let (hex, name) = ALL_PIRATES[state.next_add_idx];
            state.items.push(make_item(hex, name));
            state.next_add_idx += 1;
            state.last_action = format!("Added {name}");
        }
        if ui.button("Add 3").clicked() {
            for _ in 0..3 {
                if state.next_add_idx < ALL_PIRATES.len() {
                    let (hex, name) = ALL_PIRATES[state.next_add_idx];
                    state.items.push(make_item(hex, name));
                    state.next_add_idx += 1;
                }
            }
            state.last_action = "Added 3 items".into();
        }
        if ui.button("Reset").clicked() {
            *state = AssetStripStoryState::default();
        }
        ui.label(
            egui::RichText::new(format!("{} items", state.items.len()))
                .color(TEXT_MUTED)
                .size(10.0),
        );
    });

    ui.add_space(12.0);

    // Strip at default size (72px)
    egui::Frame::new()
        .fill(BG_MAIN)
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Default (72px)")
                    .color(TEXT_MUTED)
                    .size(10.0),
            );
            ui.add_space(4.0);

            let config = AssetStripConfig::default();
            let resp = asset_strip::show(ui, &state.items, &config);

            if let Some(idx) = resp.clicked {
                if idx < state.items.len() {
                    state.last_action = format!("Removed: {}", state.items[idx].display_name);
                    state.items.remove(idx);
                }
            }
        });

    ui.add_space(12.0);

    // Strip at smaller size (48px) in a narrow container
    egui::Frame::new()
        .fill(BG_MAIN)
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
        .show(ui, |ui| {
            ui.set_max_width(300.0);
            ui.label(
                egui::RichText::new("Small (48px) in 300px container")
                    .color(TEXT_MUTED)
                    .size(10.0),
            );
            ui.add_space(4.0);

            let config = AssetStripConfig {
                thumb_size: 48.0,
                min_visible: 14.0,
            };
            let resp = asset_strip::show(ui, &state.items, &config);

            if let Some(idx) = resp.clicked {
                if idx < state.items.len() {
                    state.last_action =
                        format!("Removed (small): {}", state.items[idx].display_name);
                    state.items.remove(idx);
                }
            }
        });

    ui.add_space(12.0);

    // Action log
    if state.last_action.is_empty() {
        ui.label(
            egui::RichText::new("Hover a card to lift it, click to remove")
                .color(TEXT_MUTED)
                .size(11.0),
        );
    } else {
        ui.label(
            egui::RichText::new(format!("Last action: {}", state.last_action))
                .color(egui_widgets::theme::ACCENT_CYAN)
                .size(11.0),
        );
    }
}
