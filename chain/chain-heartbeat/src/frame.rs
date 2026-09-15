//! Frames: how a heartbeat host shares its view of the chain.
//!
//! A host (the gateway Durable Object) follows the chain. Subscribers (a
//! surface's Durable Object, and the browsers attached to it) keep their own
//! [`Heartbeat`] in step by applying frames. Each subscriber folds with its own
//! clock, so "seconds since block" ticks locally without a frame per second.
//!
//! Every frame wakes every subscriber, so [`FrameBatcher`] keeps them rare:
//!
//! - A block at the tip is sent at once.
//! - Everything else (catch-up blocks, rollbacks, reconnects) is held and goes
//!   out with the next tip block, so a replay of thirty blocks is one frame.
//! - Keep-alive answers only go out when nothing else has for
//!   [`PULSE_AFTER_SECS`]. Keep-alives land every 30 s, so the longest gap a
//!   subscriber sees on a live feed is 75 s, inside
//!   [`crate::SILENT_AFTER_SECS`], and a live feed never looks silent
//!   downstream.

use serde::{Deserialize, Serialize};

use crate::beat::{BlockTxs, ChainEvent, SyncState};
use crate::heartbeat::{Checkpoint, FeedHealth, Heartbeat, Restore};

/// Send a keep-alive pulse once nothing has been sent for this long.
pub const PULSE_AFTER_SECS: u64 = 45;

/// Held events are sent once this many pile up, even without a tip block.
pub const MAX_BATCH: usize = 64;

/// One message from a heartbeat host to a subscriber.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum HeartbeatFrame {
    /// Everything a new subscriber needs: the recent blocks and the host's feed.
    Resync {
        checkpoint: Checkpoint,
        feed: FeedHealth,
        /// The newest blocks' transactions, so a subscriber that was away can
        /// still spot one of its own that landed meanwhile.
        #[serde(default)]
        recent_txs: Vec<BlockTxs>,
    },
    /// Chain events, in order.
    Events { events: Vec<ChainEvent> },
    /// The host lost its connection to the relay.
    UpstreamLost,
}

/// What applying a frame did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameApplied {
    Applied,
    /// A resync for another network was ignored.
    WrongNetwork,
}

/// Host side: turns the follower's events into as few frames as possible.
#[derive(Debug, Clone, Default)]
pub struct FrameBatcher {
    pending: Vec<ChainEvent>,
    last_sent_ms: Option<u64>,
}

impl FrameBatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an event. Returns the frame to send now, if any.
    pub fn push(&mut self, event: ChainEvent, now_ms: u64) -> Option<HeartbeatFrame> {
        let send_now = match &event {
            ChainEvent::RollForward {
                sync: SyncState::AtTip,
                ..
            } => true,
            ChainEvent::KeepAliveAcknowledged => {
                let pulse_due = self
                    .last_sent_ms
                    .is_none_or(|sent| now_ms.saturating_sub(sent) >= PULSE_AFTER_SECS * 1000);
                if !pulse_due {
                    // An acknowledgement carries nothing but liveness, and
                    // something recent already proved that.
                    return None;
                }
                true
            }
            // A block's transactions are sent just before the block, so holding
            // them puts both in the same frame when the block flushes.
            ChainEvent::RollForward {
                sync: SyncState::CatchingUp,
                ..
            }
            | ChainEvent::RollBackward { .. }
            | ChainEvent::Connected { .. }
            | ChainEvent::BlockTransactions { .. } => false,
        };
        self.pending.push(event);
        if send_now || self.pending.len() >= MAX_BATCH {
            Some(self.flush(now_ms))
        } else {
            None
        }
    }

    /// The host lost the relay. Held events go out first, so subscribers keep
    /// every block the host saw.
    pub fn upstream_lost(&mut self, now_ms: u64) -> Vec<HeartbeatFrame> {
        let mut frames = Vec::with_capacity(2);
        if !self.pending.is_empty() {
            frames.push(self.flush(now_ms));
        }
        self.last_sent_ms = Some(now_ms);
        frames.push(HeartbeatFrame::UpstreamLost);
        frames
    }

    fn flush(&mut self, now_ms: u64) -> HeartbeatFrame {
        self.last_sent_ms = Some(now_ms);
        HeartbeatFrame::Events {
            events: std::mem::take(&mut self.pending),
        }
    }
}

