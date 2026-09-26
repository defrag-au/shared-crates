//! `RewardTiers` — what each role is paid, as a list: the role, how many hold
//! it, and the rewards its holders receive.
//!
//! The read-only half of a tiered rewards configuration: a role (or team) the
//! caller already resolved, a member count, and what a distribution rolls for
//! it. Editing is deliberately absent — the config a distribution reads is
//! authored and baked elsewhere, and a second editor over one policy is how the
//! two come to disagree.
//!
//! ## Three states, three renders
//!
//! The distinctions this widget exists to keep:
//!
//! - **A tier with a role** — the role's name beside its member count.
//! - **A tier whose role is not in the caller's mirror** (`role: None`) — says
//!   so, in place, and the count reads unknown rather than zero. "Nobody holds
//!   this" and "we cannot see who holds this" are different findings, and a
//!   console that renders both as `0 members` sends an operator looking for
//!   players who are already there.
//! - **A tier with no rewards** — an empty reward row, not a missing one, so a
//!   tier that pays nothing reads as a decision rather than as a load failure.
//!
//! ## Chance is shown only when it is not certain
//!
//! A reward line that always fires shows its range; one that fires sometimes
//! shows the chance as well. A table where every row says "100%" is a table
//! with a column nobody reads.
//!
//! Data-only inputs, like [`crate::stat_strip`]: the caller does the lookup and
//! hands in what it found.

use egui::{Color32, RichText, Sense, Ui, Vec2};

use crate::chip::{Chip, ChipVariant};
use crate::theme::{Radius, Space, SpaceExt, TextSize, ThemeExt};

/// One tier, as the widget draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct RewardTier {
    /// Display name — "Skeleton King's Sentinels", not an id.
    pub label: String,
    /// The tier's authored colour as `0xRRGGBB`, when it has one.
    pub color: Option<u32>,
    /// How many members hold this tier's role. `None` when the caller cannot
    /// say — see the module docs; it renders as unknown, never as zero.
    pub members: Option<u32>,
    /// The role this tier resolves to, as a name. `None` means the caller's
    /// role source has not seen it.
    pub role: Option<String>,
    /// What holding the tier pays.
    pub rewards: Vec<TierReward>,
    /// What holding the tier owes, when the config says.
    pub upkeep: Vec<TierUpkeep>,
}

/// One rollable reward line.
#[derive(Debug, Clone, PartialEq)]
pub struct TierReward {
    pub label: String,
    /// Inclusive range, in whole units. `min == max` renders as a single figure.
    pub min: u32,
    pub max: u32,
    /// 0.0–1.0. Values at or above 0.999 render without a percentage.
    pub chance: f32,
}

/// One upkeep requirement.
#[derive(Debug, Clone, PartialEq)]
pub struct TierUpkeep {
    pub label: String,
    pub amount: f64,
}

/// The list. See the module docs.
#[derive(Default)]
pub struct RewardTiers {
    tiers: Vec<RewardTier>,
    /// Shown when there are no tiers at all — the caller's words for why.
    empty_note: Option<String>,
    /// Draw the upkeep row. Off for a config that has no upkeep concept.
    show_upkeep: bool,
    id_salt: &'static str,
}

impl RewardTiers {
    pub fn new() -> Self {
        Self {
            tiers: Vec::new(),
            empty_note: None,
            show_upkeep: true,
            id_salt: "reward_tiers",
        }
    }

    pub fn push(mut self, tier: RewardTier) -> Self {
        self.tiers.push(tier);
        self
    }

    pub fn tiers(mut self, tiers: impl IntoIterator<Item = RewardTier>) -> Self {
        self.tiers = tiers.into_iter().collect();
        self
    }

    /// What to say when there is nothing to list. Without it an empty list
    /// renders as an empty list, which reads as a failure.
    pub fn empty_note(mut self, note: impl Into<String>) -> Self {
        self.empty_note = Some(note.into());
        self
    }

    pub fn show_upkeep(mut self, show: bool) -> Self {
        self.show_upkeep = show;
        self
    }

    /// Distinguishes two of these in one scope.
    pub fn id_salt(mut self, salt: &'static str) -> Self {
        self.id_salt = salt;
        self
    }

    pub fn show(self, ui: &mut Ui) {
        if self.tiers.is_empty() {
            if let Some(note) = &self.empty_note {
                ui.colored_label(ui.tokens().color.text_muted, note);
            }
            return;
        }
        for (index, tier) in self.tiers.iter().enumerate() {
            tier_row(ui, tier, self.show_upkeep, self.id_salt, index);
        }
    }
}

