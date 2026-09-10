//! `TxFlight` — one server-built, wallet-signed transaction as a checklist: build, review, sign, submit, land.
//!
//! The admin-operations loop every non-custodial surface shares: the backend
//! BUILDS an unsigned transaction from the operator's own UTxOs, the operator
//! READS what it does, the browser wallet SIGNS it, the backend SUBMITS it.
//! This widget draws that loop as three stages with one live state, and
//! hands back the click the host should act on. It never talks to a wallet
//! or a server itself — the host owns the async work and moves the
//! [`FlightPhase`] along as results arrive, which is also what makes the
//! widget testable and the story honest.
//!
//! Model state, not flags: the phase carries the review facts and the tx
//! hash in its variants, so there is no parallel `Option` to drift.
//!
//! ## Usage
//!
//! ```ignore
//! let resp = tx_flight::show(ui, &state.flight, &TxFlightConfig {
//!     build_label: "Build deployment",
//!     can_build: state.blueprint.is_some(),
//!     build_blocker: state.blueprint.is_none().then(|| "Upload a blueprint first".into()),
//!     ..Default::default()
//! });
//! match resp.action {
//!     Some(FlightAction::Build) => { state.flight = FlightPhase::Building; spawn_build(); }
//!     Some(FlightAction::Sign) => { /* wallet.sign_tx(...) */ }
//!     Some(FlightAction::Submit) => { /* POST signed CBOR */ }
//!     Some(FlightAction::Discard | FlightAction::Reset) => state.flight = FlightPhase::Idle,
//!     None => {}
//! }
//! ```

use egui::RichText;

use crate::button_group::{ButtonGroup, ButtonGroupButton};
use crate::error_note::ErrorNote;
use crate::icons::PhosphorIcon;
use crate::property_list::PropertyList;
use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// The three things that can go wrong, in the order they happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlightStage {
    Build,
    Sign,
    Submit,
}

impl FlightStage {
    pub const ALL: [FlightStage; 3] = [FlightStage::Build, FlightStage::Sign, FlightStage::Submit];

    pub fn label(self) -> &'static str {
        match self {
            FlightStage::Build => "Build",
            FlightStage::Sign => "Sign",
            FlightStage::Submit => "Submit",
        }
    }
}

/// What the operator reads before signing: a one-line headline and the
/// facts the transaction commits to, as label/value rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlightReview {
    pub headline: String,
    pub rows: Vec<(String, String)>,
}

/// Where the transaction is in its flight. The review facts ride along from
/// the moment they exist so every later stage can still show them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlightPhase {
    /// Nothing built. The host decides whether Build is possible yet.
    Idle,
    /// Waiting for the backend to build the unsigned transaction.
    Building,
    /// Built. The operator is reading the review and may sign or discard.
    Review(FlightReview),
    /// The wallet's approval dialog is open.
    Signing(FlightReview),
    /// Signed, and the backend is submitting.
    Submitting(FlightReview),
    /// On chain (or already known to it).
    Landed {
        review: FlightReview,
        tx_hash: String,
    },
    /// A stage failed. The host decides what retrying means.
    Failed { stage: FlightStage, error: String },
}

/// How one stage row should read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StageStatus {
    Pending,
    Active,
    Done,
    Failed,
}

impl FlightPhase {
    /// True while something asynchronous is in flight and the host should
    /// keep repainting.
    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            FlightPhase::Building | FlightPhase::Signing(_) | FlightPhase::Submitting(_)
        )
    }

    /// The review, once it exists.
    pub fn review(&self) -> Option<&FlightReview> {
        match self {
            FlightPhase::Review(r)
            | FlightPhase::Signing(r)
            | FlightPhase::Submitting(r)
            | FlightPhase::Landed { review: r, .. } => Some(r),
            FlightPhase::Idle | FlightPhase::Building | FlightPhase::Failed { .. } => None,
        }
    }

    fn stage_status(&self, stage: FlightStage) -> StageStatus {
        // Which stage the phase is "at", and whether it is mid-stage.
        let (at, active) = match self {
            FlightPhase::Idle => (FlightStage::Build, false),
            FlightPhase::Building => (FlightStage::Build, true),
            FlightPhase::Review(_) => (FlightStage::Sign, false),
            FlightPhase::Signing(_) => (FlightStage::Sign, true),
            FlightPhase::Submitting(_) => (FlightStage::Submit, true),
            FlightPhase::Landed { .. } => return StageStatus::Done,
            FlightPhase::Failed { stage: failed, .. } => {
                return match stage_index(stage).cmp(&stage_index(*failed)) {
                    std::cmp::Ordering::Less => StageStatus::Done,
                    std::cmp::Ordering::Equal => StageStatus::Failed,
                    std::cmp::Ordering::Greater => StageStatus::Pending,
                };
            }
        };
        match stage_index(stage).cmp(&stage_index(at)) {
            std::cmp::Ordering::Less => StageStatus::Done,
            std::cmp::Ordering::Equal if active => StageStatus::Active,
            _ => StageStatus::Pending,
        }
    }
}

