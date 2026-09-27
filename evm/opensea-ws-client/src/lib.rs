//! The socket half of consuming the OpenSea stream, for native targets.
//!
//! [`opensea_stream`] is the protocol — frames, topics, filters, decode. This
//! crate is the transport: it opens one WebSocket, keeps a subscription set
//! joined, heartbeats, and reconnects. It is deliberately thin, because every
//! part worth testing already lives in the crate that has no socket.
//!
//! Native only. A Worker's WebSocket is a different binding of the same frames
//! and belongs behind `cfg(target_arch = "wasm32")` when a Worker consumer exists,
//! mirroring `http-client`'s split.
//!
//! # No replay
//!
//! Delivery is best-effort and a reconnect gets no backfill — OpenSea's own docs
//! say messages lost during connection errors are not re-sent. So this client can
//! tell a consumer when it reconnected, but never what it missed: a consumer that
//! must not miss a movement has to reconcile against the chain after any gap. See
//! `opensea-stream`'s crate docs for why that is a discontinuity problem rather
//! than a throughput one.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use opensea_stream::{EventFilter, Frame, Reply, ReplyStatus, StreamEvent, Topic, endpoint_url};
use tokio::net::TcpStream;
use tokio::time::{Interval, MissedTickBehavior};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

/// How often to prove the connection alive.
///
/// Phoenix presumes a socket dead without a heartbeat every 30 seconds, and the
/// stream's docs add the other half: reconnect if a reply has not arrived by the
/// time the next one is due.
const HEARTBEAT: Duration = Duration::from_secs(30);

/// How long to wait before the first reconnect attempt; doubles each attempt.
const FIRST_BACKOFF: Duration = Duration::from_secs(1);

/// The ceiling on reconnect backoff.
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// How many consecutive reconnects to attempt before giving up.
const MAX_ATTEMPTS: u32 = 8;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Why the socket could not be used.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The WebSocket layer refused the connection or the exchange failed.
    #[error("websocket: {0}")]
    Socket(#[from] tokio_tungstenite::tungstenite::Error),
    /// A frame could not be encoded. Not reachable with this crate's own frames,
    /// which are plain structs, but the type has to say so.
    #[error("frame encoding: {0}")]
    Frame(#[from] serde_json::Error),
}

/// One subscription: a collection, and which of its events to receive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    /// Which collection — or the wildcard, which is discovery only.
    pub topic: Topic,
    /// Which of its event types.
    pub filter: EventFilter,
}

impl Subscription {
    /// A subscription to a topic, narrowed to a filter.
    pub fn new(topic: Topic, filter: EventFilter) -> Self {
        Self { topic, filter }
    }
}

/// A frame that decoded into an event, with the bytes it came from.
///
/// The raw form is carried because it is the faithful artifact. A decoded event
/// re-encoded loses whatever the body model omits — `item.metadata` most of all —
/// and a corpus of real bytes is what a crate's fixtures should be built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivered {
    /// The frame exactly as it arrived.
    pub raw: String,
    /// What it decoded to.
    pub event: StreamEvent,
}

/// A connected stream, holding one socket and one watch set.
pub struct StreamClient {
    api_key: String,
    watch: Vec<Subscription>,
    next_ref: u64,
    socket: Socket,
    heartbeat: Interval,
}

impl StreamClient {
    /// Opens the socket and joins the watch set.
    ///
    /// Fails rather than retrying: an initial connection that cannot be made is
    /// worth surfacing, since the usual cause is a key that is wrong or does not
    /// belong to the stream.
    pub async fn connect(
        api_key: impl Into<String>,
        watch: Vec<Subscription>,
    ) -> Result<Self, Error> {
        let api_key = api_key.into();
        let socket = open(&api_key).await?;

        let mut heartbeat = tokio::time::interval(HEARTBEAT);
        // A heartbeat that could not be sent is not worth a burst of catch-up
        // ticks after a slow reconnect.
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let mut client = Self {
            api_key,
            watch,
            next_ref: 0,
            socket,
            heartbeat,
        };
        client.join_all().await?;
        Ok(client)
    }

