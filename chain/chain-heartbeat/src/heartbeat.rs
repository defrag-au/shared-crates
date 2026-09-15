//! Fold chain events into a snapshot of the chain's tempo.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::beat::{BlockBeat, ChainEvent, ChainPoint, SyncState};
use crate::network::{EpochPosition, Network, block_probability_within};

/// Beats kept: about 85 minutes of chain at the 20 s average.
pub const DEFAULT_CAPACITY: usize = 256;

/// A connected feed with no traffic at all (not even a keep-alive answer) for
/// this long is reported as `Silent`. Three missed 30 s keep-alives.
pub const SILENT_AFTER_SECS: u64 = 90;

const HOUR_SECS: u64 = 3_600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Feed {
    NotStarted,
    Connected {
        since_ms: u64,
        last_activity_ms: u64,
        sync: SyncState,
    },
    Disconnected {
        since_ms: u64,
    },
}

/// Rolling state for one network.
#[derive(Debug, Clone)]
pub struct Heartbeat {
    network: Network,
    capacity: usize,
    beats: VecDeque<BlockBeat>,
    feed: Feed,
}

/// The persistable part of a [`Heartbeat`]. Connection state is deliberately
/// not included: a restored heartbeat is `NotStarted` until its host connects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub network: Network,
    pub beats: Vec<BlockBeat>,
}

/// What [`Heartbeat::restore`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restore {
    Restored {
        beats: usize,
    },
    /// The checkpoint was for another network and was ignored.
    WrongNetwork,
}

/// The state of the feed, as distinct from the state of the chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum FeedHealth {
    /// The host has not connected since it started.
    NotStarted,
    /// Connected and hearing from the peer.
    Following {
        #[serde(with = "wasm_safe_serde::u64_required")]
        connected_secs: u64,
        sync: SyncState,
    },
    /// Connected, but nothing has arrived for at least [`SILENT_AFTER_SECS`].
    /// Treat the chain's state as unknown, not as quiet.
    Silent {
        #[serde(with = "wasm_safe_serde::u64_required")]
        silent_secs: u64,
    },
    Disconnected {
        #[serde(with = "wasm_safe_serde::u64_required")]
        disconnected_secs: u64,
    },
}

/// Rates over the contiguous run of blocks ending at the tip.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowStats {
    /// Blocks in the run.
    pub blocks: u32,
    /// Seconds from the first block of the run to the tip.
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub span_secs: u64,
    #[serde(default)]
    pub mean_interval_secs: Option<f32>,
    /// Blocks in the hour ending at the tip. `None` until the run covers an hour.
    #[serde(default)]
    pub blocks_last_hour: Option<u32>,
    /// `None` unless every block in the run carries a transaction count.
    #[serde(default)]
    pub txs_per_minute: Option<f32>,
    /// Mean body size as a fraction of [`Network::max_block_body_bytes`].
    #[serde(default)]
    pub mean_fullness: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeartbeatSnapshot {
    pub network: Network,
    pub feed: FeedHealth,
    #[serde(default)]
    pub tip: Option<BlockBeat>,
    /// Seconds since the tip block's slot began, by the host's clock.
    #[serde(default, with = "wasm_safe_serde::u64_option")]
    pub secs_since_block: Option<u64>,
    /// Likelihood a block would have landed by now. Only reported while
    /// `Following` at the tip; a silent or catching-up feed cannot say.
    #[serde(default)]
    pub block_due_probability: Option<f32>,
    pub window: WindowStats,
    #[serde(default)]
    pub epoch: Option<EpochPosition>,
}

impl Heartbeat {
    pub fn new(network: Network) -> Self {
        Self::with_capacity(network, DEFAULT_CAPACITY)
    }

    pub fn with_capacity(network: Network, capacity: usize) -> Self {
        let capacity = capacity.max(2);
        Self {
            network,
            capacity,
            beats: VecDeque::with_capacity(capacity),
            feed: Feed::NotStarted,
        }
    }

    pub fn network(&self) -> Network {
        self.network
    }

    pub fn tip(&self) -> Option<&BlockBeat> {
        self.beats.back()
    }

    pub fn apply(&mut self, event: &ChainEvent, now_ms: u64) {
        match event {
            ChainEvent::Connected { .. } => self.connected(now_ms),
            ChainEvent::RollForward { beat, sync } => {
                self.roll_forward(beat.clone(), *sync, now_ms)
            }
            ChainEvent::RollBackward { to } => self.roll_back(to.as_ref(), now_ms),
            ChainEvent::KeepAliveAcknowledged => self.touch(now_ms),
        }
    }