fn stage_index(stage: FlightStage) -> usize {
    FlightStage::ALL
        .iter()
        .position(|s| *s == stage)
        .unwrap_or(0)
}

/// What the operator clicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlightAction {
    /// Build the unsigned transaction.
    Build,
    /// Open the wallet to sign the reviewed transaction.
    Sign,
    /// Submit (again) — offered after a submit failure, since the signed
    /// transaction is deterministic and safe to resend.
    Submit,
    /// Throw the built transaction away without signing.
    Discard,
    /// Back to idle after a landing or a failure.
    Reset,
}

impl FlightAction {
    pub const ALL: [FlightAction; 5] = [
        FlightAction::Build,
        FlightAction::Sign,
        FlightAction::Submit,
        FlightAction::Discard,
        FlightAction::Reset,
    ];

    fn id(self) -> u64 {
        match self {
            FlightAction::Build => 1,
            FlightAction::Sign => 2,
            FlightAction::Submit => 3,
            FlightAction::Discard => 4,
            FlightAction::Reset => 5,
        }
    }

    fn from_id(id: u64) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }
}

/// Presentation knobs. The host owns the words, since "Build" means
/// something different for a deployment and a whitelist entry.
pub struct TxFlightConfig {
    /// Label on the Build button, e.g. "Build deployment".
    pub build_label: &'static str,
    /// Whether Build may be clicked right now.
    pub can_build: bool,
    /// Why Build is disabled, shown beside it (e.g. "Upload a blueprint first").
    pub build_blocker: Option<String>,
    /// Label on the Sign button.
    pub sign_label: &'static str,
    pub font_size: f32,
    pub heading_size: f32,
}

impl Default for TxFlightConfig {
    fn default() -> Self {
        Self {
            build_label: "Build transaction",
            can_build: true,
            build_blocker: None,
            sign_label: "Sign with wallet",
            font_size: 11.0,
            heading_size: 12.0,
        }
    }
}

/// Outcome of one `show()`.
#[derive(Debug, Default)]
pub struct TxFlightResponse {
    pub action: Option<FlightAction>,
}

// ============================================================================
// Widget
// ============================================================================

