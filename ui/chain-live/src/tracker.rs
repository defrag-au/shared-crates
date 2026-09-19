//! The egui projection of [`chain_heartbeat::Tracker`].
//!
//! The tracker itself moved to `chain-heartbeat` on 2026-09-16: it was already
//! pure except for the projection below, and leaving it here meant a macroquad
//! surface could only have had a second copy of landing detection. Two copies
//! of that WILL disagree about a rollback or a phase-2 failure in whichever one
//! is exercised less.
//!
//! What stays here is the only part that was ever egui's: turning
//! [`TxProgress`] into a [`TrainRider`]. The macroquad twin keeps its own.

pub(crate) use chain_heartbeat::{
    Asking, PollStep, TrackEvent as ChainLiveEvent, TrackedTx, Tracker, TxLanding,
};

use chain_heartbeat::TxProgress;
use egui_widgets::block_train::{RiderState, TrainRider};

/// Tracked transactions as the train draws them.
pub(crate) fn riders(txs: &[TrackedTx]) -> Vec<TrainRider> {
    txs.iter()
        .filter_map(|tracked| {
            let state = match tracked.progress {
                TxProgress::Waiting => RiderState::Waiting,
                TxProgress::Landed {
                    block_height: Some(height),
                } => RiderState::InBlock { height },
                // Still on the train as a status row while its block is found:
                // dropping it there is what made the status vanish between
                // "waiting" and "in a block".
                TxProgress::Landed { block_height: None } => RiderState::Landed,
                TxProgress::FailedInBlock { block_height } => RiderState::Failed {
                    height: block_height,
                },
                TxProgress::Dropped => return None,
            };
            Some(TrainRider::new(tracked.label.clone(), state))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracked(label: &str, progress: TxProgress) -> TrackedTx {
        TrackedTx {
            tx_hash: format!("{label}-hash"),
            label: label.to_string(),
            progress,
        }
    }

    /// The mapping the tracker's own tests used to assert before it moved. Each
    /// arm is a decision, not a formality.
    #[test]
    fn every_progress_draws_as_the_right_rider() {
        let txs = vec![
            tracked("Your swap", TxProgress::Waiting),
            tracked(
                "Your buy",
                TxProgress::Landed {
                    block_height: Some(100),
                },
            ),
            // No block yet: keeps its status row rather than vanishing between
            // "waiting" and "in a block" (changed on request 2026-09-15).
            tracked("Your claim", TxProgress::Landed { block_height: None }),
            tracked(
                "Your offer",
                TxProgress::FailedInBlock { block_height: 100 },
            ),
        ];
        assert_eq!(
            riders(&txs),
            vec![
                TrainRider::new("Your swap", RiderState::Waiting),
                TrainRider::new("Your buy", RiderState::InBlock { height: 100 }),
                TrainRider::new("Your claim", RiderState::Landed),
                TrainRider::new("Your offer", RiderState::Failed { height: 100 }),
            ]
        );
    }

    /// A transaction given up on leaves the train entirely — it is not a status
    /// worth a row, and it has no block to sit on.
    #[test]
    fn a_dropped_tx_is_off_the_train() {
        assert!(riders(&[tracked("Your swap", TxProgress::Dropped)]).is_empty());
    }
}
