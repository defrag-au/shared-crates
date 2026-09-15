//! [`ChainLive`]: the feed, the tracker and the widgets behind one handle.

use std::sync::mpsc;
use std::time::Duration;

use chain_heartbeat::{Heartbeat, Network, PollTiming};
use egui::{Response, Ui};
use egui_widgets::block_pulse::{BlockPulse, PulseDetail};
use egui_widgets::block_train::{BlockTrain, BlockTrainResponse};
use futures::future::LocalBoxFuture;

use crate::feed::{ChainFeed, FeedEndpoint, FeedLink};
use crate::tracker::{Asking, ChainLiveEvent, PollStep, TrackedTx, Tracker, TxLanding};

/// When a polled host asks about waiting transactions: a few seconds after
/// each block, so the indexer has caught up and open tabs do not ask in the
/// same second. The fallback is one mean block interval rather than the
/// poller's default minute, because a feed that is down is a real state and
/// confirmation must not get three times slower in it.
pub const CONFIRMATION_TIMING: PollTiming = PollTiming {
    settle: Duration::from_secs(2),
    jitter: Duration::from_secs(4),
    fallback: Duration::from_secs(20),
};

/// How often to drain the socket while it is up. A frame arriving does not wake
/// egui by itself; half a second is well inside a block interval.
const PUMP_INTERVAL: Duration = Duration::from_millis(500);

/// How often to drain a socket that has not delivered anything yet.
const CONNECTING_PUMP_INTERVAL: Duration = Duration::from_secs(2);

/// Asks a backend which transactions are in a block.
pub trait TxStatusSource {
    /// Of `tx_hashes`, the ones now in a block, with the block where the
    /// backend can name it. Leave a hash out to mean "still waiting". An
    /// `Err` changes nothing and is asked again on a later block.
    fn landed(
        &self,
        tx_hashes: Vec<String>,
    ) -> LocalBoxFuture<'static, Result<Vec<TxLanding>, String>>;
}

/// How the crate learns a transaction landed.
pub enum Confirmation {
    /// Ask `source` about waiting transactions on block arrival.
    Polled {
        source: Box<dyn TxStatusSource>,
        timing: PollTiming,
    },
    /// The host already learns of landings (a live delta, say) and reports
    /// them with [`ChainLive::landed`].
    ///
    /// With `blocks`, a landing reported without its block is looked up there
    /// on block arrival, so the transaction can ride its block on the train.
    /// `blocks` is only ever asked about landings the host reported: it names
    /// blocks, it never decides that something landed.
    Reported {
        blocks: Option<Box<dyn TxStatusSource>>,
        timing: PollTiming,
    },
}

impl Confirmation {
    /// [`Confirmation::Polled`] at [`CONFIRMATION_TIMING`].
    pub fn polled(source: impl TxStatusSource + 'static) -> Self {
        Self::Polled {
            source: Box::new(source),
            timing: CONFIRMATION_TIMING,
        }
    }

    /// [`Confirmation::Reported`], with no block lookups: landings reported
    /// without a block stay off the train.
    pub fn reported() -> Self {
        Self::Reported {
            blocks: None,
            timing: CONFIRMATION_TIMING,
        }
    }

    /// [`Confirmation::Reported`], looking up the block of each landing the
    /// host reports without one, at [`CONFIRMATION_TIMING`].
    pub fn reported_with_blocks(blocks: impl TxStatusSource + 'static) -> Self {
        Self::Reported {
            blocks: Some(Box::new(blocks)),
            timing: CONFIRMATION_TIMING,
        }
    }
}

pub struct ChainLiveConfig {
    /// The network this frontend is on. A feed following another is refused.
    pub network: Network,
    pub feed: FeedEndpoint,
    pub confirmation: Confirmation,
}

pub struct ChainLive {
    feed: ChainFeed,
    tracker: Tracker,
    confirmation: Confirmation,
    results_tx: mpsc::Sender<Result<Vec<TxLanding>, String>>,
    results_rx: mpsc::Receiver<Result<Vec<TxLanding>, String>>,
}

impl ChainLive {
    /// Open the feed. Call once, when the app starts.
    pub fn new(config: ChainLiveConfig) -> Self {
        let (timing, asking) = match &config.confirmation {
            Confirmation::Polled { timing, .. } => (*timing, Asking::Landings),
            Confirmation::Reported {
                blocks: Some(_),
                timing,
            } => (*timing, Asking::BlocksOnly),
            Confirmation::Reported {
                blocks: None,
                timing,
            } => (*timing, Asking::Nothing),
        };
        let (results_tx, results_rx) = mpsc::channel();
        Self {
            feed: ChainFeed::connect(config.network, &config.feed),
            tracker: Tracker::new(timing, asking),
            confirmation: config.confirmation,
            results_tx,
            results_rx,
        }
    }

