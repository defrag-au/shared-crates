//! Storybook demo for the TraitFilter widget from egui-widgets.

use egui::Color32;
use egui_widgets::trait_filter::{self, FilterEntry, TraitFilterConfig, TraitFilterState};

use crate::{ACCENT, TEXT_MUTED};

// ============================================================================
// Mock data
// ============================================================================

/// Mock trait data mimicking a real NFT collection (Hodlcroft Pirates-style).
fn build_mock_entries() -> Vec<FilterEntry> {
    let categories: &[(&str, &[&str])] = &[
        (
            "Background",
            &[
                "Red", "Blue", "Green", "Purple", "Orange", "Teal", "Gold", "Black", "White",
                "Sunset", "Ocean", "Nebula", "Forest",
            ],
        ),
        (
            "Body",
            &[
                "Human", "Skeleton", "Zombie", "Robot", "Ghost", "Alien", "Vampire", "Werewolf",
            ],
        ),
        (
            "Eyes",
            &[
                "Normal",
                "Laser",
                "Cyclops",
                "Blind",
                "Glowing",
                "Red",
                "Heterochromia",
                "Visor",
            ],
        ),
        (
            "Faction",
            &["Tsuki", "Mac", "Void", "Solar", "Ember", "Frost"],
        ),
        (
            "Hat",
            &[
                "Pirate",
                "Crown",
                "Bandana",
                "Tricorn",
                "None",
                "Feathered",
                "Skull Cap",
                "Top Hat",
                "Straw Hat",
                "Viking Helm",
            ],
        ),
        (
            "Mouth",
            &[
                "Smile",
                "Frown",
                "Pipe",
                "Fangs",
                "Cigar",
                "Gold Teeth",
                "Mask",
            ],
        ),
        (
            "Necklace",
            &["None", "Gold Chain", "Pearl", "Anchor", "Ruby", "Compass"],
        ),
        (
            "Outfit",
            &[
                "Captain Coat",
                "Striped Shirt",
                "Bare Chest",
                "Admiral",
                "Red Cape",
                "Leather Vest",
                "Naval Uniform",
                "Rags",
            ],
        ),
        (
            "Shield",
            &["None", "Wooden", "Iron", "Gold", "Magic", "Skull"],
        ),
        (
            "Weapon",
            &[
                "Cutlass",
                "Pistol",
                "Cannon",
                "Hook",
                "Dagger",
                "Trident",
                "None",
                "Blunderbuss",
                "Rapier",
            ],
        ),
    ];

    let mut entries = Vec::new();
    let mut idx = 0;
    for (category, values) in categories {
        for value in *values {
            // Simulate ownership: ~60% owned
            let owned = idx % 5 != 0;
            let color = if *value == "None" {
                Some(Color32::from_rgba_premultiplied(60, 65, 80, 120))
            } else if owned {
                Some(Color32::from_rgba_premultiplied(158, 206, 106, 200))
            } else {
                Some(Color32::from_rgba_premultiplied(86, 95, 137, 160))
            };
            entries.push(FilterEntry {
                label: format!("{category}: {value}"),
                category: category.to_string(),
                value: value.to_string(),
                color,
            });
            idx += 1;
        }
    }
    entries
}

// ============================================================================
// State
// ============================================================================

pub struct TraitFilterStoryState {
    pub filter: TraitFilterState,
    pub entries: Vec<FilterEntry>,
}

impl Default for TraitFilterStoryState {
    fn default() -> Self {
        Self {
            filter: TraitFilterState::default(),
            entries: build_mock_entries(),
        }
    }
}

// ============================================================================
// Main show function
// ============================================================================

pub fn show(ui: &mut egui::Ui, state: &mut TraitFilterStoryState) {
    ui.label(
        egui::RichText::new("TraitFilter Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Compound-key prefix trie with dual indexing. \
             Type a category name (\"Back\") or value (\"Re\") to search.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(8.0);

    let config = TraitFilterConfig::default();
    let resp = trait_filter::show(ui, &mut state.filter, &state.entries, &config);

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    // Show active filters and match count
    if state.filter.selected.is_empty() {
        ui.label(
            egui::RichText::new(format!(
                "No filters active \u{2014} {} total entries",
                state.entries.len()
            ))
            .color(TEXT_MUTED)
            .size(11.0),
        );
    } else {
        // Show selected tags as debug info
        let tags: Vec<&str> = state
            .filter
            .selected
            .iter()
            .filter_map(|&idx| state.entries.get(idx).map(|e| e.label.as_str()))
            .collect();

        ui.label(
            egui::RichText::new(format!("Active filters ({}): AND logic", tags.len()))
                .color(ACCENT)
                .size(11.0)
                .strong(),
        );
        for tag in &tags {
            ui.label(
                egui::RichText::new(format!("  \u{2022} {tag}"))
                    .color(egui::Color32::from_rgb(158, 206, 106))
                    .size(10.0),
            );
        }

        // Simulate match count: count mock "assets" that have all selected traits
        // In a real app this would filter actual asset data
        let match_count = simulate_match_count(&state.entries, &state.filter.selected);
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!("{match_count} assets would match"))
                .color(TEXT_MUTED)
                .size(10.0),
        );
    }

    if resp.changed {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("\u{2192} Filter changed this frame")
                .color(egui::Color32::from_rgb(125, 207, 255))
                .size(9.0),
        );
    }

    ui.add_space(16.0);
    ui.label(
        egui::RichText::new("Try:")
            .color(ACCENT)
            .size(11.0)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "  \u{2022} \"back\" \u{2192} Background values\n  \
             \u{2022} \"re\" \u{2192} Red values across categories\n  \
             \u{2022} \"background: re\" \u{2192} narrows to Background + Re prefix\n  \
             \u{2022} Backspace on empty input removes last tag\n  \
             \u{2022} Up/Down + Enter for keyboard nav",
        )
        .color(TEXT_MUTED)
        .size(10.0),
    );
}

/// Simulate how many "assets" would match the selected filters.
/// Each filter selects a category:value. An asset matches if it has ALL selected traits.
/// We pretend there are 8888 assets and each trait has a random coverage.
fn simulate_match_count(entries: &[FilterEntry], selected: &[usize]) -> usize {
    if selected.is_empty() {
        return 8888;
    }
    // Simple simulation: each additional filter roughly halves the pool
    let mut count = 8888.0_f64;
    for &idx in selected {
        if idx < entries.len() {
            // Use a hash of the entry label to get a pseudo-random coverage fraction
            let hash = entries[idx]
                .label
                .bytes()
                .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
            let fraction = 0.05 + (hash % 30) as f64 / 100.0; // 5-35% coverage
            count *= fraction;
        }
    }
    (count as usize).max(0)
}
