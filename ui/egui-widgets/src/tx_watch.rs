//! `TxWatch` — several transactions on their way to chain, with the wait made visible.
//!
//! NOT [`crate::tx_flight`], which DRIVES one transaction: it owns the Build /
//! Sign / Submit buttons and hands the host an action to perform. This one
//! drives nothing. It watches a bundle that is already in motion — signed,
//! submitting, waiting for a block — and its whole job is that the reader can
//! see progress without reading anything.
//!
//! Three reasons it is its own widget:
//!
//! - **Several transactions.** A chained plan is two or more, submitted in
//!   order, and the interesting question is which one the bundle is on.
//! - **Past submit.** `tx_flight` finishes at `Landed` the moment a node
//!   accepts. The wait a user actually feels is the one AFTER that, for a
//!   block, and it is the wait most likely to make someone refresh the page.
//! - **Motion.** A static checklist during a 40-second wait reads as a hung
//!   screen. The active stage pulses so the surface is visibly alive.
//!
//! ## Motion is opacity, not travel
//!
//! The pulse is an alpha ramp, which stays legible under
//! [`MotionMode::Reduced`](crate::theme::MotionMode) and is the reason it was
//! chosen over anything that moves. Under `MotionMode::None` it settles flat
//! and the widget stops asking for repaints at all — which is what makes a
//! screenshot of this deterministic.
//!
//! The host still owns the async work. This widget asks for repaints only to
//! advance its OWN animation, keyed off an explicit busy phase rather than off
//! missing data — `data.is_none()` is also what failure looks like, and keying
//! a repaint loop on it spins forever on a flat battery.

use egui::{Color32, RichText, Ui};

use crate::icons::PhosphorIcon;
use crate::theme::{Radius, Space, SpaceExt, Speed, TextSize, ThemeExt};

// ============================================================================
// Types
// ============================================================================

/// The three things that happen to a transaction once the plan exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TxStage {
    Sign,
    Submit,
    /// Accepted by a node, waiting for a block.
    Confirm,
}

impl TxStage {
    pub const ALL: [TxStage; 3] = [TxStage::Sign, TxStage::Submit, TxStage::Confirm];

    pub fn label(self) -> &'static str {
        match self {
            TxStage::Sign => "sign",
            TxStage::Submit => "submit",
            TxStage::Confirm => "confirm",
        }
    }
}

/// Where one transaction has got to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TxPhase {
    /// Built, but its turn has not come — an earlier transaction in the chain
    /// has to land first.
    Waiting,
    /// The wallet dialog is open, or this transaction is part of a bundle
    /// being signed in one.
    Signing,
    Submitting,
    /// A node took it. Now it needs a block.
    Confirming {
        tx_hash: String,
    },
    Confirmed {
        tx_hash: String,
    },
    Failed {
        stage: TxStage,
        error: String,
    },
}

impl TxPhase {
    /// True while something is actually happening — what the pulse and the
    /// repaint loop key off.
    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            TxPhase::Signing | TxPhase::Submitting | TxPhase::Confirming { .. }
        )
    }

    pub fn tx_hash(&self) -> Option<&str> {
        match self {
            TxPhase::Confirming { tx_hash } | TxPhase::Confirmed { tx_hash } => Some(tx_hash),
            _ => None,
        }
    }

    /// How one stage's mark should read for this phase.
    fn status(&self, stage: TxStage) -> MarkStatus {
        let (at, active) = match self {
            TxPhase::Waiting => return MarkStatus::Pending,
            TxPhase::Signing => (TxStage::Sign, true),
            TxPhase::Submitting => (TxStage::Submit, true),
            TxPhase::Confirming { .. } => (TxStage::Confirm, true),
            TxPhase::Confirmed { .. } => return MarkStatus::Done,
            TxPhase::Failed { stage: failed, .. } => {
                return match stage.cmp(failed) {
                    std::cmp::Ordering::Less => MarkStatus::Done,
                    std::cmp::Ordering::Equal => MarkStatus::Failed,
                    std::cmp::Ordering::Greater => MarkStatus::Pending,
                };
            }
        };
        match stage.cmp(&at) {
            std::cmp::Ordering::Less => MarkStatus::Done,
            std::cmp::Ordering::Equal if active => MarkStatus::Active,
            _ => MarkStatus::Pending,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkStatus {
    Pending,
    Active,
    Done,
    Failed,
}

/// A pending mark's radius, as a fraction of the label's point size — so the
/// track scales with the type ramp rather than sitting at a fixed pixel size
/// that a larger theme would dwarf. Sized to read as the same weight as the
/// `Check` glyph it alternates with.
const MARK_RADIUS: f32 = 0.28;

/// How far a pending mark dims. Opacity, not a separate colour, so the track
/// reads as one run at two strengths.
const PENDING_ALPHA: f32 = 0.35;

/// One transaction in the bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchedTx {
    /// What this transaction DOES, in the reader's terms — "Swap ADA for
    /// LUMP", not "transaction 1".
    pub label: String,
    pub phase: TxPhase,
    /// Shown once this transaction has landed. For a chained plan this is the
    /// place to say what the user now holds, which is the fact they most want
    /// while the next one is still in flight.
    pub landed_note: Option<String>,
}

impl WatchedTx {
    pub fn new(label: impl Into<String>, phase: TxPhase) -> Self {
        Self {
            label: label.into(),
            phase,
            landed_note: None,
        }
    }
}

/// Display options. Sizes are theme steps — see `tests/theme_tokens.rs`.
#[derive(Clone, Debug)]
pub struct TxWatchConfig {
    pub body: TextSize,
    pub detail: TextSize,
    /// Show each transaction's hash once it exists.
    pub show_hashes: bool,
}

impl Default for TxWatchConfig {
    fn default() -> Self {
        Self {
            body: TextSize::Md,
            detail: TextSize::Base,
            show_hashes: true,
        }
    }
}

/// The bundle's overall state, for a host that wants to say something about
/// it without re-deriving it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundleState {
    /// Every transaction confirmed.
    Complete,
    /// One failed; the rest will not proceed.
    Stalled,
    /// Still going.
    InFlight,
}