/// Draw the flight and return what was clicked.
pub fn show(ui: &mut egui::Ui, phase: &FlightPhase, config: &TxFlightConfig) -> TxFlightResponse {
    crate::install_phosphor_font(ui.ctx());
    let mut response = TxFlightResponse::default();

    let heading = match phase {
        FlightPhase::Idle => "READY",
        FlightPhase::Building => "BUILDING",
        FlightPhase::Review(_) => "REVIEW",
        FlightPhase::Signing(_) => "SIGNING",
        FlightPhase::Submitting(_) => "SUBMITTING",
        FlightPhase::Landed { .. } => "LANDED",
        FlightPhase::Failed { .. } => "FAILED",
    };

    egui::Frame::new()
        .fill(theme::BG_SECONDARY)
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
        .show(ui, |ui| {
            ui.label(
                RichText::new(heading)
                    .color(theme::TEXT_SECONDARY)
                    .size(config.heading_size)
                    .strong(),
            );
            ui.add_space(6.0);

            // Stage ladder: three rows sharing a column edge.
            egui::Grid::new(ui.id().with("tx_flight_stages"))
                .spacing(egui::vec2(6.0, 4.0))
                .show(ui, |ui| {
                    for stage in FlightStage::ALL {
                        draw_stage_row(ui, stage, phase.stage_status(stage), config.font_size);
                        ui.end_row();
                    }
                });

            // The review, from the moment it exists.
            if let Some(review) = phase.review() {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(&review.headline)
                        .color(theme::TEXT_PRIMARY)
                        .size(config.font_size)
                        .strong(),
                );
                let mut list = PropertyList::new().id("tx_flight_review");
                for (label, value) in &review.rows {
                    list = list.add(label, value.clone());
                }
                list.show(ui);
            }

            ui.add_space(8.0);

            match phase {
                FlightPhase::Idle => {
                    let group = ButtonGroup::new().add(
                        ButtonGroupButton::new(FlightAction::Build.id(), config.build_label)
                            .icon(PhosphorIcon::Play)
                            .enabled(config.can_build),
                    );
                    response.action = group.show(ui).clicked.and_then(FlightAction::from_id);
                    if let Some(blocker) =
                        config.build_blocker.as_ref().filter(|_| !config.can_build)
                    {
                        ui.label(
                            RichText::new(blocker)
                                .color(theme::TEXT_MUTED)
                                .size(config.font_size - 1.0),
                        );
                    }
                }
                FlightPhase::Building => {
                    busy_line(ui, "Building the transaction…", config.font_size);
                }
                FlightPhase::Review(_) => {
                    let group = ButtonGroup::new()
                        .add(
                            ButtonGroupButton::new(FlightAction::Sign.id(), config.sign_label)
                                .icon(PhosphorIcon::Check),
                        )
                        .add(ButtonGroupButton::new(
                            FlightAction::Discard.id(),
                            "Discard",
                        ));
                    response.action = group.show(ui).clicked.and_then(FlightAction::from_id);
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Nothing is sent until you approve it in your wallet.")
                            .color(theme::TEXT_MUTED)
                            .size(config.font_size - 1.0),
                    );
                }
                FlightPhase::Signing(_) => {
                    busy_line(
                        ui,
                        "Check your wallet — approve the transaction there.",
                        config.font_size,
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Hardware wallets can take a minute.")
                            .color(theme::TEXT_MUTED)
                            .size(config.font_size - 1.0),
                    );
                }
                FlightPhase::Submitting(_) => {
                    busy_line(ui, "Submitting to the chain…", config.font_size);
                }
                FlightPhase::Landed { tx_hash, .. } => {
                    ui.horizontal(|ui| {
                        PhosphorIcon::CheckCircle.show(
                            ui,
                            config.heading_size,
                            theme::ACCENT_GREEN,
                        );
                        ui.label(
                            RichText::new("On chain")
                                .color(theme::ACCENT_GREEN)
                                .size(config.heading_size),
                        );
                    });
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(crate::utils::truncate_hex(tx_hash, 10, 10))
                                .color(theme::TEXT_MUTED)
                                .size(config.font_size - 1.0)
                                .monospace(),
                        );
                        if ui
                            .small_button("copy")
                            .on_hover_text("Copy the full transaction hash")
                            .clicked()
                        {
                            ui.ctx().copy_text(tx_hash.clone());
                        }
                    });
                    ui.add_space(6.0);
                    let group = ButtonGroup::new().add(
                        ButtonGroupButton::new(FlightAction::Reset.id(), "Start another")
                            .icon(PhosphorIcon::ArrowsClockwise),
                    );
                    response.action = group.show(ui).clicked.and_then(FlightAction::from_id);
                }
                FlightPhase::Failed { stage, error } => {
                    ui.label(
                        RichText::new(format!("{} failed", stage.label()))
                            .color(theme::ACCENT_RED)
                            .size(config.font_size)
                            .strong(),
                    );
                    ErrorNote::new(error).show(ui);
                    ui.add_space(6.0);
                    let mut group = ButtonGroup::new();
                    // A signed transaction is deterministic: resending it is
                    // safe, and a landed one comes back as a duplicate.
                    if *stage == FlightStage::Submit {
                        group = group.add(
                            ButtonGroupButton::new(FlightAction::Submit.id(), "Submit again")
                                .icon(PhosphorIcon::ArrowsClockwise),
                        );
                    }
                    group = group.add(ButtonGroupButton::new(
                        FlightAction::Reset.id(),
                        "Start over",
                    ));
                    response.action = group.show(ui).clicked.and_then(FlightAction::from_id);
                }
            }
        });

    response
}