fn tier_row(
    ui: &mut Ui,
    tier: &RewardTier,
    show_upkeep: bool,
    id_salt: &str,
    index: usize,
) {
    ui.push_id((id_salt, index), |ui| {
        ui.horizontal_wrapped(|ui| {
            swatch(ui, tier.color);
            ui.label(
                RichText::new(&tier.label)
                    .strong()
                    .size(ui.text_size(TextSize::Base)),
            );
            match tier.members {
                Some(members) => {
                    ui.colored_label(
                        ui.tokens().color.text_primary,
                        format!("{members} member{}", pluralise(members)),
                    );
                }
                None => {
                    ui.colored_label(
                        ui.tokens().color.text_muted,
                        RichText::new("member count unknown").size(ui.text_size(TextSize::Sm)),
                    );
                }
            }
            match &tier.role {
                Some(role) => {
                    ui.colored_label(
                        ui.tokens().color.text_muted,
                        RichText::new(role).size(ui.text_size(TextSize::Sm)),
                    );
                }
                // Says the gap in place. The count beside it is unknown for the
                // same reason, and an operator can act on this.
                None => {
                    ui.colored_label(
                        ui.tokens().color.accent_yellow,
                        RichText::new("role not in the role source")
                            .size(ui.text_size(TextSize::Sm)),
                    );
                }
            }
        });

        ui.horizontal_wrapped(|ui| {
            // Indented past the swatch, so the rewards read as belonging to the
            // tier above them rather than as a peer row.
            ui.add_space(ui.space(Space::Xl2));
            for reward in &tier.rewards {
                Chip::new(&reward_text(reward))
                    .variant(ChipVariant::Muted)
                    .show(ui);
            }
            if show_upkeep {
                for upkeep in &tier.upkeep {
                    Chip::new(&format!("upkeep {} {}", trim_amount(upkeep.amount), upkeep.label))
                        .variant(ChipVariant::Warning)
                        .show(ui);
                }
            }
        });
        ui.gap(Space::Sm);
    });
}

/// The tier's colour, from the config that authored it.
fn swatch(ui: &mut Ui, color: Option<u32>) {
    // theme-exempt: a colour chip the caller supplies, not a theme size
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
    let color = color
        .map(|rgb| Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8))
        .unwrap_or(ui.tokens().color.text_muted);
    ui.painter()
        .rect_filled(rect, ui.tokens().corner(Radius::Sm), color);
}

/// `min`/`max` as one figure where they are equal, and the chance only when it
/// is not certain. See the module docs.
fn reward_text(reward: &TierReward) -> String {
    let amount = match reward.min == reward.max {
        true => format!("{}", reward.min),
        false => format!("{}–{}", reward.min, reward.max),
    };
    match reward.chance >= 0.999 {
        true => format!("{} {}", reward.label, amount),
        false => format!(
            "{} {} ({:.0}%)",
            reward.label,
            amount,
            reward.chance * 100.0
        ),
    }
}

/// "1 member", "0 members", "12 members" — a count that reads as English.
fn pluralise(count: u32) -> String {
    match count == 1 {
        true => String::new(),
        false => String::from("s"),
    }
}

/// Amounts as a reader expects them: no trailing zeros where the number is
/// whole, one decimal where it is not.
fn trim_amount(amount: f64) -> String {
    match (amount.fract()).abs() < f64::EPSILON {
        true => format!("{amount:.0}"),
        false => format!("{amount:.1}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reward(label: &str, min: u32, max: u32, chance: f32) -> TierReward {
        TierReward {
            label: label.to_string(),
            min,
            max,
            chance,
        }
    }

    #[test]
    fn a_certain_reward_shows_no_percentage() {
        assert_eq!(reward_text(&reward("rum", 2, 6, 1.0)), "rum 2–6");
        // The threshold is "certain enough to be noise", so 0.999 is the same
        // answer as 1.0 rather than a rounding artefact of its own.
        assert_eq!(reward_text(&reward("rum", 2, 6, 0.999)), "rum 2–6");
        assert_eq!(reward_text(&reward("rum", 2, 6, 0.5)), "rum 2–6 (50%)");
    }

    #[test]
    fn an_equal_range_collapses_to_one_figure() {
        assert_eq!(reward_text(&reward("silver", 1, 1, 1.0)), "silver 1");
    }

    /// The distinction the `Option`s exist for: an unknown count is not zero,
    /// and neither is an unmirrored role a colourless zero-member tier.
    #[test]
    fn unknown_is_representable_and_distinct_from_zero() {
        let known = RewardTier {
            label: "t".into(),
            color: None,
            members: Some(0),
            role: Some("Deck Dogs".into()),
            rewards: Vec::new(),
            upkeep: Vec::new(),
        };
        let unknown = RewardTier {
            members: None,
            role: None,
            ..known.clone()
        };
        assert_ne!(known.members, unknown.members);
        assert_ne!(known.role, unknown.role);
        assert_eq!(known.members, Some(0), "zero holders is a count");
        assert_eq!(unknown.members, None, "no source is not a count");
    }

    #[test]
    fn amounts_lose_their_trailing_zeros() {
        assert_eq!(trim_amount(22.0), "22");
        assert_eq!(trim_amount(4.25), "4.2");
    }

    #[test]
    fn pluralisation_is_english() {
        assert_eq!(format!("1{}", pluralise(1)), "1");
        assert_eq!(format!("0{}", pluralise(0)), "0s");
        assert_eq!(format!("12{}", pluralise(12)), "12s");
    }
}