/// What the bundle is doing, derived from its transactions.
pub fn bundle_state(transactions: &[WatchedTx]) -> BundleState {
    if transactions
        .iter()
        .any(|t| matches!(t.phase, TxPhase::Failed { .. }))
    {
        return BundleState::Stalled;
    }
    if !transactions.is_empty()
        && transactions
            .iter()
            .all(|t| matches!(t.phase, TxPhase::Confirmed { .. }))
    {
        return BundleState::Complete;
    }
    BundleState::InFlight
}

/// How many have confirmed.
pub fn confirmed_count(transactions: &[WatchedTx]) -> usize {
    transactions
        .iter()
        .filter(|t| matches!(t.phase, TxPhase::Confirmed { .. }))
        .count()
}

// ============================================================================
// Widget
// ============================================================================

/// Render the watcher.
pub fn show(ui: &mut Ui, transactions: &[WatchedTx], config: &TxWatchConfig) {
    egui::Frame::new()
        .fill(ui.tokens().color.bg_secondary)
        .corner_radius(ui.tokens().corner(Radius::Md))
        .inner_margin(ui.tokens().margin(Space::Xl))
        .stroke(ui.tokens().geometry.border(ui.tokens().color.border))
        .show(ui, |ui| {
            // Read-only rows: opt out of the touch-target floor, which
            // otherwise sets the height of every `horizontal` row.
            ui.spacing_mut().interact_size = egui::Vec2::ZERO;
            ui.spacing_mut().item_spacing.y = ui.tokens().space(Space::Sm);

            let body = ui.text_size(config.body);
            let detail = ui.text_size(config.detail);
            let busy = transactions.iter().any(|t| t.phase.is_busy());

            // The pulse. A full cycle per `Speed::Slow`, so it reads as
            // breathing rather than blinking. Zero under `MotionMode::None`,
            // which also stops the repaint request below.
            let period = ui.duration(Speed::Slow).max(0.0);
            let pulse = if period > 0.0 && busy {
                let t = ui.input(|i| i.time) as f32;
                // Smooth at both ends, and bounded BELOW at the pending
                // strength so the active mark never dims past a mark that is
                // merely waiting — which would read as going backwards.
                let wave = 0.5 - 0.5 * (t / period * std::f32::consts::TAU).cos();
                PENDING_ALPHA + (1.0 - PENDING_ALPHA) * wave
            } else {
                1.0
            };

            header(ui, transactions, body);
            ui.gap(Space::Md);

            for (index, watched) in transactions.iter().enumerate() {
                transaction_row(
                    ui,
                    watched,
                    index,
                    transactions.len(),
                    pulse,
                    body,
                    detail,
                    config,
                );
                if index + 1 < transactions.len() {
                    ui.gap(Space::Sm);
                }
            }

            // Keep the frame clock running ONLY while something is actually
            // in flight, and only when motion is on. Keyed off the phase, not
            // off absent data.
            if busy && period > 0.0 {
                ui.ctx().request_repaint();
            }
        });
}