    /// Drain the feed, fold in status answers, ask again if a block says to,
    /// and schedule the next wake-up. Call every frame, before drawing.
    pub fn tick(&mut self, ctx: &egui::Context) -> Vec<ChainLiveEvent> {
        let now = now_ms();
        let mut events = Vec::new();
        while let Ok(result) = self.results_rx.try_recv() {
            events.extend(
                self.tracker
                    .check_finished(result, now)
                    .into_iter()
                    .map(ChainLiveEvent::Landed),
            );
        }

        // First: a block the feed delivered is the fastest evidence there is.
        events.extend(self.feed.pump(now, &mut self.tracker, entropy()));

        let source = match &self.confirmation {
            Confirmation::Polled { source, .. }
            | Confirmation::Reported {
                blocks: Some(source),
                ..
            } => Some(source),
            Confirmation::Reported { blocks: None, .. } => None,
        };
        if let Some(source) = source {
            match self.tracker.next_poll(now) {
                PollStep::Idle => {}
                PollStep::WakeIn(after) => ctx.request_repaint_after(after),
                PollStep::Check(hashes) => {
                    let answer = source.landed(hashes);
                    let results = self.results_tx.clone();
                    let ctx = ctx.clone();
                    // Wake the context on arrival: egui repaints on demand, and
                    // an answer sitting in the channel would otherwise wait for
                    // the next mouse move.
                    wasm_bindgen_futures::spawn_local(async move {
                        let _ = results.send(answer.await);
                        ctx.request_repaint();
                    });
                }
            }
        }

        match self.feed.link() {
            FeedLink::Live => ctx.request_repaint_after(PUMP_INTERVAL),
            // Still has to be drained to ever become live, but nothing is drawn
            // and a host that never answers should not cost a repaint twice a
            // second for as long as the page is open.
            FeedLink::Connecting => ctx.request_repaint_after(CONNECTING_PUMP_INTERVAL),
            FeedLink::Disabled | FeedLink::Unavailable | FeedLink::WrongNetwork => {}
        }
        events
    }

    /// The host submitted `tx_hash`. Track it until it lands.
    pub fn submitted(&mut self, tx_hash: impl Into<String>, label: impl Into<String>) {
        self.tracker
            .submitted(tx_hash.into(), label.into(), now_ms());
    }

    /// The host learned `tx_hash` is in a block. The only way a
    /// [`Confirmation::Reported`] host confirms; harmless on a polled one.
    /// Idempotent, so a host may simply report every landing it knows of each
    /// frame.
    pub fn landed(&mut self, tx_hash: &str, block_height: Option<u64>) {
        self.tracker.landed(tx_hash, block_height, now_ms());
    }

    /// The host gave up on `tx_hash`: rejected, or failed to submit.
    pub fn dropped(&mut self, tx_hash: &str) {
        self.tracker.dropped(tx_hash);
    }

    pub fn forget(&mut self, tx_hash: &str) {
        self.tracker.forget(tx_hash);
    }

    /// Forget everything tracked, e.g. when the wallet disconnects.
    pub fn clear(&mut self) {
        self.tracker.clear();
    }

    pub fn tracked(&self) -> &[TrackedTx] {
        self.tracker.txs()
    }

    pub fn heartbeat(&self) -> &Heartbeat {
        self.feed.heartbeat()
    }

    pub fn link(&self) -> FeedLink {
        self.feed.link()
    }

    /// The compact pulse, for a header. Nothing until the feed has delivered,
    /// because until then it would say nothing about the chain.
    pub fn pulse(&self, ui: &mut Ui) -> Option<Response> {
        (self.link() == FeedLink::Live).then(|| {
            BlockPulse::new(self.heartbeat(), now_ms())
                .detail(PulseDetail::Compact)
                .id_salt("chain_live_pulse")
                .show(ui)
        })
    }

    /// The block train with this frontend's transactions riding it, for a
    /// footer that stays in view. Nothing when the feed cannot be live.
    pub fn train(&self, ui: &mut Ui) -> Option<BlockTrainResponse> {
        if self.link() != FeedLink::Live {
            return None;
        }
        let riders = self.tracker.riders();
        Some(
            BlockTrain::new(self.heartbeat(), now_ms())
                .riders(&riders)
                .id_salt("chain_live_train")
                .show(ui),
        )
    }
}

/// Wall-clock milliseconds. Not egui's input time: block times and the host's
/// frames are wall-clock.
fn now_ms() -> u64 {
    js_sys::Date::now() as u64
}

/// Randomness to spread a block-triggered check across open tabs.
fn entropy() -> u64 {
    (js_sys::Math::random() * u32::MAX as f64) as u64
}
