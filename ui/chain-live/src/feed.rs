//! The socket to a heartbeat host, folded into a local [`Heartbeat`].
//!
//! Two hops, both honest. The host's own feed state arrives inside its frames;
//! this page's link to the host is folded in here, so a dropped socket reads as
//! offline rather than as a chain that stopped producing.

use chain_heartbeat::{CHAIN_DOMAIN, ChainEvent, FrameApplied, Heartbeat, HeartbeatFrame, Network};
use ui_flow::notify::{NoAction, NoDelta, NoState};
use ui_flow::{ConnectionStatus, FlowEvent, PollingFlowConnection};

use crate::tracker::{ChainLiveEvent, Tracker};

type ChainFlow = PollingFlowConnection<NoState, NoDelta, HeartbeatFrame, NoAction>;

/// Where the chain feed comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeedEndpoint {
    /// The public block feed for the frontend's own network:
    /// `blocks-<network>.hodlcroft.com`. Chosen by the network the chain is on,
    /// never by which environment the page is deployed in — a dev deployment
    /// on mainnet follows mainnet.
    Blocks,
    /// A heartbeat host's ui-flow socket, e.g. `wss://…/heartbeat/flow`.
    Url(String),
    /// No feed. Transactions are still tracked and confirmed; nothing is drawn.
    Disabled,
}

impl FeedEndpoint {
    /// The socket to open for `network`, or `None` when configured off.
    fn url(&self, network: Network) -> Option<String> {
        match self {
            Self::Blocks => Some(format!(
                "wss://blocks-{}.hodlcroft.com/heartbeat/flow",
                network.name()
            )),
            Self::Url(url) => Some(url.clone()),
            Self::Disabled => None,
        }
    }
}

/// This page's link to the feed, as far as it affects what is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedLink {
    /// The socket is open, or opening, but nothing has arrived yet. Nothing is
    /// drawn: a feed that has never spoken (an unreachable or stopped host)
    /// says nothing about the chain, and "offline" would read as if it did.
    Connecting,
    /// The feed has delivered. A later drop still counts as live: the widgets
    /// then say, in words, that the feed is offline and reconnecting.
    Live,
    /// Configured off.
    Disabled,
    /// The socket could not be opened at all.
    Unavailable,
    /// The host follows a different network from this page. Nothing from it is
    /// applied.
    WrongNetwork,
}

pub(crate) struct ChainFeed {
    flow: Option<ChainFlow>,
    heartbeat: Heartbeat,
    link: FeedLink,
}

impl ChainFeed {
    pub(crate) fn connect(network: Network, endpoint: &FeedEndpoint) -> Self {
        let (flow, link) = match endpoint.url(network) {
            None => (None, FeedLink::Disabled),
            Some(url) => match ChainFlow::connect(&url) {
                Ok(flow) => (Some(flow), FeedLink::Connecting),
                Err(e) => {
                    log::warn!("chain-live: feed unavailable: {e}");
                    (None, FeedLink::Unavailable)
                }
            },
        };
        Self {
            flow,
            heartbeat: Heartbeat::new(network),
            link,
        }
    }

    pub(crate) fn heartbeat(&self) -> &Heartbeat {
        &self.heartbeat
    }

    pub(crate) fn link(&self) -> FeedLink {
        self.link
    }

    /// Drain the socket into the heartbeat and the tracker. Returns what the
    /// chain showed about tracked transactions.
    pub(crate) fn pump(
        &mut self,
        now_ms: u64,
        tracker: &mut Tracker,
        entropy: u64,
    ) -> Vec<ChainLiveEvent> {
        let mut seen = Vec::new();
        let Some(flow) = self.flow.as_mut() else {
            return seen;
        };
        while let Some(event) = flow.poll() {
            match event {
                FlowEvent::Notify { domain, event, .. } if domain == CHAIN_DOMAIN => {
                    if self.link == FeedLink::WrongNetwork {
                        continue;
                    }
                    match self.heartbeat.apply_frame(&event, now_ms) {
                        FrameApplied::Applied => {
                            if self.link == FeedLink::Connecting {
                                self.link = FeedLink::Live;
                            }
                            seen.extend(tracker.observe_frame(&event, now_ms, entropy));
                            if may_roll_back(&event) {
                                tracker.rolled_back(self.heartbeat.tip().map(|b| b.height), now_ms);
                            }
                        }
                        FrameApplied::WrongNetwork => {
                            log::warn!("chain-live: the feed follows another network");
                            self.link = FeedLink::WrongNetwork;
                        }
                    }
                }
                // Whatever the host is doing, this page is not receiving the
                // chain until the socket is back. The resync the host sends on
                // reconnect restores its view, and the poller's fallback keeps
                // confirmations moving meanwhile.
                FlowEvent::StatusChanged(
                    ConnectionStatus::Reconnecting { .. }
                    | ConnectionStatus::Disconnected
                    | ConnectionStatus::AuthFailed,
                ) => self.heartbeat.disconnected(now_ms),
                FlowEvent::Error { message, .. } => log::debug!("chain-live: {message}"),
                _ => {}
            }
        }
        seen
    }
}

/// Whether applying `frame` may have moved the tip back: a rollback, or a
/// resync replacing the whole window.
fn may_roll_back(frame: &HeartbeatFrame) -> bool {
    match frame {
        HeartbeatFrame::Events { events } => events
            .iter()
            .any(|event| matches!(event, ChainEvent::RollBackward { .. })),
        HeartbeatFrame::Resync { .. } => true,
        HeartbeatFrame::UpstreamLost => false,
    }
}