fn header(ui: &mut Ui, transactions: &[WatchedTx], size: f32) {
    let total = transactions.len();
    let done = confirmed_count(transactions);
    let (text, colour) = match bundle_state(transactions) {
        BundleState::Complete if total == 1 => {
            ("Confirmed".to_string(), ui.tokens().color.accent_green)
        }
        BundleState::Complete => (
            format!("All {total} transactions confirmed"),
            ui.tokens().color.accent_green,
        ),
        BundleState::Stalled => ("Stopped — see below".to_string(), ui.tokens().color.error),
        BundleState::InFlight if total == 1 => {
            ("In flight".to_string(), ui.tokens().color.text_primary)
        }
        BundleState::InFlight => (
            format!("{done} of {total} confirmed"),
            ui.tokens().color.text_primary,
        ),
    };
    ui.label(RichText::new(text).color(colour).strong().size(size));
}

#[allow(clippy::too_many_arguments)]
fn transaction_row(
    ui: &mut Ui,
    watched: &WatchedTx,
    index: usize,
    total: usize,
    pulse: f32,
    body: f32,
    detail: f32,
    config: &TxWatchConfig,
) {
    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(
                RichText::new(&watched.label)
                    .color(ui.tokens().color.text_primary)
                    .size(body),
            )
            .truncate(),
        );
        if total > 1 {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} of {total}", index + 1))
                        .color(ui.tokens().color.text_muted)
                        .size(detail),
                );
            });
        }
    });

    stage_marks(ui, &watched.phase, pulse, detail);

    if config.show_hashes
        && let Some(hash) = watched.phase.tx_hash()
    {
        ui.label(
            RichText::new(elide(hash))
                .color(ui.tokens().color.text_muted)
                .monospace()
                .size(detail),
        );
    }

    if let TxPhase::Failed { error, .. } = &watched.phase {
        ui.label(
            RichText::new(error)
                .color(ui.tokens().color.error)
                .size(detail),
        );
    }

    // What the user now holds — the fact they most want while the NEXT
    // transaction is still in flight.
    if let Some(note) = &watched.landed_note
        && matches!(watched.phase, TxPhase::Confirmed { .. })
    {
        ui.label(
            RichText::new(note)
                .color(ui.tokens().color.text_secondary)
                .size(detail),
        );
    }
}

/// The pip track: one mark per stage, the active one breathing.
fn stage_marks(ui: &mut Ui, phase: &TxPhase, pulse: f32, size: f32) {
    // Every item in this row is allocated at the LABEL's line height.
    //
    // A `horizontal` centres on the cross axis against each item's own
    // allocation, so a dot allocated `size × size` beside text allocated a
    // full line height rides up against the ascenders. It used to look right
    // only because `interact_size.y` was silently padding both to the same
    // touch target — and opting this region out of that (which the dense
    // layout needs) took the coincidence with it.
    let row = crate::theme::line_height(ui, size);

    ui.horizontal(|ui| {
        ui.add_space(ui.tokens().space(Space::Sm));
        for (position, stage) in TxStage::ALL.into_iter().enumerate() {
            if position > 0 {
                // A hairline joining the marks, so the track reads as one run
                // rather than three dots.
                let width = ui.tokens().space(Space::Lg);
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(width, row), egui::Sense::hover());
                if ui.is_rect_visible(rect) {
                    let y = rect.center().y;
                    let stroke = ui.tokens().geometry.border(ui.tokens().color.border);
                    ui.painter().line_segment(
                        [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                        stroke,
                    );
                }
            }

            let status = phase.status(stage);
            let colour = match status {
                MarkStatus::Done => ui.tokens().color.accent_green,
                MarkStatus::Active => ui.tokens().color.accent,
                MarkStatus::Failed => ui.tokens().color.error,
                MarkStatus::Pending => ui.tokens().color.text_muted,
            };

            match status {
                // A completed stage is a tick, not a filled dot: at a glance a
                // reader should be able to count what is DONE without
                // decoding a colour.
                MarkStatus::Done => {
                    PhosphorIcon::Check.show(ui, size, colour);
                }
                MarkStatus::Failed => {
                    PhosphorIcon::Warning.show(ui, size, colour);
                }
                _ => {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(size, row), egui::Sense::hover());
                    if ui.is_rect_visible(rect) {
                        let alpha = if status == MarkStatus::Active {
                            pulse
                        } else {
                            PENDING_ALPHA
                        };
                        ui.painter().circle_filled(
                            rect.center(),
                            size * MARK_RADIUS,
                            fade(colour, alpha),
                        );
                    }
                }
            }

            ui.label(
                RichText::new(stage.label())
                    .color(match status {
                        MarkStatus::Pending => ui.tokens().color.text_muted,
                        _ => colour,
                    })
                    .size(size),
            );
        }
    });
}

/// Scale a colour's alpha, PREMULTIPLYING as `Color32` requires.
///
/// `from_rgba_premultiplied` with channels above the alpha blends additively
/// and renders far lighter than intended — the trap `tests/contrast.rs`
/// exists for.
fn fade(colour: Color32, alpha: f32) -> Color32 {
    let a = alpha.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (colour.r() as f32 * a) as u8,
        (colour.g() as f32 * a) as u8,
        (colour.b() as f32 * a) as u8,
        (colour.a() as f32 * a) as u8,
    )
}

