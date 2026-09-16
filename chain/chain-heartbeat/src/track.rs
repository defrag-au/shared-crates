//! What is waiting, when to ask about it, and what an answer means. Pure: no
//! clock, no socket, no runtime, no renderer.
//!
//! This lived in `chain-live` (an egui crate) until 2026-09-16. It was already
//! pure apart from one import — it projected straight to egui's `TrainRider` —
//! so a macroquad surface could only have had a second copy of landing
//! detection, and two copies of that WILL disagree about a rollback or a
//! phase-2 failure in whichever one is exercised less.
//!
//! It lives here instead because everything it needs is already here:
//! [`BlockPoller`], [`BlockTxs`], [`HeartbeatFrame`], [`TxInBlock`]. Each
//! renderer keeps its own projection of [`TxProgress`] onto whatever it draws.

use std::collections::HashMap;
use std::time::Duration;

use crate::beat::{BlockTxs, ChainEvent, TxInBlock};
use crate::frame::HeartbeatFrame;
use crate::poll::{BlockPoller, PollDecision, PollTiming};

/// Transactions kept once they stop waiting, so a landed one stays on the train
/// while its block scrolls by. Waiting ones are never dropped.
const SETTLED_KEPT: usize = 16;

/// Block lookups for one reported landing before giving up on placing it.
///
/// A host can be wrong that something landed — a buy "confirmed" because its
/// listing left the book may have lost the race for it — and such a
/// transaction has no block to find. About five minutes at one lookup a block.
const MAX_BLOCK_LOOKUPS: u32 = 15;

/// A transaction a frontend submitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackedTx {
    pub tx_hash: String,
    /// What it does, in the reader's words: "Your swap".
    pub label: String,
    pub progress: TxProgress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxProgress {
    /// Accepted by a node, not yet seen in a block.
    Waiting,
    /// In a block. `block_height` is `None` when whoever reported it could not
    /// say which, and then it is left off the train rather than guessed onto a
    /// bar.
    Landed { block_height: Option<u64> },
    /// Will not land as far as the host knows: rejected, or given up on. A
    /// landing reported later still counts.
    Dropped,
    /// In the block at `block_height`, but a script rejected it: its collateral
    /// was taken and nothing else it did happened. Chain evidence, so no report
    /// of a landing overrides it.
    FailedInBlock { block_height: u64 },
}

/// One transaction a status source found in a block. A hash it was asked about
/// and left out is still waiting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxLanding {
    pub tx_hash: String,
    pub block_height: Option<u64>,
}

/// Something the host should act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackEvent {
    /// A tracked transaction is in a block. Once per transaction, and only for
    /// what this crate found itself — in a block the feed delivered, or from a
    /// polled status source — never echoed back from a host's own report.
    Landed(TxLanding),
    /// A tracked transaction is in a block the feed delivered, but failed
    /// phase-2 validation: its collateral was taken, and nothing else it did
    /// happened.
    FailedInBlock(TxLanding),
}

/// What the host should do about confirmation right now.
#[derive(Debug, PartialEq, Eq)]
pub enum PollStep {
    Idle,
    /// Nothing yet; look again after this long.
    WakeIn(Duration),
    /// Ask the status source about these, now.
    Check(Vec<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CheckState {
    Ready,
    /// A check is out. Nothing is asked again until it answers, so a slow
    /// source never stacks up duplicate questions.
    InFlight,
}

/// What the tracker asks a status source about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asking {
    /// Whether waiting transactions have landed.
    Landings,
    /// Only which block a host-reported landing is in. The host stays the sole
    /// authority on whether anything landed; the source only names blocks.
    BlocksOnly,
    /// Nothing: the host reports landings and has no block source.
    Nothing,
}

pub struct Tracker {
    txs: Vec<TrackedTx>,
    poller: BlockPoller<String>,
    check: CheckState,
    asking: Asking,
    /// The hashes the check in flight asked about.
    asked: Vec<String>,
    /// Block lookups spent per reported landing, for [`MAX_BLOCK_LOOKUPS`].
    block_lookups: HashMap<String, u32>,
}

impl Tracker {
    pub fn new(timing: PollTiming, asking: Asking) -> Self {
        Self {
            txs: Vec::new(),
            poller: BlockPoller::new(timing),
            check: CheckState::Ready,
            asking,
            asked: Vec::new(),
            block_lookups: HashMap::new(),
        }
    }