fn busy_line(ui: &mut egui::Ui, text: &str, font_size: f32) {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(
            RichText::new(text)
                .color(theme::ACCENT_CYAN)
                .size(font_size),
        );
    });
}

/// One ladder row: icon, stage name, status word. The caller owns the grid.
fn draw_stage_row(ui: &mut egui::Ui, stage: FlightStage, status: StageStatus, font_size: f32) {
    let (word, color) = match status {
        StageStatus::Pending => ("pending", theme::TEXT_MUTED),
        StageStatus::Active => ("in progress", theme::ACCENT_CYAN),
        StageStatus::Done => ("done", theme::ACCENT_GREEN),
        StageStatus::Failed => ("failed", theme::ACCENT_RED),
    };
    match status {
        StageStatus::Pending => {
            PhosphorIcon::Clock.show(ui, 14.0, theme::TEXT_MUTED);
        }
        StageStatus::Active => {
            ui.add(egui::Spinner::new().size(12.0));
        }
        StageStatus::Done => {
            PhosphorIcon::CheckCircle.show(ui, 14.0, theme::ACCENT_GREEN);
        }
        StageStatus::Failed => {
            PhosphorIcon::Warning.show(ui, 14.0, theme::ACCENT_RED);
        }
    }
    ui.label(
        RichText::new(stage.label())
            .color(theme::TEXT_PRIMARY)
            .size(font_size),
    );
    ui.label(RichText::new(word).color(color).size(font_size));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review() -> FlightReview {
        FlightReview {
            headline: "Park ask.spend".into(),
            rows: vec![("Fee".into(), "0.19 ADA".into())],
        }
    }

    #[test]
    fn ladder_follows_the_phase() {
        use StageStatus::*;
        let ladder = |p: &FlightPhase| FlightStage::ALL.map(|s| p.stage_status(s));
        assert_eq!(ladder(&FlightPhase::Idle), [Pending, Pending, Pending]);
        assert_eq!(ladder(&FlightPhase::Building), [Active, Pending, Pending]);
        assert_eq!(
            ladder(&FlightPhase::Review(review())),
            [Done, Pending, Pending]
        );
        assert_eq!(
            ladder(&FlightPhase::Signing(review())),
            [Done, Active, Pending]
        );
        assert_eq!(
            ladder(&FlightPhase::Submitting(review())),
            [Done, Done, Active]
        );
        assert_eq!(
            ladder(&FlightPhase::Landed {
                review: review(),
                tx_hash: "ab".into()
            }),
            [Done, Done, Done]
        );
        assert_eq!(
            ladder(&FlightPhase::Failed {
                stage: FlightStage::Sign,
                error: "declined".into()
            }),
            [Done, Failed, Pending]
        );
    }

    #[test]
    fn review_survives_every_later_phase() {
        let r = review();
        assert_eq!(FlightPhase::Review(r.clone()).review(), Some(&r));
        assert_eq!(FlightPhase::Signing(r.clone()).review(), Some(&r));
        assert_eq!(FlightPhase::Submitting(r.clone()).review(), Some(&r));
        assert_eq!(
            FlightPhase::Landed {
                review: r.clone(),
                tx_hash: "ab".into()
            }
            .review(),
            Some(&r)
        );
        assert_eq!(FlightPhase::Idle.review(), None);
    }

    #[test]
    fn actions_round_trip_through_button_ids() {
        for action in FlightAction::ALL {
            assert_eq!(FlightAction::from_id(action.id()), Some(action));
        }
        assert_eq!(FlightAction::from_id(0), None);
    }

    #[test]
    fn busy_phases_are_the_async_ones() {
        assert!(FlightPhase::Building.is_busy());
        assert!(FlightPhase::Signing(review()).is_busy());
        assert!(FlightPhase::Submitting(review()).is_busy());
        assert!(!FlightPhase::Idle.is_busy());
        assert!(!FlightPhase::Review(review()).is_busy());
    }
}
