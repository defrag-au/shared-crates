//! `ScriptCatalogue` — the compiled scripts in a build artifact, each with
//! whether it can go on chain and what is stopping it.
//!
//! A compiler blueprint (Aiken's `plutus.json`, and the same shape from any
//! toolchain that emits one) is a list of validators. Choosing what to deploy
//! from it is not a plain multi-select, because three facts about a row change
//! what the reader should do, and none of them is visible in a title:
//!
//! - **A script may not be finished.** A parameterised validator still carries
//!   its compile-time parameters. Its bytes are not the program that will ever
//!   run — applying the parameters changes the code, and therefore the hash,
//!   the address and the policy id. Offering it for deployment would park a
//!   reference nothing can use.
//! - **Several titles are often ONE script.** A toolchain emits one entry per
//!   handler, so `foo.mint`, `foo.spend` and a `foo.else` fallback are three
//!   names over one compiled program with one hash. They want one deployment
//!   between them, and a reader who ticks all three should be told so rather
//!   than charged three times.
//! - **It may already be deployed.** The same bytes parked twice cost the
//!   locked ADA twice and buy nothing.
//!
//! So the widget's job is to make the status the loudest thing in the row, and
//! to only let a reader select what is actually selectable.
//!
//! ## Status is a named decision, never a flag
//!
//! [`ScriptStatus`] says WHY a row is in the state it is, so call sites never
//! grow their own `is_deployable()` reading of a pile of booleans. A new
//! reason is a new variant, and every reader is told to handle it.
//!
//! ## Example
//!
//! ```ignore
//! let rows: Vec<ScriptRow> = resp.validators.iter().map(to_row).collect();
//! let action = ScriptCatalogue::new(&rows, &mut selected).show(ui);
//! if let ScriptCatalogueAction::SelectionChanged = action {
//!     // `selected` now holds the chosen titles
//! }
//! ```

use std::collections::BTreeSet;

use egui::{Color32, RichText, Ui};

use crate::chip::{Chip, ChipVariant};
use crate::id_pill::{IdPill, IdPillLayout};
use crate::theme::{ColorTokens, Radius, TextSize, ThemeExt};

/// Why a compiled script can or cannot be deployed.
///
/// A named decision rather than a set of booleans: the reader of a row needs
/// to know what to DO, and "not deployable" is three different situations with
/// three different remedies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptStatus {
    /// Finished, verified, and not yet on chain. The only selectable state.
    Deployable,
    /// Already on chain at this reference. Deploying again would lock the ADA
    /// a second time for identical bytes.
    Deployed { reference: String },
    /// Compile-time parameters are still unapplied, named here. The remedy is
    /// in the contracts repo, not this screen: apply them and rebuild.
    Unapplied { parameters: Vec<String> },
    /// The artifact itself is wrong — the bytes do not hash to the hash they
    /// declare, or the entry could not be read.
    Rejected { reason: String },
}

impl ScriptStatus {
    /// May a reader tick this row?
    pub fn selectable(&self) -> bool {
        matches!(self, ScriptStatus::Deployable)
    }

    /// The short word shown in the row's chip.
    pub fn label(&self) -> &'static str {
        match self {
            ScriptStatus::Deployable => "ready",
            ScriptStatus::Deployed { .. } => "deployed",
            ScriptStatus::Unapplied { .. } => "unapplied",
            ScriptStatus::Rejected { .. } => "rejected",
        }
    }

    fn variant(&self) -> ChipVariant {
        match self {
            ScriptStatus::Deployable => ChipVariant::Success,
            ScriptStatus::Deployed { .. } => ChipVariant::Info,
            ScriptStatus::Unapplied { .. } => ChipVariant::Warning,
            ScriptStatus::Rejected { .. } => ChipVariant::Danger,
        }
    }

    /// The sentence under the row saying what this status means for the
    /// reader, and what to do about it.
    pub fn explanation(&self) -> Option<String> {
        match self {
            ScriptStatus::Deployable => None,
            ScriptStatus::Deployed { reference } => {
                Some(format!("already on chain at {reference}"))
            }
            ScriptStatus::Unapplied { parameters } => Some(format!(
                "still expects {} — apply the parameters and rebuild; these bytes are not the \
                 script that would run, so their hash names nothing",
                parameters.join(", ")
            )),
            ScriptStatus::Rejected { reason } => Some(reason.clone()),
        }
    }
}