    pub fn connected(&mut self, now_ms: u64) {
        self.feed = Feed::Connected {
            since_ms: now_ms,
            last_activity_ms: now_ms,
            sync: SyncState::CatchingUp,
        };
    }

    /// The host lost (or closed) the connection.
    pub fn disconnected(&mut self, now_ms: u64) {
        if !matches!(self.feed, Feed::Disconnected { .. }) {
            self.feed = Feed::Disconnected { since_ms: now_ms };
        }
    }

    /// Take on another heartbeat's view of its feed, measured back from
    /// `now_ms`. How a subscriber adopts its host's feed on resync.
    pub(crate) fn adopt_feed(&mut self, feed: FeedHealth, now_ms: u64) {
        let ago = |secs: u64| now_ms.saturating_sub(secs * 1000);
        self.feed = match feed {
            FeedHealth::NotStarted => Feed::NotStarted,
            FeedHealth::Following {
                connected_secs,
                sync,
            } => Feed::Connected {
                since_ms: ago(connected_secs),
                last_activity_ms: now_ms,
                sync,
            },
            FeedHealth::Silent { silent_secs } => Feed::Connected {
                since_ms: ago(silent_secs),
                last_activity_ms: ago(silent_secs),
                sync: SyncState::AtTip,
            },
            FeedHealth::Disconnected { disconnected_secs } => Feed::Disconnected {
                since_ms: ago(disconnected_secs),
            },
        };
    }

    pub fn roll_forward(&mut self, beat: BlockBeat, sync: SyncState, now_ms: u64) {
        self.touch(now_ms);
        if let Feed::Connected { sync: current, .. } = &mut self.feed {
            *current = sync;
        }
        // A height at or below the tip replaces what it supersedes. Chain-sync
        // always sends the rollback first, so this only guards against a host
        // that feeds beats from elsewhere.
        while self.beats.back().is_some_and(|b| b.height >= beat.height) {
            self.beats.pop_back();
        }
        self.beats.push_back(beat);
        while self.beats.len() > self.capacity {
            self.beats.pop_front();
        }
    }

    pub fn roll_back(&mut self, to: Option<&ChainPoint>, now_ms: u64) {
        self.touch(now_ms);
        match to {
            None => self.beats.clear(),
            Some(point) => {
                while self.beats.back().is_some_and(|b| b.slot > point.slot) {
                    self.beats.pop_back();
                }
            }
        }
    }

    /// Up to `count` recent points, newest first, to intersect from after a
    /// reconnect so the blocks missed meanwhile are replayed.
    pub fn resume_points(&self, count: usize) -> Vec<ChainPoint> {
        self.beats
            .iter()
            .rev()
            .filter_map(BlockBeat::point)
            .take(count)
            .collect()
    }

