//! Poll backends on block arrival, but only while waiting on something.
//!
//! A page that polls on every block does two bad things: it asks when it has
//! nothing to learn, and it synchronises every open tab onto the same second,
//! every ~20 s. [`BlockPoller`] fixes both.
//!
//! - **Only while awaiting.** The host registers what it is waiting on (a
//!   submitted transaction hash, say) with [`BlockPoller::await_work`] and
//!   clears it with [`BlockPoller::resolve`] when the answer arrives. With
//!   nothing awaited, blocks schedule nothing.
//! - **Spread out.** A poll lands [`PollTiming::settle`] plus a random slice of
//!   [`PollTiming::jitter`] after the block. Tabs fan out across that window,
//!   and indexers that trail the relay by a few seconds have time to catch up.
//! - **Coalesced.** Several blocks (or a replay after reconnecting) before the
//!   poll fires still produce one poll. Catch-up blocks schedule nothing; the
//!   block that reaches the tip does.
//! - **Never dependent on the feed.** While work is awaited and no poll has
//!   fired for [`PollTiming::fallback`], it polls anyway, so a dead heartbeat
//!   slows confirmation down rather than hiding it.
//!
//! Pure: no clock, no RNG, no runtime. The host passes `now_ms`, and entropy
//! from whatever it has (`js_sys::Math::random`, `rand`, a hash).

use std::collections::BTreeSet;
use std::time::Duration;

use crate::beat::{ChainEvent, SyncState};
use crate::frame::HeartbeatFrame;

/// How long to wait before polling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollTiming {
    /// Minimum wait after a block. Indexers (Koios, a DO fed by mitos) trail
    /// the relay by a few seconds, so polling the instant a block lands often
    /// reads the state from before it.
    pub settle: Duration,
    /// Extra wait, uniform in `0..=jitter`, drawn per poll.
    pub jitter: Duration,
    /// Poll anyway after this long without a poll while work is awaited.
    pub fallback: Duration,
}

impl Default for PollTiming {
    fn default() -> Self {
        Self {
            settle: Duration::from_secs(2),
            jitter: Duration::from_secs(4),
            fallback: Duration::from_secs(60),
        }
    }
}