/// One compiled script in the artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptRow {
    /// The toolchain's name for it, e.g. `fuel.mint`.
    pub title: String,
    /// The hash these bytes produce. Used as a minting policy, this IS the
    /// policy id — same hash, different purpose.
    pub script_hash: String,
    /// Compiled size. Drives what the deployment will lock (roughly 4.3
    /// lovelace per byte of output, so ~8 ADA for 1.5 KB).
    pub script_bytes: u64,
    pub status: ScriptStatus,
    /// Other titles in the same artifact compiling to these same bytes. One
    /// script, one deployment, however many handlers name it.
    pub shares_script_with: Vec<String>,
}

/// What the reader did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptCatalogueAction {
    None,
    /// The tick set changed; the caller's `selected` has been updated.
    SelectionChanged,
}

/// The catalogue widget. Borrows the rows and the caller's selection set.
pub struct ScriptCatalogue<'a> {
    rows: &'a [ScriptRow],
    selected: &'a mut BTreeSet<String>,
    enabled: bool,
}

impl<'a> ScriptCatalogue<'a> {
    pub fn new(rows: &'a [ScriptRow], selected: &'a mut BTreeSet<String>) -> Self {
        Self {
            rows,
            selected,
            enabled: true,
        }
    }

    /// Grey the ticks out (a build is in flight, no session, …).
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// How many DISTINCT scripts the current selection covers.
    ///
    /// Not the tick count: ticking three handlers of one program is one
    /// deployment and one lot of locked ADA. Quoting the tick count would
    /// overstate the cost by a factor of three.
    pub fn distinct_selected(rows: &[ScriptRow], selected: &BTreeSet<String>) -> usize {
        rows.iter()
            .filter(|r| selected.contains(&r.title))
            .map(|r| r.script_hash.as_str())
            .collect::<BTreeSet<_>>()
            .len()
    }

    /// Total bytes across the DISTINCT selected scripts, for a size estimate.
    pub fn selected_bytes(rows: &[ScriptRow], selected: &BTreeSet<String>) -> u64 {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        rows.iter()
            .filter(|r| selected.contains(&r.title))
            .filter(|r| seen.insert(r.script_hash.as_str()))
            .map(|r| r.script_bytes)
            .sum()
    }

    pub fn show(self, ui: &mut Ui) -> ScriptCatalogueAction {
        let mut action = ScriptCatalogueAction::None;
        let tokens = ui.tokens();
        let colors = tokens.color;
        let radius = tokens.corner(Radius::Sm);

        if self.rows.is_empty() {
            ui.label(
                RichText::new("No validators in this artifact.")
                    .color(colors.text_muted)
                    .size(ui.text_size(TextSize::Base)),
            );
            return action;
        }

        for row in self.rows {
            let mut ticked = self.selected.contains(&row.title);
            let selectable = row.status.selectable();

            egui::Frame::new()
                .fill(colors.bg_secondary)
                .inner_margin(egui::Margin::same(8))
                .corner_radius(radius)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Only a deployable row gets a tick. A disabled
                        // checkbox on an unapplied script would read as "not
                        // now" when the truth is "not from this file".
                        if selectable {
                            if ui
                                .add_enabled(self.enabled, egui::Checkbox::new(&mut ticked, ""))
                                .changed()
                            {
                                if ticked {
                                    self.selected.insert(row.title.clone());
                                } else {
                                    self.selected.remove(&row.title);
                                }
                                action = ScriptCatalogueAction::SelectionChanged;
                            }
                        } else {
                            ui.add_space(22.0);
                        }

                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&row.title)
                                        .color(if selectable {
                                            colors.text_primary
                                        } else {
                                            colors.text_muted
                                        })
                                        .size(ui.text_size(TextSize::Md))
                                        .monospace(),
                                );
                                Chip::new(row.status.label())
                                    .variant(row.status.variant())
                                    .show(ui);
                            });

                            ui.horizontal(|ui| {
                                // The hash is worth copying: it is the policy
                                // id for a minting script and the thing every
                                // deployment record is keyed by.
                                IdPill::new("hash", &row.script_hash)
                                    .layout(IdPillLayout::Inline)
                                    .show(ui);
                                if row.script_bytes > 0 {
                                    ui.label(
                                        RichText::new(format!("{} bytes", row.script_bytes))
                                            .color(colors.text_muted)
                                            .size(ui.text_size(TextSize::Sm)),
                                    );
                                }
                            });

                            if !row.shares_script_with.is_empty() {
                                ui.label(
                                    RichText::new(format!(
                                        "one script with {} — deploying any of them covers all",
                                        row.shares_script_with.join(", ")
                                    ))
                                    .color(colors.text_muted)
                                    .size(ui.text_size(TextSize::Sm)),
                                );
                            }

                            if let Some(why) = row.status.explanation() {
                                ui.label(
                                    RichText::new(why)
                                        .color(explanation_colour(&row.status, &colors))
                                        .size(ui.text_size(TextSize::Sm)),
                                );
                            }
                        });
                    });
                });
            ui.add_space(4.0);
        }

        action
    }
}