    pub fn txs(&self) -> &[TrackedTx] {
        &self.txs
    }

    /// Start waiting on `tx_hash`. Submitting the same hash again (a retry)
    /// relabels it and puts it back to waiting.
    pub fn submitted(&mut self, tx_hash: String, label: String, now_ms: u64) {
        self.block_lookups.remove(&tx_hash);
        match self.asking {
            Asking::Landings => self.poller.await_work(tx_hash.clone(), now_ms),
            // Not the source's to decide: only a reported landing is looked up.
            Asking::BlocksOnly | Asking::Nothing => self.poller.resolve(&tx_hash),
        }
        match self.txs.iter_mut().find(|t| t.tx_hash == tx_hash) {
            Some(tracked) => {
                tracked.label = label;
                tracked.progress = TxProgress::Waiting;
            }
            None => self.txs.push(TrackedTx {
                tx_hash,
                label,
                progress: TxProgress::Waiting,
            }),
        }
    }

    /// Record that `tx_hash` is in a block. Returns whether that is news. A
    /// later report that cannot name the block keeps a height already known.
    ///
    /// With a block source, a landing that arrives without its block is looked
    /// up, a block at a time, until the source names it or the lookups run out.
    /// Reporting it again (every frame, say) neither restarts nor extends that.
    pub fn landed(&mut self, tx_hash: &str, block_height: Option<u64>, now_ms: u64) -> bool {
        let key = tx_hash.to_string();
        let Some(tracked) = self.txs.iter_mut().find(|t| t.tx_hash == tx_hash) else {
            self.poller.resolve(&key);
            return false;
        };
        let (news, known) = match tracked.progress {
            TxProgress::Landed { block_height } => (false, block_height),
            TxProgress::Waiting | TxProgress::Dropped => (true, None),
            // The chain showed it failed. A status source (Koios counts a
            // failed transaction as confirmed) or the host saying it landed
            // must not paint it green.
            TxProgress::FailedInBlock { .. } => {
                self.poller.resolve(&key);
                return false;
            }
        };
        let block_height = block_height.or(known);
        tracked.progress = TxProgress::Landed { block_height };

        let lookups = self.block_lookups.get(&key).copied().unwrap_or(0);
        match (self.asking, block_height) {
            (Asking::BlocksOnly, None) if lookups < MAX_BLOCK_LOOKUPS => {
                self.poller.await_work(key, now_ms);
            }
            (_, Some(_)) => {
                self.poller.resolve(&key);
                self.block_lookups.remove(&key);
            }
            // Out of lookups, or nothing to look up with. The count is kept, so
            // a repeat report does not start the lookups over.
            _ => self.poller.resolve(&key),
        }
        self.trim();
        news
    }

    /// Stop waiting on `tx_hash`.
    pub fn dropped(&mut self, tx_hash: &str) {
        self.poller.resolve(&tx_hash.to_string());
        self.block_lookups.remove(tx_hash);
        if let Some(tracked) = self
            .txs
            .iter_mut()
            .find(|t| t.tx_hash == tx_hash && t.progress == TxProgress::Waiting)
        {
            tracked.progress = TxProgress::Dropped;
        }
        self.trim();
    }

    pub fn forget(&mut self, tx_hash: &str) {
        self.poller.resolve(&tx_hash.to_string());
        self.block_lookups.remove(tx_hash);
        self.txs.retain(|t| t.tx_hash != tx_hash);
    }

    pub fn clear(&mut self) {
        for tracked in &self.txs {
            self.poller.resolve(&tracked.tx_hash);
        }
        self.txs.clear();
        self.block_lookups.clear();
    }

