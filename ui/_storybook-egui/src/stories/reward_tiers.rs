//! `RewardTiers` story — a tiered rewards config as a read-only list.
//!
//! The three states the widget exists to keep apart are all on this page: a
//! tier with a role and a count, a tier whose role the caller cannot resolve
//! (count unknown, *not* zero), and a tier that pays nothing at all.

use egui_widgets::reward_tiers::{RewardTier, RewardTiers, TierReward, TierUpkeep};

use crate::{accent, muted};

/// Tiers from the Black Flag rewards bake — real names, real ranges, real
/// colours, so the list is read at the width it will actually be read at.
fn black_flag_tiers() -> Vec<RewardTier> {
    vec![
        RewardTier {
            label: "Skeleton King's Sentinels".to_string(),
            color: Some(0x21f994),
            members: Some(6),
            role: Some("Sentinels".to_string()),
            rewards: vec![
                reward("chicken", 12, 20, 1.0),
                reward("wood", 10, 18, 1.0),
                reward("treasure", 1, 4, 1.0),
                reward("silver", 1, 7, 1.0),
            ],
            upkeep: vec![
                upkeep("crew requirements", 13.0),
                upkeep("ship maintenance", 22.0),
            ],
        },
        RewardTier {
            label: "Davy's Dominion".to_string(),
            color: Some(0xb34646),
            members: Some(11),
            role: Some("Dominion".to_string()),
            rewards: vec![
                reward("chicken", 10, 18, 1.0),
                // A range with a chance: the two ways a line can be
                // conditional, on one row.
                reward("treasure", 1, 3, 1.0),
                reward("silver", 1, 5, 0.5),
            ],
            upkeep: Vec::new(),
        },
        // The case that has no role to show. `members: None` is what keeps this
        // from rendering as a tier nobody holds.
        RewardTier {
            label: "Tidal Tyrants".to_string(),
            color: Some(0x54aa95),
            members: None,
            role: None,
            rewards: vec![reward("silver", 1, 2, 0.8), reward("treasure", 1, 2, 0.4)],
            upkeep: Vec::new(),
        },
        // A tier that pays nothing. Renders an empty reward row rather than
        // vanishing, so it reads as a decision.
        RewardTier {
            label: "Deckhand".to_string(),
            color: Some(0xe6ca67),
            members: Some(43),
            role: Some("Deckhand".to_string()),
            rewards: Vec::new(),
            upkeep: Vec::new(),
        },
    ]
}

fn reward(label: &str, min: u32, max: u32, chance: f32) -> TierReward {
    TierReward {
        label: label.to_string(),
        min,
        max,
        chance,
    }
}

fn upkeep(label: &str, amount: f64) -> TierUpkeep {
    TierUpkeep {
        label: label.to_string(),
        amount,
    }
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Reward Tiers").color(accent(ui)).strong());
    ui.label(
        egui::RichText::new(
            "What each role is paid, as a list: the role, how many hold it, and what a \
             distribution rolls for it. Read-only by design — the config a distribution \
             reads is baked elsewhere, and a second editor over one policy is how the two \
             come to disagree.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    ui.label(egui::RichText::new("A configured world").color(accent(ui)).strong());
    ui.add_space(6.0);
    RewardTiers::new()
        .tiers(black_flag_tiers())
        .show(ui);
    ui.add_space(16.0);

    // Upkeep is optional: a config with no upkeep concept should not grow an
    // empty column for it.
    ui.label(egui::RichText::new("Upkeep hidden").color(accent(ui)).strong());
    ui.label(
        egui::RichText::new("The same tiers with `show_upkeep(false)`.")
            .color(muted(ui))
            .small(),
    );
    ui.add_space(6.0);
    RewardTiers::new()
        .tiers(black_flag_tiers())
        .show_upkeep(false)
        .id_salt("reward_tiers_no_upkeep")
        .show(ui);
    ui.add_space(16.0);

    // Nothing to list is a state with a reason, not an empty region.
    ui.label(egui::RichText::new("Nothing configured").color(accent(ui)).strong());
    ui.add_space(6.0);
    RewardTiers::new()
        .empty_note("No tiers in this server's baked config.")
        .show(ui);
}