fn explanation_colour(status: &ScriptStatus, colors: &ColorTokens) -> Color32 {
    match status {
        ScriptStatus::Rejected { .. } => colors.error,
        ScriptStatus::Unapplied { .. } => colors.warning,
        ScriptStatus::Deployable | ScriptStatus::Deployed { .. } => colors.text_muted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(title: &str, hash: &str, bytes: u64, status: ScriptStatus) -> ScriptRow {
        ScriptRow {
            title: title.to_string(),
            script_hash: hash.to_string(),
            script_bytes: bytes,
            status,
            shares_script_with: Vec::new(),
        }
    }

    /// Only a finished, not-yet-deployed script may be ticked. The other three
    /// states each have a different remedy and none of them is "tick it".
    #[test]
    fn only_a_deployable_script_is_selectable() {
        assert!(ScriptStatus::Deployable.selectable());
        assert!(
            !ScriptStatus::Deployed {
                reference: "ab#0".into()
            }
            .selectable()
        );
        assert!(
            !ScriptStatus::Unapplied {
                parameters: vec!["registry".into()]
            }
            .selectable()
        );
        assert!(
            !ScriptStatus::Rejected {
                reason: "hash mismatch".into()
            }
            .selectable()
        );
    }

    /// THE counting rule. Ticking three handlers of one compiled program is
    /// ONE deployment locking ONE lot of ADA. Counting ticks would tell the
    /// operator they are about to spend three times what they will.
    #[test]
    fn handlers_of_one_program_count_once() {
        let rows = vec![
            row("fuel.mint", "aa", 1_500, ScriptStatus::Deployable),
            row("fuel.spend", "aa", 1_500, ScriptStatus::Deployable),
            row("fuel.else", "aa", 1_500, ScriptStatus::Deployable),
            row("escrow.spend", "bb", 900, ScriptStatus::Deployable),
        ];
        let all: BTreeSet<String> = rows.iter().map(|r| r.title.clone()).collect();

        assert_eq!(
            ScriptCatalogue::distinct_selected(&rows, &all),
            2,
            "four ticks over two programs is two deployments"
        );
        assert_eq!(
            ScriptCatalogue::selected_bytes(&rows, &all),
            2_400,
            "the shared program's bytes are counted once, not three times"
        );
    }

    #[test]
    fn an_empty_selection_costs_nothing() {
        let rows = vec![row("a.spend", "aa", 100, ScriptStatus::Deployable)];
        let none = BTreeSet::new();
        assert_eq!(ScriptCatalogue::distinct_selected(&rows, &none), 0);
        assert_eq!(ScriptCatalogue::selected_bytes(&rows, &none), 0);
    }

    /// Every non-deployable status must say WHY, or the reader is left with a
    /// row they cannot tick and no idea what to do about it.
    #[test]
    fn every_blocked_status_explains_itself() {
        for status in [
            ScriptStatus::Deployed {
                reference: "ab#0".into(),
            },
            ScriptStatus::Unapplied {
                parameters: vec!["registry_hash".into()],
            },
            ScriptStatus::Rejected {
                reason: "declared hash does not match".into(),
            },
        ] {
            let why = status.explanation();
            assert!(why.is_some(), "{status:?} must explain itself");
            assert!(!why.unwrap().is_empty());
        }
        // A ready row needs no excuse.
        assert_eq!(ScriptStatus::Deployable.explanation(), None);
    }

    /// An unapplied row names the parameters holding it back — that is the
    /// whole diagnostic, and it points at the contracts repo rather than here.
    #[test]
    fn an_unapplied_row_names_its_parameters() {
        let status = ScriptStatus::Unapplied {
            parameters: vec!["fuel_policy".into(), "fee_address".into()],
        };
        let why = status.explanation().unwrap();
        assert!(why.contains("fuel_policy"), "{why}");
        assert!(why.contains("fee_address"), "{why}");
    }
}