    /// Feed a host's frame through: the poller, then every tracked transaction
    /// against the transactions of each block in it. Returns what the chain just
    /// showed about tracked transactions.
    pub fn observe_frame(
        &mut self,
        frame: &HeartbeatFrame,
        now_ms: u64,
        entropy: u64,
    ) -> Vec<TrackEvent> {
        self.poller.observe_frame(frame, now_ms, entropy);
        let mut seen = Vec::new();
        match frame {
            HeartbeatFrame::Events { events } => {
                for event in events {
                    if let ChainEvent::BlockTransactions { txs } = event {
                        seen.extend(self.observe_block(txs, now_ms));
                    }
                }
            }
            HeartbeatFrame::Resync { recent_txs, .. } => {
                for txs in recent_txs {
                    seen.extend(self.observe_block(txs, now_ms));
                }
            }
            HeartbeatFrame::UpstreamLost => {}
        }
        seen
    }

    /// Match tracked transactions against one block's. A match is chain
    /// evidence, so it lands (or fails) the transaction in any mode: the host
    /// may still keep its own record, but the train shows what the chain did.
    pub fn observe_block(&mut self, txs: &BlockTxs, now_ms: u64) -> Vec<TrackEvent> {
        let hits: Vec<(String, TxInBlock)> = self
            .txs
            .iter()
            .filter_map(|t| {
                txs.find_hex(&t.tx_hash)
                    .map(|found| (t.tx_hash.clone(), found))
            })
            .collect();
        let mut seen = Vec::new();
        for (tx_hash, found) in hits {
            let landing = TxLanding {
                tx_hash: tx_hash.clone(),
                block_height: Some(txs.height),
            };
            match found {
                TxInBlock::Valid => {
                    if self.landed(&tx_hash, Some(txs.height), now_ms) {
                        seen.push(TrackEvent::Landed(landing));
                    }
                }
                TxInBlock::FailedValidation => {
                    if self.failed_in_block(&tx_hash, txs.height) {
                        seen.push(TrackEvent::FailedInBlock(landing));
                    }
                }
            }
        }
        seen
    }

    /// Record a phase-2 failure. Returns whether that is news.
    fn failed_in_block(&mut self, tx_hash: &str, block_height: u64) -> bool {
        self.poller.resolve(&tx_hash.to_string());
        self.block_lookups.remove(tx_hash);
        let Some(tracked) = self.txs.iter_mut().find(|t| t.tx_hash == tx_hash) else {
            return false;
        };
        let failed = TxProgress::FailedInBlock { block_height };
        let news = tracked.progress != failed;
        tracked.progress = failed;
        self.trim();
        news
    }

    /// The tip moved back to `tip_height` (`None`: nothing on the chain). A
    /// transaction placed in a block above it is waiting again — its block is
    /// gone, and it will most likely land in another.
    pub fn rolled_back(&mut self, tip_height: Option<u64>, now_ms: u64) {
        let orphaned = |height: u64| tip_height.is_none_or(|tip| height > tip);
        let mut reverted = Vec::new();
        for tracked in &mut self.txs {
            if let TxProgress::Landed {
                block_height: Some(height),
            }
            | TxProgress::FailedInBlock {
                block_height: height,
            } = tracked.progress
                && orphaned(height)
            {
                tracked.progress = TxProgress::Waiting;
                reverted.push(tracked.tx_hash.clone());
            }
        }
        if self.asking == Asking::Landings {
            for tx_hash in reverted {
                self.poller.await_work(tx_hash, now_ms);
            }
        }
    }

    /// Whether to ask the status source now. A `Check` is recorded as in flight
    /// until [`Self::check_finished`].
    pub fn next_poll(&mut self, now_ms: u64) -> PollStep {
        if self.check == CheckState::InFlight {
            return PollStep::Idle;
        }
        match self.poller.poll(now_ms) {
            PollDecision::Idle => PollStep::Idle,
            PollDecision::Wait { in_ms } => PollStep::WakeIn(Duration::from_millis(in_ms)),
            PollDecision::Poll { .. } => {
                let hashes: Vec<String> = self.poller.awaiting().cloned().collect();
                if hashes.is_empty() {
                    return PollStep::Idle;
                }
                self.check = CheckState::InFlight;
                self.asked = hashes.clone();
                PollStep::Check(hashes)
            }
        }
    }

