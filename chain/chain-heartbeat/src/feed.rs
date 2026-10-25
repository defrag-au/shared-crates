//! The socket to a heartbeat host, folded into a local [`Heartbeat`](crate::Heartbeat).
//!
//! Feature `flow`. Renderer-free, like the rest of this crate: it holds a
//! [`ui_flow::PollingFlowConnection`], whose TRANSPORT is chosen by whichever
//! `ui-flow` feature the consumer enables — `macroquad` under miniquad, the
//! default web-sys transport under egui.
//!
//! This lived in `chain-live` (an egui crate) and was ported by hand into
//! `frontend-pump` (a macroquad app), because `chain-live` also draws egui
//! widgets. The port's own header named the cost: "if one of them changes how a
//! dropped socket reads, so must the other". It lives here instead, so there is
//! one answer to what a dropped socket means.
//!
//! Two hops, both honest. The host's own feed state arrives inside its frames;
//! this page's link to the host is folded in here, so a dropped socket reads as
//! offline rather than as a chain that stopped producing.

use ui_flow::{ConnectionStatus, FlowEvent, NoAction, NoDelta, NoState, PollingFlowConnection};

use crate::beat::ChainEvent;
use crate::frame::{FrameApplied, HeartbeatFrame};
use crate::heartbeat::{Heartbeat, ReconnectGrace};
use crate::network::Network;
use crate::relay::CHAIN_DOMAIN;
use crate::track::{TrackEvent, Tracker};

type ChainFlow = PollingFlowConnection<NoState, NoDelta, HeartbeatFrame, NoAction>;

/// Where the chain feed comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeedEndpoint {
    /// The public block feed for the frontend's own network:
    /// `blocks-<network>.hodlcroft.com`. Chosen by the network the chain is on,
    /// never by which environment the page is deployed in — a dev deployment on
    /// mainnet follows mainnet.
    Blocks,
    /// A heartbeat host's ui-flow socket, e.g. `wss://…/heartbeat/flow`.
    Url(String),
    /// No feed. Transactions are still tracked and confirmed; nothing is drawn.
    Disabled,
}

impl FeedEndpoint {
    /// The socket to open for `network`, or `None` when configured off.
    pub fn url(&self, network: Network) -> Option<String> {
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// The socket could not be opened at all, and why.
    Unavailable(String),
    /// The host follows a different network from this page. Nothing from it is
    /// applied — a preprod feed must never draw a mainnet train.
    WrongNetwork,
}

/// The socket, the heartbeat it fills, and what the link is doing.
pub struct ChainFeed {
    flow: Option<ChainFlow>,
    heartbeat: Heartbeat,
    link: FeedLink,
    /// This page's link, held back until a loss outlasts the grace. A blip
    /// must not reach the heartbeat, because the widgets read its `FeedHealth`
    /// and would paint an outage over a feed that is merely reconnecting.
    grace: ReconnectGrace,
}

impl ChainFeed {
    /// Open the feed for `network`. A socket that cannot open is a drawn state,
    /// not an error: a reader staring at an empty panel deserves the reason.
    ///
    /// ## Native builds get no socket
    ///
    /// quad-net's native transport PANICS when the host cannot be reached
    /// (`Failed to connect websocket`), and the release profile aborts on a
    /// panic, so there is no `Err` to draw. A native build is a review harness
    /// rather than a surface with a chain, so it reports [`FeedLink::Disabled`]
    /// and the screen says so. The browser transport reports a failed
    /// connection as a status event, which is what [`FeedLink`] exists for.
    pub fn connect(network: Network, endpoint: &FeedEndpoint) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = endpoint;
            Self::disabled(network)
        }
        #[cfg(target_arch = "wasm32")]
        {
            let Some(url) = endpoint.url(network) else {
                return Self::disabled(network);
            };
            let (flow, link) = match ChainFlow::connect(&url) {
                Ok(flow) => (Some(flow), FeedLink::Connecting),
                Err(e) => (None, FeedLink::Unavailable(format!("{e}"))),
            };
            Self {
                flow,
                heartbeat: Heartbeat::new(network),
                link,
                grace: ReconnectGrace::default(),
            }
        }
    }

    /// No socket, but a heartbeat to draw: the honest state for a build that
    /// cannot follow the chain.
    fn disabled(network: Network) -> Self {
        Self {
            flow: None,
            heartbeat: Heartbeat::new(network),
            link: FeedLink::Disabled,
            grace: ReconnectGrace::default(),
        }
    }

    pub fn heartbeat(&self) -> &Heartbeat {
        &self.heartbeat
    }

    pub fn link(&self) -> &FeedLink {
        &self.link
    }

    /// Drain the socket into the heartbeat, and show each frame to `tracker` so
    /// this page's own transactions ride the train. Call once per frame.
    ///
    /// `entropy` only jitters POLLING. A feed that names the transactions of
    /// every block it delivers places a tracked transaction the instant its
    /// block arrives, with no polling at all — pass `0` for that.
    pub fn pump(&mut self, now_ms: u64, tracker: &mut Tracker, entropy: u64) -> Vec<TrackEvent> {
        // Before draining anything: a socket that has been down long enough is
        // an outage, even if it never delivers another event to say so.
        self.apply_link_grace(now_ms);
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
                            self.grace.recovered();
                            if self.link == FeedLink::Connecting {
                                self.link = FeedLink::Live;
                            }
                            seen.extend(tracker.observe_frame(&event, now_ms, entropy));
                            // A block that left the chain takes its riders with
                            // it: they are waiting again, most likely for
                            // another block.
                            if may_roll_back(&event) {
                                tracker.rolled_back(self.heartbeat.tip().map(|b| b.height), now_ms);
                            }
                        }
                        FrameApplied::WrongNetwork => {
                            log::warn!("chain-heartbeat: the feed follows another network");
                            self.link = FeedLink::WrongNetwork;
                        }
                    }
                }
                // The socket is reconnecting. Do not mark the feed offline yet:
                // the usual drop is a recycled edge socket or a relay rotation
                // that is back in two or three seconds, and the widgets must
                // keep the live reading through it. `apply_link_grace` folds it
                // in only if it outlasts the grace.
                FlowEvent::StatusChanged(ConnectionStatus::Reconnecting { .. }) => {
                    self.grace.lost(now_ms);
                }
                // Terminal for this socket — there is no retry to wait for, so
                // this is an outage rather than a reconnect in progress.
                FlowEvent::StatusChanged(
                    ConnectionStatus::Disconnected | ConnectionStatus::AuthFailed,
                ) => {
                    self.grace.recovered();
                    self.heartbeat.disconnected(now_ms);
                }
                FlowEvent::Error { message, .. } => {
                    log::debug!("chain-heartbeat: {message}");
                }
                _ => {}
            }
        }
        seen
    }

    /// Fold a pending loss into the heartbeat once it outlasts the grace.
    fn apply_link_grace(&mut self, now_ms: u64) {
        if let Some(lost_at) = self.grace.outage(now_ms) {
            self.heartbeat.disconnected(lost_at);
        }
    }
}

/// Whether applying `frame` may have moved the tip back: a rollback, or a
/// resync replacing the whole window.
pub fn may_roll_back(frame: &HeartbeatFrame) -> bool {
    match frame {
        HeartbeatFrame::Events { events } => events
            .iter()
            .any(|event| matches!(event, ChainEvent::RollBackward { .. })),
        HeartbeatFrame::Resync { .. } => true,
        HeartbeatFrame::UpstreamLost => false,
    }
}