/// What the host should do now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollDecision {
    /// Nothing is awaited. There is no reason to poll or to wake up.
    Idle,
    /// Work is awaited. Ask again within `in_ms` (a repaint timer, say).
    Wait { in_ms: u64 },
    /// Poll now. The poller has already recorded the poll.
    Poll { reason: PollReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollReason {
    /// A block reached the tip.
    NewBlock,
    /// Blocks were orphaned: something already shown may no longer be on chain.
    Rollback,
    /// No block-driven poll for a whole fallback interval.
    Fallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Due {
    at_ms: u64,
    reason: PollReason,
}

/// Decides when to poll, keyed by whatever the host is waiting on.
#[derive(Debug, Clone)]
pub struct BlockPoller<K> {
    timing: PollTiming,
    awaiting: BTreeSet<K>,
    tip_slot: Option<u64>,
    due: Option<Due>,
    last_poll_ms: u64,
}

impl<K: Ord> BlockPoller<K> {
    pub fn new(timing: PollTiming) -> Self {
        Self {
            timing,
            awaiting: BTreeSet::new(),
            tip_slot: None,
            due: None,
            last_poll_ms: 0,
        }
    }

    /// Start waiting on `key`. The fallback clock starts when the first key is
    /// added, not at construction.
    pub fn await_work(&mut self, key: K, now_ms: u64) {
        if self.awaiting.is_empty() {
            self.last_poll_ms = now_ms;
        }
        self.awaiting.insert(key);
    }

    /// Stop waiting on `key`. Resolving the last key cancels any scheduled poll.
    pub fn resolve(&mut self, key: &K) {
        self.awaiting.remove(key);
        if self.awaiting.is_empty() {
            self.due = None;
        }
    }

    pub fn awaiting(&self) -> impl Iterator<Item = &K> {
        self.awaiting.iter()
    }

    pub fn awaiting_count(&self) -> usize {
        self.awaiting.len()
    }

    /// Feed every chain event through here. `entropy` only matters when this
    /// event schedules a poll; any `u64` from a random source will do.
    pub fn observe(&mut self, event: &ChainEvent, now_ms: u64, entropy: u64) {
        let reason = match event {
            ChainEvent::RollForward { beat, sync } => {
                self.tip_slot = Some(beat.slot);
                match sync {
                    SyncState::AtTip => Some(PollReason::NewBlock),
                    SyncState::CatchingUp => None,
                }
            }
            ChainEvent::RollBackward { to } => {
                let to_slot = to.map(|point| point.slot);
                let orphaned = match (to_slot, self.tip_slot) {
                    (Some(to), Some(tip)) => to < tip,
                    (None, Some(_)) => true,
                    // Nothing seen yet: this is the echo every intersect sends.
                    (_, None) => false,
                };
                if orphaned {
                    self.tip_slot = to_slot;
                    Some(PollReason::Rollback)
                } else {
                    None
                }
            }
            ChainEvent::Connected { .. } | ChainEvent::KeepAliveAcknowledged => None,
        };

        if let Some(reason) = reason {
            self.schedule(reason, now_ms, entropy);
        }
    }

    /// Feed every frame from a heartbeat host through here, for subscribers
    /// that receive the chain relayed rather than following it.
    pub fn observe_frame(&mut self, frame: &HeartbeatFrame, now_ms: u64, entropy: u64) {
        match frame {
            HeartbeatFrame::Events { events } => {
                for event in events {
                    self.observe(event, now_ms, entropy);
                }
            }
            HeartbeatFrame::Resync { checkpoint, .. } => {
                let Some(tip) = checkpoint.beats.last().map(|beat| beat.slot) else {
                    return;
                };
                // A resync after a gap (our socket dropped and came back) may
                // carry blocks we never saw. The first one is just where we
                // start from.
                let reason = match self.tip_slot {
                    Some(known) if tip > known => Some(PollReason::NewBlock),
                    Some(known) if tip < known => Some(PollReason::Rollback),
                    _ => None,
                };
                self.tip_slot = Some(tip);
                if let Some(reason) = reason {
                    self.schedule(reason, now_ms, entropy);
                }
            }
            HeartbeatFrame::UpstreamLost => {}
        }
    }

    fn schedule(&mut self, reason: PollReason, now_ms: u64, entropy: u64) {
        // Nothing to learn, or a poll is already on its way: keep the earlier
        // time rather than pushing everyone later on every block.
        if self.awaiting.is_empty() || self.due.is_some() {
            return;
        }
        self.due = Some(Due {
            at_ms: now_ms + self.delay_ms(entropy),
            reason,
        });
    }

    /// Call on every frame or timer tick while anything is awaited.
    pub fn poll(&mut self, now_ms: u64) -> PollDecision {
        if self.awaiting.is_empty() {
            self.due = None;
            return PollDecision::Idle;
        }
        let fallback_at = self.last_poll_ms + millis(self.timing.fallback);
        let (at_ms, reason) = match self.due {
            Some(due) if due.at_ms <= fallback_at => (due.at_ms, due.reason),
            _ => (fallback_at, PollReason::Fallback),
        };
        if now_ms >= at_ms {
            self.due = None;
            self.last_poll_ms = now_ms;
            PollDecision::Poll { reason }
        } else {
            PollDecision::Wait {
                in_ms: at_ms - now_ms,
            }
        }
    }

    fn delay_ms(&self, entropy: u64) -> u64 {
        millis(self.timing.settle) + entropy % (millis(self.timing.jitter) + 1)
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beat::{BlockBeat, ChainPoint};

    const TIMING: PollTiming = PollTiming {
        settle: Duration::from_millis(2_000),
        jitter: Duration::from_millis(4_000),
        fallback: Duration::from_millis(60_000),
    };

    fn block(slot: u64, sync: SyncState) -> ChainEvent {
        ChainEvent::RollForward {
            beat: BlockBeat {
                height: slot,
                slot,
                hash: format!("{slot:064x}"),
                issuer_pool: "00".repeat(28),
                body_size: 0,
                tx_count: None,
                block_time_unix: None,
            },
            sync,
        }
    }

    fn rollback_to(slot: u64) -> ChainEvent {
        ChainEvent::RollBackward {
            to: Some(ChainPoint {
                slot,
                hash: [0; 32],
            }),
        }
    }

    #[test]
    fn blocks_schedule_nothing_while_nothing_is_awaited() {
        let mut poller = BlockPoller::<&str>::new(TIMING);
        poller.observe(&block(100, SyncState::AtTip), 0, 7);
        assert_eq!(poller.poll(10_000), PollDecision::Idle);
        assert_eq!(poller.poll(1_000_000), PollDecision::Idle);
    }

    #[test]
    fn a_block_schedules_one_jittered_poll() {
        let mut poller = BlockPoller::new(TIMING);
        poller.await_work("tx-a", 0);
        poller.observe(&block(100, SyncState::AtTip), 1_000, 1_500);

        // settle 2,000 + jitter 1,500 after the block.
        assert_eq!(poller.poll(1_000), PollDecision::Wait { in_ms: 3_500 });
        assert_eq!(
            poller.poll(4_500),
            PollDecision::Poll {
                reason: PollReason::NewBlock
            }
        );
        // Consumed: the next wake is the fallback, measured from this poll.
        assert_eq!(poller.poll(4_600), PollDecision::Wait { in_ms: 59_900 });
    }

    #[test]
    fn jitter_stays_inside_the_window() {
        for entropy in [0, 1, 3_999, 4_000, 4_001, u64::MAX] {
            let mut poller = BlockPoller::new(TIMING);
            poller.await_work(1, 0);
            poller.observe(&block(100, SyncState::AtTip), 0, entropy);
            let PollDecision::Wait { in_ms } = poller.poll(0) else {
                panic!("expected a wait");
            };
            assert!((2_000..=6_000).contains(&in_ms), "{entropy} -> {in_ms}");
        }
    }

    #[test]
    fn a_burst_of_blocks_coalesces_into_the_first_schedule() {
        let mut poller = BlockPoller::new(TIMING);
        poller.await_work("tx-a", 0);
        // A reconnect replays blocks: none of them schedule.
        for slot in 100..130 {
            poller.observe(&block(slot, SyncState::CatchingUp), 0, 0);
        }
        assert_eq!(poller.poll(10), PollDecision::Wait { in_ms: 59_990 });

        // The block that reaches the tip does, and a second one doesn't move it.
        poller.observe(&block(130, SyncState::AtTip), 100, 0);
        poller.observe(&block(131, SyncState::AtTip), 1_000, 4_000);
        assert_eq!(poller.poll(100), PollDecision::Wait { in_ms: 2_000 });
        assert!(matches!(poller.poll(2_100), PollDecision::Poll { .. }));
    }

    #[test]
    fn only_an_orphaning_rollback_schedules() {
        let mut poller = BlockPoller::new(TIMING);
        poller.await_work("tx-a", 0);

        // The echo after an intersect, before any block was seen.
        poller.observe(&rollback_to(500), 0, 0);
        assert_eq!(poller.poll(0), PollDecision::Wait { in_ms: 60_000 });

        poller.observe(&block(600, SyncState::CatchingUp), 0, 0);
        // Rolling back to the tip itself orphans nothing.
        poller.observe(&rollback_to(600), 0, 0);
        assert_eq!(poller.poll(0), PollDecision::Wait { in_ms: 60_000 });

        poller.observe(&rollback_to(580), 0, 0);
        assert_eq!(
            poller.poll(2_000),
            PollDecision::Poll {
                reason: PollReason::Rollback
            }
        );
    }

    #[test]
    fn a_dead_feed_still_polls_on_the_fallback() {
        let mut poller = BlockPoller::new(TIMING);
        poller.await_work("tx-a", 5_000);
        assert_eq!(poller.poll(64_999), PollDecision::Wait { in_ms: 1 });
        assert_eq!(
            poller.poll(65_000),
            PollDecision::Poll {
                reason: PollReason::Fallback
            }
        );
    }

    #[test]
    fn work_added_after_a_block_waits_for_the_next_one() {
        let mut poller = BlockPoller::new(TIMING);
        poller.observe(&block(100, SyncState::AtTip), 0, 0);
        // A transaction submitted now cannot be in the block that already landed.
        poller.await_work("tx-a", 1_000);
        assert_eq!(poller.poll(3_000), PollDecision::Wait { in_ms: 58_000 });
    }

    #[test]
    fn resolving_the_last_key_cancels_the_scheduled_poll() {
        let mut poller = BlockPoller::new(TIMING);
        poller.await_work("tx-a", 0);
        poller.await_work("tx-b", 0);
        poller.observe(&block(100, SyncState::AtTip), 0, 0);

        poller.resolve(&"tx-a");
        assert!(matches!(poller.poll(2_000), PollDecision::Poll { .. }));

        poller.observe(&block(101, SyncState::AtTip), 3_000, 0);
        poller.resolve(&"tx-b");
        assert_eq!(poller.poll(10_000), PollDecision::Idle);
        assert_eq!(poller.awaiting_count(), 0);
    }

    fn resync_at(slot: u64) -> HeartbeatFrame {
        let crate::beat::ChainEvent::RollForward { beat, .. } = block(slot, SyncState::AtTip)
        else {
            unreachable!()
        };
        HeartbeatFrame::Resync {
            checkpoint: crate::heartbeat::Checkpoint {
                network: crate::network::Network::Mainnet,
                beats: vec![beat],
            },
            feed: crate::heartbeat::FeedHealth::NotStarted,
        }
    }

    #[test]
    fn a_first_resync_is_a_starting_point_not_a_change() {
        let mut poller = BlockPoller::new(TIMING);
        poller.await_work("tx-a", 0);
        poller.observe_frame(&resync_at(100), 0, 0);
        assert_eq!(poller.poll(10), PollDecision::Wait { in_ms: 59_990 });
    }

    #[test]
    fn a_resync_after_a_gap_polls_for_what_was_missed() {
        let mut poller = BlockPoller::new(TIMING);
        poller.await_work("tx-a", 0);
        poller.observe_frame(&resync_at(100), 0, 0);
        poller.observe_frame(&resync_at(160), 1_000, 0);
        assert_eq!(
            poller.poll(3_000),
            PollDecision::Poll {
                reason: PollReason::NewBlock
            }
        );
        poller.observe_frame(&resync_at(120), 4_000, 0);
        assert_eq!(
            poller.poll(6_000),
            PollDecision::Poll {
                reason: PollReason::Rollback
            }
        );
    }

    #[test]
    fn relayed_events_schedule_like_followed_ones() {
        let mut poller = BlockPoller::new(TIMING);
        poller.await_work("tx-a", 0);
        let frame = HeartbeatFrame::Events {
            events: vec![
                block(100, SyncState::CatchingUp),
                block(101, SyncState::AtTip),
            ],
        };
        poller.observe_frame(&frame, 0, 0);
        assert!(matches!(poller.poll(2_000), PollDecision::Poll { .. }));
    }
}