    /// Fold a status source's answer in. Returns the landings that are news,
    /// which only a [`Asking::Landings`] source can produce.
    ///
    /// A failed check says nothing about whether anything landed, and the next
    /// block (or the poller's fallback) asks again. For a block lookup it still
    /// spends one of the landing's lookups, so a broken source gives up too.
    pub fn check_finished(
        &mut self,
        result: Result<Vec<TxLanding>, String>,
        now_ms: u64,
    ) -> Vec<TxLanding> {
        self.check = CheckState::Ready;
        let asked = std::mem::take(&mut self.asked);
        let landings = result.unwrap_or_else(|e| {
            log::warn!("tracker: status check failed: {e}");
            Vec::new()
        });
        match self.asking {
            Asking::Landings => landings
                .into_iter()
                .filter(|landing| self.landed(&landing.tx_hash, landing.block_height, now_ms))
                .collect(),
            Asking::BlocksOnly => {
                for landing in landings {
                    let Some(height) = landing.block_height else {
                        continue;
                    };
                    // A block for a transaction the host has not reported as
                    // landed is not taken as news that it did.
                    let reported = self.txs.iter().any(|t| {
                        t.tx_hash == landing.tx_hash
                            && matches!(t.progress, TxProgress::Landed { .. })
                    });
                    if reported {
                        self.landed(&landing.tx_hash, Some(height), now_ms);
                    }
                }
                for hash in asked {
                    let unplaced = self.txs.iter().any(|t| {
                        t.tx_hash == hash && t.progress == TxProgress::Landed { block_height: None }
                    });
                    if !unplaced {
                        continue;
                    }
                    let lookups = self.block_lookups.entry(hash.clone()).or_insert(0);
                    *lookups += 1;
                    if *lookups >= MAX_BLOCK_LOOKUPS {
                        log::debug!("tracker: no block found for {hash}; leaving it off");
                        self.poller.resolve(&hash);
                    }
                }
                Vec::new()
            }
            Asking::Nothing => Vec::new(),
        }
    }