    /// [`Self::resume_points`], but only while the tip is younger than
    /// `max_age_secs`. Resuming from an old tip replays every block since,
    /// which costs more than the gap is worth; starting from the peer's tip
    /// instead leaves a gap in heights, and the window honestly restarts.
    pub fn fresh_resume_points(
        &self,
        now_ms: u64,
        max_age_secs: u64,
        count: usize,
    ) -> Vec<ChainPoint> {
        let fresh = self
            .beats
            .back()
            .and_then(|tip| tip.block_time_unix)
            .is_some_and(|t| (now_ms / 1000).saturating_sub(t) <= max_age_secs);
        if fresh {
            self.resume_points(count)
        } else {
            Vec::new()
        }
    }

    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            network: self.network,
            beats: self.beats.iter().cloned().collect(),
        }
    }

    pub fn restore(&mut self, checkpoint: Checkpoint) -> Restore {
        if checkpoint.network != self.network {
            return Restore::WrongNetwork;
        }
        let skip = checkpoint.beats.len().saturating_sub(self.capacity);
        self.beats = checkpoint.beats.into_iter().skip(skip).collect();
        Restore::Restored {
            beats: self.beats.len(),
        }
    }

    pub fn snapshot(&self, now_ms: u64) -> HeartbeatSnapshot {
        let feed = self.feed_health(now_ms);
        let tip = self.beats.back().cloned();
        let secs_since_block = tip
            .as_ref()
            .and_then(|t| t.block_time_unix)
            .map(|t| (now_ms / 1000).saturating_sub(t));
        let block_due_probability = match (feed, secs_since_block) {
            (
                FeedHealth::Following {
                    sync: SyncState::AtTip,
                    ..
                },
                Some(secs),
            ) => Some(block_probability_within(secs) as f32),
            _ => None,
        };
        HeartbeatSnapshot {
            network: self.network,
            feed,
            epoch: tip
                .as_ref()
                .and_then(|t| self.network.epoch_position(t.slot)),
            tip,
            secs_since_block,
            block_due_probability,
            window: self.window(),
        }
    }

    fn touch(&mut self, now_ms: u64) {
        match &mut self.feed {
            Feed::Connected {
                last_activity_ms, ..
            } => *last_activity_ms = now_ms,
            // Traffic without a Connected event means the host skipped it;
            // the traffic itself is the better evidence.
            Feed::NotStarted | Feed::Disconnected { .. } => self.connected(now_ms),
        }
    }

    fn feed_health(&self, now_ms: u64) -> FeedHealth {
        let secs = |since: u64| now_ms.saturating_sub(since) / 1000;
        match self.feed {
            Feed::NotStarted => FeedHealth::NotStarted,
            Feed::Disconnected { since_ms } => FeedHealth::Disconnected {
                disconnected_secs: secs(since_ms),
            },
            Feed::Connected {
                since_ms,
                last_activity_ms,
                sync,
            } => {
                let silent_secs = secs(last_activity_ms);
                if silent_secs >= SILENT_AFTER_SECS {
                    FeedHealth::Silent { silent_secs }
                } else {
                    FeedHealth::Following {
                        connected_secs: secs(since_ms),
                        sync,
                    }
                }
            }
        }
    }

    /// The run of consecutive heights ending at the tip.
    fn contiguous_run(&self) -> &[BlockBeat] {
        let beats = self.beats.as_slices();
        // `beats` is only ever appended at the back and trimmed at the front,
        // so make it contiguous in memory once rather than copying per call.
        let all: &[BlockBeat] = if beats.1.is_empty() {
            beats.0
        } else {
            return self.contiguous_run_slow();
        };
        let mut start = all.len();
        while start > 0 {
            let candidate = start - 1;
            if start < all.len() && all[candidate].height + 1 != all[start].height {
                break;
            }
            start = candidate;
        }
        &all[start..]
    }

    fn contiguous_run_slow(&self) -> &[BlockBeat] {
        // Only reached when the ring has wrapped; `make_contiguous` needs
        // `&mut`, so fall back to the longest contiguous tail of the back slice
        // joined with the front slice when they line up.
        let (front, back) = self.beats.as_slices();
        let mut start = back.len();
        while start > 0 {
            let candidate = start - 1;
            if start < back.len() && back[candidate].height + 1 != back[start].height {
                return &back[start..];
            }
            start = candidate;
        }
        match (front.last(), back.first()) {
            (Some(last), Some(first)) if last.height + 1 == first.height => {
                // The run continues into `front`, but a slice cannot span both
                // halves. The back half alone is still a correct (shorter) run.
                back
            }
            _ => back,
        }
    }

    fn window(&self) -> WindowStats {
        let run = self.contiguous_run();
        let (Some(first), Some(tip)) = (run.first(), run.last()) else {
            return WindowStats {
                blocks: 0,
                span_secs: 0,
                mean_interval_secs: None,
                blocks_last_hour: None,
                txs_per_minute: None,
                mean_fullness: None,
            };
        };
        let blocks = run.len() as u32;
        let span_secs = tip.slot.saturating_sub(first.slot);
        let mean_interval_secs = (blocks >= 2).then(|| span_secs as f32 / (blocks - 1) as f32);
        let blocks_last_hour = (span_secs >= HOUR_SECS).then(|| {
            let from = tip.slot - HOUR_SECS;
            run.iter().filter(|b| b.slot > from).count() as u32
        });
        let txs_per_minute = if blocks >= 2 && span_secs > 0 {
            run[1..]
                .iter()
                .map(|b| b.tx_count)
                .sum::<Option<u32>>()
                .map(|txs| txs as f32 * 60.0 / span_secs as f32)
        } else {
            None
        };
        let max_body = self.network.max_block_body_bytes() as f32;
        let mean_fullness = Some(
            run.iter()
                .map(|b| b.body_size as f32 / max_body)
                .sum::<f32>()
                / blocks as f32,
        );
        WindowStats {
            blocks,
            span_secs,
            mean_interval_secs,
            blocks_last_hour,
            txs_per_minute,
            mean_fullness,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mainnet beat at `height`, 20 s after the previous one.
    fn beat(height: u64, slot: u64, txs: Option<u32>) -> BlockBeat {
        BlockBeat {
            height,
            slot,
            hash: format!("{height:064x}"),
            issuer_pool: "00".repeat(28),
            body_size: 45_056,
            tx_count: txs,
            block_time_unix: Network::Mainnet.slot_to_unix_secs(slot),
        }
    }

    const BASE_SLOT: u64 = 186_000_000;

    fn ms_at_slot(slot: u64) -> u64 {
        Network::Mainnet.slot_to_unix_secs(slot).unwrap() * 1000
    }

    #[test]
    fn a_fresh_heartbeat_reports_nothing_it_does_not_know() {
        let hb = Heartbeat::new(Network::Mainnet);
        let snap = hb.snapshot(1_000);
        assert_eq!(snap.feed, FeedHealth::NotStarted);
        assert_eq!(snap.tip, None);
        assert_eq!(snap.block_due_probability, None);
        assert_eq!(snap.window.blocks, 0);
    }

    #[test]
    fn rates_come_from_the_blocks_seen() {
        let mut hb = Heartbeat::new(Network::Mainnet);
        let start = ms_at_slot(BASE_SLOT);
        hb.connected(start);
        for i in 0..4 {
            let slot = BASE_SLOT + i * 20;
            hb.roll_forward(
                beat(100 + i, slot, Some(6)),
                SyncState::AtTip,
                ms_at_slot(slot),
            );
        }
        let now = ms_at_slot(BASE_SLOT + 60) + 10_000;
        let snap = hb.snapshot(now);

        assert_eq!(snap.tip.as_ref().unwrap().height, 103);
        assert_eq!(snap.secs_since_block, Some(10));
        assert_eq!(snap.window.blocks, 4);
        assert_eq!(snap.window.span_secs, 60);
        assert_eq!(snap.window.mean_interval_secs, Some(20.0));
        // 18 transactions arrived over 60 seconds.
        assert_eq!(snap.window.txs_per_minute, Some(18.0));
        assert_eq!(snap.window.mean_fullness, Some(0.5));
        assert_eq!(
            snap.window.blocks_last_hour, None,
            "run is shorter than an hour"
        );
        let p = snap.block_due_probability.unwrap();
        assert!((0.40..0.41).contains(&p), "{p}");
        assert_eq!(snap.epoch.unwrap().epoch, 628);
    }

    #[test]
    fn a_gap_in_heights_cuts_the_window() {
        let mut hb = Heartbeat::new(Network::Mainnet);
        hb.roll_forward(beat(1, BASE_SLOT, None), SyncState::AtTip, 0);
        hb.roll_forward(beat(2, BASE_SLOT + 20, None), SyncState::AtTip, 0);
        hb.roll_forward(beat(9, BASE_SLOT + 400, None), SyncState::AtTip, 0);
        hb.roll_forward(beat(10, BASE_SLOT + 420, None), SyncState::AtTip, 0);
        let window = hb.snapshot(0).window;
        assert_eq!(window.blocks, 2);
        assert_eq!(window.span_secs, 20);
        assert_eq!(
            window.txs_per_minute, None,
            "headers-only beats carry no count"
        );
    }

    #[test]
    fn rollbacks_remove_orphaned_blocks() {
        let mut hb = Heartbeat::new(Network::Mainnet);
        for i in 0..5 {
            hb.roll_forward(beat(i, BASE_SLOT + i * 20, None), SyncState::AtTip, 0);
        }
        let to = beat(2, BASE_SLOT + 40, None).point().unwrap();
        hb.roll_back(Some(&to), 0);
        assert_eq!(hb.tip().unwrap().height, 2);

        // A replacement block at an existing height supersedes the old one,
        // and the run 0, 1, 2 stays contiguous.
        hb.roll_forward(beat(2, BASE_SLOT + 41, None), SyncState::AtTip, 0);
        assert_eq!(hb.tip().unwrap().slot, BASE_SLOT + 41);
        assert_eq!(hb.snapshot(0).window.blocks, 3);

        hb.roll_back(None, 0);
        assert_eq!(hb.tip(), None);
    }

    #[test]
    fn a_silent_feed_is_not_reported_as_a_quiet_chain() {
        let mut hb = Heartbeat::new(Network::Mainnet);
        let t0 = ms_at_slot(BASE_SLOT);
        hb.connected(t0);
        hb.roll_forward(beat(1, BASE_SLOT, None), SyncState::AtTip, t0);

        let still_following = hb.snapshot(t0 + 60_000);
        assert!(matches!(still_following.feed, FeedHealth::Following { .. }));
        assert!(still_following.block_due_probability.is_some());

        let silent = hb.snapshot(t0 + SILENT_AFTER_SECS * 1000);
        assert_eq!(
            silent.feed,
            FeedHealth::Silent {
                silent_secs: SILENT_AFTER_SECS
            }
        );
        assert_eq!(silent.block_due_probability, None);

        // A keep-alive answer proves the connection even with no block.
        hb.apply(&ChainEvent::KeepAliveAcknowledged, t0 + 80_000);
        assert!(matches!(
            hb.snapshot(t0 + 120_000).feed,
            FeedHealth::Following { .. }
        ));

        hb.disconnected(t0 + 130_000);
        assert_eq!(
            hb.snapshot(t0 + 135_000).feed,
            FeedHealth::Disconnected {
                disconnected_secs: 5
            }
        );
    }

    #[test]
    fn catching_up_reports_no_due_probability() {
        let mut hb = Heartbeat::new(Network::Mainnet);
        let t0 = ms_at_slot(BASE_SLOT);
        hb.connected(t0);
        hb.roll_forward(beat(1, BASE_SLOT, None), SyncState::CatchingUp, t0);
        let snap = hb.snapshot(t0 + 5_000);
        assert_eq!(
            snap.feed,
            FeedHealth::Following {
                connected_secs: 5,
                sync: SyncState::CatchingUp
            }
        );
        assert_eq!(snap.block_due_probability, None);
    }

    #[test]
    fn capacity_bounds_memory_and_checkpoints_round_trip() {
        let mut hb = Heartbeat::with_capacity(Network::Mainnet, 3);
        for i in 0..5 {
            hb.roll_forward(beat(i, BASE_SLOT + i * 20, None), SyncState::AtTip, 0);
        }
        assert_eq!(hb.checkpoint().beats.len(), 3);
        assert_eq!(
            hb.resume_points(2)
                .iter()
                .map(|p| p.slot)
                .collect::<Vec<_>>(),
            vec![BASE_SLOT + 80, BASE_SLOT + 60]
        );

        let json = serde_json::to_string(&hb.checkpoint()).unwrap();
        let mut restored = Heartbeat::with_capacity(Network::Mainnet, 3);
        assert_eq!(
            restored.restore(serde_json::from_str(&json).unwrap()),
            Restore::Restored { beats: 3 }
        );
        assert_eq!(restored.tip(), hb.tip());
        assert_eq!(restored.snapshot(0).feed, FeedHealth::NotStarted);

        let mut other = Heartbeat::new(Network::Preprod);
        assert_eq!(other.restore(hb.checkpoint()), Restore::WrongNetwork);
    }

    #[test]
    fn stale_tips_start_from_the_peer_tip_instead_of_replaying() {
        let mut hb = Heartbeat::new(Network::Mainnet);
        hb.roll_forward(beat(1, BASE_SLOT, None), SyncState::AtTip, 0);
        let tip_ms = ms_at_slot(BASE_SLOT);
        assert_eq!(hb.fresh_resume_points(tip_ms + 600_000, 1_800, 4).len(), 1);
        assert!(
            hb.fresh_resume_points(tip_ms + 1_801_000, 1_800, 4)
                .is_empty()
        );
    }

    #[test]
    fn an_hour_long_run_counts_its_last_hour() {
        let mut hb = Heartbeat::new(Network::Mainnet);
        // 200 blocks at 20 s: a span of 3,980 s.
        for i in 0..200 {
            hb.roll_forward(beat(i, BASE_SLOT + i * 20, None), SyncState::AtTip, 0);
        }
        // Blocks strictly after tip - 3600 s: 180 of them.
        assert_eq!(hb.snapshot(0).window.blocks_last_hour, Some(180));
    }

    #[test]
    fn snapshots_serialise() {
        let mut hb = Heartbeat::new(Network::Mainnet);
        let t0 = ms_at_slot(BASE_SLOT);
        hb.connected(t0);
        hb.roll_forward(beat(1, BASE_SLOT, Some(3)), SyncState::AtTip, t0);
        let snap = hb.snapshot(t0 + 1_000);
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains(r#""state":"following""#), "{json}");
        let back: HeartbeatSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, snap);
    }
}