impl Heartbeat {
    /// Subscriber side: apply a frame from a host.
    ///
    /// A subscriber's own link to the host is a second hop. When that link
    /// drops (the browser's socket closes), call [`Heartbeat::disconnected`]
    /// too, or the snapshot will describe a feed nobody is receiving.
    pub fn apply_frame(&mut self, frame: &HeartbeatFrame, now_ms: u64) -> FrameApplied {
        match frame {
            HeartbeatFrame::Resync {
                checkpoint,
                feed,
                recent_txs,
            } => {
                if let Restore::WrongNetwork = self.restore(checkpoint.clone()) {
                    return FrameApplied::WrongNetwork;
                }
                self.adopt_feed(*feed, now_ms);
                self.replace_recent_txs(recent_txs.clone());
            }
            HeartbeatFrame::Events { events } => {
                for event in events {
                    self.apply(event, now_ms);
                }
            }
            HeartbeatFrame::UpstreamLost => self.disconnected(now_ms),
        }
        FrameApplied::Applied
    }

    /// Host side: the frame that brings a new subscriber up to date.
    pub fn resync_frame(&self, now_ms: u64) -> HeartbeatFrame {
        HeartbeatFrame::Resync {
            checkpoint: self.checkpoint(),
            feed: self.snapshot(now_ms).feed,
            recent_txs: self.recent_txs().cloned().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beat::{BlockBeat, ChainPoint};
    use crate::network::Network;

    const BASE_SLOT: u64 = 186_000_000;

    #[test]
    fn recent_transactions_ride_a_resync_and_leave_with_a_rollback() {
        let mut host = Heartbeat::new(Network::Mainnet);
        host.connected(ms_at(BASE_SLOT));
        for i in 0..10u64 {
            let slot = BASE_SLOT + i * 20;
            host.apply(
                &ChainEvent::BlockTransactions {
                    txs: BlockTxs::new(i, slot, &[[i as u8; 32]], Vec::new()),
                },
                ms_at(slot),
            );
            host.apply(&block(i, slot, SyncState::AtTip), ms_at(slot));
        }
        // Only the newest few blocks' transactions are kept.
        assert_eq!(host.recent_txs().len(), crate::heartbeat::RECENT_TX_BLOCKS);

        host.apply(
            &ChainEvent::RollBackward {
                to: Some(ChainPoint {
                    slot: BASE_SLOT + 8 * 20,
                    hash: [0; 32],
                }),
            },
            ms_at(BASE_SLOT + 200),
        );
        assert_eq!(host.recent_txs().last().map(|txs| txs.height), Some(8));

        let mut subscriber = Heartbeat::new(Network::Mainnet);
        subscriber.apply_frame(
            &host.resync_frame(ms_at(BASE_SLOT + 200)),
            ms_at(BASE_SLOT + 200),
        );
        assert_eq!(
            subscriber.recent_txs().collect::<Vec<_>>(),
            host.recent_txs().collect::<Vec<_>>()
        );
    }

    fn ms_at(slot: u64) -> u64 {
        Network::Mainnet.slot_to_unix_secs(slot).unwrap() * 1000
    }

    fn block(height: u64, slot: u64, sync: SyncState) -> ChainEvent {
        ChainEvent::RollForward {
            beat: BlockBeat {
                height,
                slot,
                hash: format!("{height:064x}"),
                issuer_pool: "11".repeat(28),
                body_size: 20_000,
                tx_count: Some(4),
                block_time_unix: Network::Mainnet.slot_to_unix_secs(slot),
            },
            sync,
        }
    }

    #[test]
    fn a_replay_is_held_until_the_tip_block() {
        let mut batcher = FrameBatcher::new();
        assert_eq!(batcher.push(ChainEvent::Connected { version: 14 }, 0), None);
        for i in 0..5 {
            assert_eq!(
                batcher.push(block(i, BASE_SLOT + i * 20, SyncState::CatchingUp), 0),
                None
            );
        }
        let Some(HeartbeatFrame::Events { events }) =
            batcher.push(block(5, BASE_SLOT + 100, SyncState::AtTip), 0)
        else {
            panic!("the tip block sends the batch");
        };
        assert_eq!(events.len(), 7);
        assert_eq!(events[0], ChainEvent::Connected { version: 14 });
    }

    #[test]
    fn keep_alive_answers_only_pulse_after_a_quiet_spell() {
        let mut batcher = FrameBatcher::new();
        batcher.push(block(1, BASE_SLOT, SyncState::AtTip), 0);
        assert_eq!(
            batcher.push(ChainEvent::KeepAliveAcknowledged, 30_000),
            None
        );
        assert_eq!(
            batcher.push(ChainEvent::KeepAliveAcknowledged, 45_000),
            Some(HeartbeatFrame::Events {
                events: vec![ChainEvent::KeepAliveAcknowledged]
            })
        );
    }

    #[test]
    fn a_long_replay_is_sent_in_bounded_batches() {
        let mut batcher = FrameBatcher::new();
        let mut frames = 0;
        for i in 0..(MAX_BATCH as u64 * 2) {
            if batcher
                .push(block(i, BASE_SLOT + i * 20, SyncState::CatchingUp), 0)
                .is_some()
            {
                frames += 1;
            }
        }
        assert_eq!(frames, 2);
    }

    #[test]
    fn losing_the_relay_flushes_held_events_first() {
        let mut batcher = FrameBatcher::new();
        batcher.push(block(1, BASE_SLOT, SyncState::CatchingUp), 0);
        let frames = batcher.upstream_lost(10);
        assert!(matches!(frames[0], HeartbeatFrame::Events { ref events } if events.len() == 1));
        assert_eq!(frames[1], HeartbeatFrame::UpstreamLost);
        assert_eq!(
            batcher.upstream_lost(20),
            vec![HeartbeatFrame::UpstreamLost]
        );
    }

    #[test]
    fn a_subscriber_folds_to_the_same_chain_as_its_host() {
        let mut host = Heartbeat::new(Network::Mainnet);
        let mut batcher = FrameBatcher::new();
        let t0 = ms_at(BASE_SLOT);
        host.connected(t0);

        // A subscriber that joins after three blocks gets a resync.
        for i in 0..3 {
            let event = block(i, BASE_SLOT + i * 20, SyncState::AtTip);
            host.apply(&event, ms_at(BASE_SLOT + i * 20));
            batcher.push(event, ms_at(BASE_SLOT + i * 20));
        }
        let joined = ms_at(BASE_SLOT + 45);
        let mut subscriber = Heartbeat::new(Network::Mainnet);
        assert_eq!(
            subscriber.apply_frame(&host.resync_frame(joined), joined),
            FrameApplied::Applied
        );

        // Then frames as they happen, including a rollback and a replacement.
        let rollback = ChainEvent::RollBackward {
            to: Some(ChainPoint {
                slot: BASE_SLOT + 20,
                hash: [0; 32],
            }),
        };
        let replacement = block(2, BASE_SLOT + 61, SyncState::AtTip);
        let now = ms_at(BASE_SLOT + 61);
        for event in [rollback, replacement] {
            host.apply(&event, now);
            if let Some(frame) = batcher.push(event, now) {
                subscriber.apply_frame(&frame, now);
            }
        }

        let later = now + 5_000;
        let (host_view, subscriber_view) = (host.snapshot(later), subscriber.snapshot(later));
        assert_eq!(subscriber_view.tip, host_view.tip);
        assert_eq!(subscriber_view.window, host_view.window);
        assert!(subscriber_view.block_due_probability.is_some());
    }

    #[test]
    fn a_resync_carries_the_hosts_silence() {
        let mut subscriber = Heartbeat::new(Network::Mainnet);
        let frame = HeartbeatFrame::Resync {
            checkpoint: Heartbeat::new(Network::Mainnet).checkpoint(),
            feed: FeedHealth::Silent { silent_secs: 120 },
            recent_txs: Vec::new(),
        };
        subscriber.apply_frame(&frame, 1_000_000);
        assert_eq!(
            subscriber.snapshot(1_005_000).feed,
            FeedHealth::Silent { silent_secs: 125 }
        );

        let preprod = HeartbeatFrame::Resync {
            checkpoint: Heartbeat::new(Network::Preprod).checkpoint(),
            feed: FeedHealth::NotStarted,
            recent_txs: Vec::new(),
        };
        assert_eq!(
            subscriber.apply_frame(&preprod, 0),
            FrameApplied::WrongNetwork
        );

        subscriber.apply_frame(&HeartbeatFrame::UpstreamLost, 2_000_000);
        assert!(matches!(
            subscriber.snapshot(2_000_000).feed,
            FeedHealth::Disconnected { .. }
        ));
    }

    #[test]
    fn frames_round_trip_through_json() {
        let frame = HeartbeatFrame::Events {
            events: vec![
                ChainEvent::Connected { version: 14 },
                block(9, BASE_SLOT, SyncState::CatchingUp),
                ChainEvent::RollBackward {
                    to: Some(ChainPoint {
                        slot: BASE_SLOT,
                        hash: [0xAB; 32],
                    }),
                },
                ChainEvent::RollBackward { to: None },
                ChainEvent::KeepAliveAcknowledged,
            ],
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains(r#""type":"roll_backward""#), "{json}");
        assert!(json.contains(&"ab".repeat(32)), "{json}");
        let back: HeartbeatFrame = serde_json::from_str(&json).unwrap();
        assert_eq!(back, frame);
    }
}