    /// Keep every waiting transaction and the newest settled ones.
    fn trim(&mut self) {
        let settled = self
            .txs
            .iter()
            .filter(|t| t.progress != TxProgress::Waiting)
            .count();
        let mut excess = settled.saturating_sub(SETTLED_KEPT);
        self.txs.retain(|t| {
            if excess > 0 && t.progress != TxProgress::Waiting {
                excess -= 1;
                false
            } else {
                true
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beat::{BlockBeat, SyncState};

    const TIMING: PollTiming = PollTiming {
        settle: Duration::from_secs(2),
        jitter: Duration::from_secs(4),
        fallback: Duration::from_secs(20),
    };
    const T0: u64 = 1_777_566_291_000;

    fn block_at_tip(height: u64) -> HeartbeatFrame {
        HeartbeatFrame::Events {
            events: vec![ChainEvent::RollForward {
                beat: BlockBeat {
                    height,
                    slot: height,
                    hash: format!("{height:064x}"),
                    issuer_pool: "00".repeat(28),
                    body_size: 0,
                    tx_count: None,
                    block_time_unix: None,
                    vrf_output: None,
                },
                sync: SyncState::AtTip,
            }],
        }
    }

    fn landing(tx_hash: &str, block_height: Option<u64>) -> TxLanding {
        TxLanding {
            tx_hash: tx_hash.to_string(),
            block_height,
        }
    }

    /// What each tracked transaction is doing, in order — the assertions below
    /// used to read this off the egui rider projection, which no longer lives
    /// in this crate. Same facts, stated in the tracker's own vocabulary; the
    /// `TxProgress -> RiderState` mapping is pinned by `chain-live`'s own tests.
    fn progress(tracker: &Tracker) -> Vec<(&str, TxProgress)> {
        tracker
            .txs()
            .iter()
            .map(|t| (t.label.as_str(), t.progress))
            .collect()
    }

    #[test]
    fn a_waiting_tx_is_asked_about_once_after_the_next_block() {
        let mut tracker = Tracker::new(TIMING, Asking::Landings);
        tracker.submitted("aa".into(), "Your swap".into(), T0);

        tracker.observe_frame(&block_at_tip(100), T0 + 1_000, 0);
        assert_eq!(
            tracker.next_poll(T0 + 1_000),
            PollStep::WakeIn(Duration::from_secs(2))
        );
        assert_eq!(
            tracker.next_poll(T0 + 3_000),
            PollStep::Check(vec!["aa".to_string()])
        );
        // In flight: no second question, however many blocks arrive.
        tracker.observe_frame(&block_at_tip(101), T0 + 4_000, 0);
        assert_eq!(tracker.next_poll(T0 + 60_000), PollStep::Idle);
    }

    #[test]
    fn nothing_waiting_means_nothing_asked() {
        let mut tracker = Tracker::new(TIMING, Asking::Landings);
        tracker.observe_frame(&block_at_tip(100), T0, 0);
        assert_eq!(tracker.next_poll(T0 + 60_000), PollStep::Idle);
    }

    #[test]
    fn a_landing_is_news_once_and_rides_its_block() {
        let mut tracker = Tracker::new(TIMING, Asking::Landings);
        tracker.submitted("aa".into(), "Your swap".into(), T0);
        tracker.submitted("bb".into(), "Your claim".into(), T0);

        let news = tracker.check_finished(Ok(vec![landing("aa", Some(100))]), T0);
        assert_eq!(news, vec![landing("aa", Some(100))]);
        assert_eq!(
            progress(&tracker),
            vec![
                (
                    "Your swap",
                    TxProgress::Landed {
                        block_height: Some(100)
                    }
                ),
                ("Your claim", TxProgress::Waiting),
            ]
        );
        assert!(
            tracker
                .check_finished(Ok(vec![landing("aa", Some(100))]), T0)
                .is_empty()
        );
    }

    #[test]
    fn a_failed_check_keeps_waiting_and_asks_again() {
        let mut tracker = Tracker::new(TIMING, Asking::Landings);
        tracker.submitted("aa".into(), "Your swap".into(), T0);
        // No feed at all: the fallback still asks.
        assert_eq!(
            tracker.next_poll(T0 + 20_000),
            PollStep::Check(vec!["aa".to_string()])
        );
        assert!(tracker.check_finished(Err("502".into()), T0).is_empty());
        assert_eq!(tracker.txs()[0].progress, TxProgress::Waiting);
        assert_eq!(
            tracker.next_poll(T0 + 40_000),
            PollStep::Check(vec!["aa".to_string()])
        );
    }

    #[test]
    fn an_unnamed_block_is_off_the_train_but_a_known_one_is_kept() {
        let mut tracker = Tracker::new(TIMING, Asking::Landings);
        tracker.submitted("aa".into(), "Your swap".into(), T0);
        tracker.submitted("bb".into(), "Your buy".into(), T0);

        assert!(tracker.landed("aa", Some(100), T0));
        assert!(!tracker.landed("aa", None, T0));
        assert_eq!(
            tracker.txs()[0].progress,
            TxProgress::Landed {
                block_height: Some(100)
            }
        );

        assert!(tracker.landed("bb", None, T0));
        // Changed on request (2026-09-15): a landing whose block is not known
        // yet keeps its status row, rather than leaving the train until the
        // block is found. `Landed { block_height: None }` is what carries that.
        assert_eq!(
            progress(&tracker),
            vec![
                (
                    "Your swap",
                    TxProgress::Landed {
                        block_height: Some(100)
                    }
                ),
                ("Your buy", TxProgress::Landed { block_height: None }),
            ]
        );
    }

    #[test]
    fn a_dropped_tx_that_lands_anyway_is_news() {
        let mut tracker = Tracker::new(TIMING, Asking::Landings);
        tracker.submitted("aa".into(), "Your swap".into(), T0);
        tracker.dropped("aa");
        assert_eq!(tracker.next_poll(T0 + 60_000), PollStep::Idle);
        assert!(tracker.landed("aa", Some(100), T0));
    }

    #[test]
    fn settled_txs_are_capped_but_waiting_ones_never_are() {
        let mut tracker = Tracker::new(TIMING, Asking::Landings);
        tracker.submitted("waiting".into(), "Still going".into(), T0);
        for n in 0..(SETTLED_KEPT as u64 + 4) {
            let hash = format!("{n:02}");
            tracker.submitted(hash.clone(), format!("Tx {n}"), T0);
            tracker.landed(&hash, Some(n), T0);
        }
        assert_eq!(tracker.txs().len(), SETTLED_KEPT + 1);
        assert_eq!(tracker.txs()[0].tx_hash, "waiting");
        // The oldest settled went first.
        assert_eq!(tracker.txs()[1].tx_hash, "04");
    }

    #[test]
    fn a_reporting_host_is_never_pre_empted() {
        let mut tracker = Tracker::new(TIMING, Asking::BlocksOnly);
        tracker.submitted("aa".into(), "Your buy".into(), T0);
        tracker.observe_frame(&block_at_tip(100), T0 + 1_000, 0);
        // Waiting is the host's to resolve: the block source is not asked.
        assert_eq!(tracker.next_poll(T0 + 60_000), PollStep::Idle);
    }

    #[test]
    fn a_reported_landing_without_a_block_is_looked_up_and_placed() {
        let mut tracker = Tracker::new(TIMING, Asking::BlocksOnly);
        tracker.submitted("aa".into(), "Your buy".into(), T0);
        tracker.submitted("bb".into(), "Your offer".into(), T0);
        assert!(tracker.landed("aa", None, T0));

        tracker.observe_frame(&block_at_tip(100), T0 + 1_000, 0);
        assert_eq!(
            tracker.next_poll(T0 + 3_000),
            PollStep::Check(vec!["aa".to_string()])
        );
        // The source's word on `bb` is not taken: only the host lands things.
        let news = tracker.check_finished(
            Ok(vec![landing("aa", Some(100)), landing("bb", Some(100))]),
            T0 + 3_500,
        );
        assert!(news.is_empty());
        assert_eq!(
            progress(&tracker),
            vec![
                (
                    "Your buy",
                    TxProgress::Landed {
                        block_height: Some(100)
                    }
                ),
                ("Your offer", TxProgress::Waiting),
            ]
        );
        assert_eq!(tracker.next_poll(T0 + 120_000), PollStep::Idle);
    }

    #[test]
    fn block_lookups_give_up_and_a_repeat_report_does_not_restart_them() {
        let mut tracker = Tracker::new(TIMING, Asking::BlocksOnly);
        tracker.submitted("aa".into(), "Your buy".into(), T0);
        tracker.landed("aa", None, T0);

        let mut now = T0;
        for _ in 0..MAX_BLOCK_LOOKUPS {
            now += 20_000;
            assert_eq!(
                tracker.next_poll(now),
                PollStep::Check(vec!["aa".to_string()])
            );
            tracker.check_finished(Ok(Vec::new()), now);
            // A host reconciling every frame reports it again.
            tracker.landed("aa", None, now);
        }
        assert_eq!(tracker.next_poll(now + 60_000), PollStep::Idle);
        assert_eq!(
            tracker.txs()[0].progress,
            TxProgress::Landed { block_height: None }
        );
    }

    const HASH_A: &str = "22ef4bf62480f87fa0a97ec5239b3d682a20cdff3ef6dbde426a6b4e9aeb1be3";
    const HASH_B: &str = "f41e2d220cbf3b309ea966a1870c2f699a5951ec5e13d4a86cb6fc342a803160";

    fn bytes(hex_hash: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        hex::decode_to_slice(hex_hash, &mut out).unwrap();
        out
    }

    fn block_with(height: u64, hashes: &[&str], invalid: Vec<u32>) -> HeartbeatFrame {
        let hashes: Vec<[u8; 32]> = hashes.iter().map(|h| bytes(h)).collect();
        let HeartbeatFrame::Events { events } = block_at_tip(height) else {
            unreachable!()
        };
        let mut all = vec![ChainEvent::BlockTransactions {
            txs: BlockTxs::new(height, height, &hashes, invalid),
        }];
        all.extend(events);
        HeartbeatFrame::Events { events: all }
    }

    #[test]
    fn a_tx_seen_in_a_block_lands_there_at_once_in_any_mode() {
        for asking in [Asking::Landings, Asking::BlocksOnly, Asking::Nothing] {
            let mut tracker = Tracker::new(TIMING, asking);
            tracker.submitted(HASH_A.into(), "Your swap".into(), T0);
            tracker.submitted(HASH_B.into(), "Your buy".into(), T0);

            let seen =
                tracker.observe_frame(&block_with(100, &[HASH_A], Vec::new()), T0 + 1_000, 0);
            assert_eq!(
                seen,
                vec![TrackEvent::Landed(TxLanding {
                    tx_hash: HASH_A.into(),
                    block_height: Some(100),
                })],
                "{asking:?}"
            );
            assert_eq!(
                progress(&tracker),
                vec![
                    (
                        "Your swap",
                        TxProgress::Landed {
                            block_height: Some(100)
                        }
                    ),
                    ("Your buy", TxProgress::Waiting),
                ],
                "{asking:?}"
            );
            // Seen again (a resync replays recent blocks): not news.
            assert!(
                tracker
                    .observe_frame(&block_with(100, &[HASH_A], Vec::new()), T0 + 2_000, 0)
                    .is_empty()
            );
        }
    }

    #[test]
    fn a_failed_tx_is_failed_and_no_later_report_turns_it_green() {
        let mut tracker = Tracker::new(TIMING, Asking::Landings);
        tracker.submitted(HASH_A.into(), "Your swap".into(), T0);

        let seen = tracker.observe_frame(&block_with(100, &[HASH_B, HASH_A], vec![1]), T0, 0);
        assert_eq!(
            seen,
            vec![TrackEvent::FailedInBlock(TxLanding {
                tx_hash: HASH_A.into(),
                block_height: Some(100),
            })]
        );
        // Koios counts a failed transaction as confirmed; that must not win.
        assert!(
            tracker
                .check_finished(Ok(vec![landing(HASH_A, Some(100))]), T0)
                .is_empty()
        );
        assert!(!tracker.landed(HASH_A, None, T0));
        assert_eq!(
            progress(&tracker),
            vec![("Your swap", TxProgress::FailedInBlock { block_height: 100 })]
        );
        assert_eq!(tracker.next_poll(T0 + 60_000), PollStep::Idle);
    }

    #[test]
    fn a_rollback_under_a_landed_tx_puts_it_back_to_waiting() {
        let mut tracker = Tracker::new(TIMING, Asking::Landings);
        tracker.submitted(HASH_A.into(), "Your swap".into(), T0);
        tracker.observe_frame(&block_with(100, &[HASH_A], Vec::new()), T0, 0);

        tracker.rolled_back(Some(100), T0);
        assert_eq!(
            tracker.txs()[0].progress,
            TxProgress::Landed {
                block_height: Some(100)
            }
        );

        tracker.rolled_back(Some(99), T0 + 1_000);
        assert_eq!(tracker.txs()[0].progress, TxProgress::Waiting);
        // Waiting again means asked about again.
        assert_eq!(
            tracker.next_poll(T0 + 21_000),
            PollStep::Check(vec![HASH_A.to_string()])
        );
    }

    #[test]
    fn a_resync_catches_a_landing_made_while_away() {
        let mut tracker = Tracker::new(TIMING, Asking::BlocksOnly);
        tracker.submitted(HASH_A.into(), "Your swap".into(), T0);
        let frame = HeartbeatFrame::Resync {
            checkpoint: crate::heartbeat::Checkpoint {
                network: crate::network::Network::Mainnet,
                beats: Vec::new(),
            },
            feed: crate::heartbeat::FeedHealth::NotStarted,
            recent_txs: vec![BlockTxs::new(99, 99, &[bytes(HASH_A)], Vec::new())],
        };
        assert_eq!(
            tracker.observe_frame(&frame, T0, 0),
            vec![TrackEvent::Landed(TxLanding {
                tx_hash: HASH_A.into(),
                block_height: Some(99),
            })]
        );
    }

    #[test]
    fn without_a_block_source_nothing_is_ever_asked() {
        let mut tracker = Tracker::new(TIMING, Asking::Nothing);
        tracker.submitted("aa".into(), "Your buy".into(), T0);
        tracker.landed("aa", None, T0);
        tracker.observe_frame(&block_at_tip(100), T0 + 1_000, 0);
        assert_eq!(tracker.next_poll(T0 + 60_000), PollStep::Idle);
    }
}