/// `a7f3c1d2…9b2c4e1f` — enough to recognise against an explorer.
fn elide(hash: &str) -> String {
    if hash.len() <= 20 {
        return hash.to_string();
    }
    format!("{}…{}", &hash[..8], &hash[hash.len() - 8..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx(phase: TxPhase) -> WatchedTx {
        WatchedTx::new("leg", phase)
    }

    fn hash() -> String {
        "a7f3c1d2b4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b9b2c4e1f".to_string()
    }

    /// A stage before the current one is done, the current one is active, and
    /// the rest are pending — the mapping the whole widget reads from.
    #[test]
    fn marks_advance_with_the_phase() {
        let submitting = TxPhase::Submitting;
        assert_eq!(submitting.status(TxStage::Sign), MarkStatus::Done);
        assert_eq!(submitting.status(TxStage::Submit), MarkStatus::Active);
        assert_eq!(submitting.status(TxStage::Confirm), MarkStatus::Pending);

        // Waiting has not started: nothing is done yet.
        for stage in TxStage::ALL {
            assert_eq!(TxPhase::Waiting.status(stage), MarkStatus::Pending);
        }
        // Confirmed is done all the way across.
        let done = TxPhase::Confirmed { tx_hash: hash() };
        for stage in TxStage::ALL {
            assert_eq!(done.status(stage), MarkStatus::Done);
        }
    }

    /// A failure marks its own stage and leaves the later ones pending —
    /// never "done", which would claim something that did not happen.
    #[test]
    fn a_failure_does_not_imply_later_stages() {
        let failed = TxPhase::Failed {
            stage: TxStage::Submit,
            error: "node rejected".into(),
        };
        assert_eq!(failed.status(TxStage::Sign), MarkStatus::Done);
        assert_eq!(failed.status(TxStage::Submit), MarkStatus::Failed);
        assert_eq!(failed.status(TxStage::Confirm), MarkStatus::Pending);
    }

    /// The repaint loop and the pulse key off this, so it has to be true only
    /// while something is really happening.
    #[test]
    fn only_in_flight_phases_are_busy() {
        assert!(TxPhase::Signing.is_busy());
        assert!(TxPhase::Submitting.is_busy());
        assert!(TxPhase::Confirming { tx_hash: hash() }.is_busy());

        assert!(!TxPhase::Waiting.is_busy());
        assert!(!TxPhase::Confirmed { tx_hash: hash() }.is_busy());
        assert!(
            !TxPhase::Failed {
                stage: TxStage::Sign,
                error: "declined".into()
            }
            .is_busy(),
            "a failed transaction is not in flight — a spinner on it would lie"
        );
    }

    #[test]
    fn bundle_state_needs_every_transaction() {
        let all_done = vec![
            tx(TxPhase::Confirmed { tx_hash: hash() }),
            tx(TxPhase::Confirmed { tx_hash: hash() }),
        ];
        assert_eq!(bundle_state(&all_done), BundleState::Complete);
        assert_eq!(confirmed_count(&all_done), 2);

        let half = vec![
            tx(TxPhase::Confirmed { tx_hash: hash() }),
            tx(TxPhase::Submitting),
        ];
        assert_eq!(bundle_state(&half), BundleState::InFlight);
        assert_eq!(confirmed_count(&half), 1);

        // One failure stalls the bundle even if others confirmed.
        let stalled = vec![
            tx(TxPhase::Confirmed { tx_hash: hash() }),
            tx(TxPhase::Failed {
                stage: TxStage::Confirm,
                error: "pool moved".into(),
            }),
        ];
        assert_eq!(bundle_state(&stalled), BundleState::Stalled);

        // An empty bundle is not "complete".
        assert_eq!(bundle_state(&[]), BundleState::InFlight);
    }

    /// Premultiplied, so a faded mark dims rather than glowing.
    #[test]
    fn fading_keeps_channels_within_alpha() {
        let faded = fade(Color32::from_rgb(200, 180, 40), 0.35);
        assert!(faded.r() <= faded.a(), "r {} > a {}", faded.r(), faded.a());
        assert!(faded.g() <= faded.a(), "g {} > a {}", faded.g(), faded.a());
        assert!(faded.b() <= faded.a(), "b {} > a {}", faded.b(), faded.a());
        // Fully opaque is unchanged.
        let solid = fade(Color32::from_rgb(200, 180, 40), 1.0);
        assert_eq!((solid.r(), solid.g(), solid.b()), (200, 180, 40));
    }

    #[test]
    fn a_hash_elides_to_both_ends() {
        assert_eq!(elide(&hash()), "a7f3c1d2…9b2c4e1f");
        assert_eq!(elide("short"), "short");
    }
}