    /// The next delivered event.
    ///
    /// Heartbeats and reconnects happen here, so a caller sees only events.
    /// `None` means reconnection was given up on after [`MAX_ATTEMPTS`]
    /// consecutive failures.
    ///
    /// A reconnect re-joins the whole watch set but delivers nothing that arrived
    /// while the socket was down — see the crate docs. A re-join is also the
    /// moment the event-type filter is re-sent, so the watch set is never left
    /// silently unfiltered.
    pub async fn next_event(&mut self) -> Option<Delivered> {
        loop {
            tokio::select! {
                _ = self.heartbeat.tick() => {
                    let frame = Frame::heartbeat(self.take_ref());
                    if self.send(&frame).await.is_err() && !self.reconnect().await {
                        return None;
                    }
                }
                message = self.socket.next() => {
                    match message {
                        Some(Ok(Message::Text(text))) => {
                            if let Some(delivered) = delivered(text.as_str()) {
                                return Some(delivered);
                            }
                        }
                        Some(Ok(Message::Ping(payload))) => {
                            if self.socket.send(Message::Pong(payload)).await.is_err()
                                && !self.reconnect().await
                            {
                                return None;
                            }
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => {
                            tracing::warn!("stream: socket error: {error}");
                            if !self.reconnect().await {
                                return None;
                            }
                        }
                        None => {
                            // CF tears sockets with a dirty 1006 and no Close frame,
                            // so an exhausted stream is the ordinary way this ends.
                            if !self.reconnect().await {
                                return None;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Sends a frame, returning the wire form's error if it cannot be encoded.
    async fn send<P: serde::Serialize>(&mut self, frame: &Frame<P>) -> Result<(), Error> {
        let wire = frame.to_wire()?;
        self.socket.send(Message::Text(wire.into())).await?;
        Ok(())
    }

    /// Joins every subscription in the watch set.
    async fn join_all(&mut self) -> Result<(), Error> {
        // The watch set is cloned rather than borrowed: `send` needs `&mut self`,
        // and a borrow of `self.watch` cannot be held across the await.
        for subscription in self.watch.clone() {
            let reference = self.take_ref();
            let frame = Frame::join(subscription.topic, subscription.filter, reference);
            self.send(&frame).await?;
        }
        Ok(())
    }

    /// The next reference. Phoenix only requires one per request per connection.
    fn take_ref(&mut self) -> String {
        self.next_ref += 1;
        self.next_ref.to_string()
    }

    /// Re-opens the socket and re-joins, backing off between attempts.
    ///
    /// Returns false once [`MAX_ATTEMPTS`] consecutive attempts have failed. Each
    /// call gets a fresh budget, so a connection that recovers and drops again is
    /// not penalised for the earlier failure.
    async fn reconnect(&mut self) -> bool {
        for attempt in 1..=MAX_ATTEMPTS {
            let backoff = backoff_for(attempt);
            tracing::warn!(
                "stream: reconnecting in {backoff:?} (attempt {attempt}/{MAX_ATTEMPTS})"
            );
            tokio::time::sleep(backoff).await;

            match open(&self.api_key).await {
                Ok(socket) => {
                    self.socket = socket;
                    self.heartbeat.reset();
                    match self.join_all().await {
                        Ok(()) => return true,
                        Err(error) => tracing::warn!("stream: re-join failed: {error}"),
                    }
                }
                Err(error) => tracing::warn!("stream: reconnect failed: {error}"),
            }
        }

        false
    }
}

/// Backoff for a 1-based attempt number, doubling to a ceiling.
fn backoff_for(attempt: u32) -> Duration {
    let doubled = FIRST_BACKOFF.saturating_mul(1u32 << attempt.min(16).saturating_sub(1));
    doubled.min(MAX_BACKOFF)
}

/// Opens a socket to the stream.
async fn open(api_key: &str) -> Result<Socket, Error> {
    let (socket, _response) = connect_async(endpoint_url(api_key)).await?;
    Ok(socket)
}

/// Decodes one frame, if it is an event.
///
/// A frame that is not an event is the common case — replies, and the heartbeat's
/// own answer — so it is silently ignored. A frame that will not parse at all is
/// logged and skipped rather than killing the follower: one malformed frame is not
/// a reason to stop reading a live stream.
fn delivered(raw: &str) -> Option<Delivered> {
    match StreamEvent::from_wire(raw) {
        Ok(Some(event)) => Some(Delivered {
            raw: raw.to_owned(),
            event,
        }),
        Ok(None) => {
            note_refusal(raw);
            None
        }
        Err(error) => {
            tracing::warn!("stream: skipped an undecodable frame: {error}");
            None
        }
    }
}

/// Logs a refused request, which is the only protocol frame worth surfacing.
///
/// A refused join is not the interesting failure — a wrong slug is accepted — but
/// a refusal is the one case the socket says anything at all.
fn note_refusal(raw: &str) {
    let Ok(frame) = Frame::<Reply>::from_wire(raw) else {
        return;
    };

    let status = frame.payload.status.as_wire();
    if status == ReplyStatus::Ok.as_wire() {
        return;
    }

    let topic = &frame.topic;
    let reason = frame
        .payload
        .response
        .reason
        .as_deref()
        .unwrap_or("no reason given");
    tracing::warn!("stream: refused on {topic}: {status} ({reason})");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_then_holds_at_the_ceiling() {
        assert_eq!(backoff_for(1), Duration::from_secs(1));
        assert_eq!(backoff_for(2), Duration::from_secs(2));
        assert_eq!(backoff_for(3), Duration::from_secs(4));
        assert_eq!(backoff_for(6), Duration::from_secs(32));
        // Held at the ceiling rather than doubling past it. An attempt number
        // arrives from a counter, not a constant, so it must not shift its way
        // into an overflow.
        assert_eq!(backoff_for(7), MAX_BACKOFF);
        assert_eq!(backoff_for(64), MAX_BACKOFF);
    }
}
