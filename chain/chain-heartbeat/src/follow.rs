//! Follow the chain tip over one Ouroboros node-to-node connection.
//!
//! [`follow`] runs until the connection fails and then returns the reason. It
//! never reconnects: the host owns the socket, the retry policy and whether it
//! should be running at all. On return the host calls
//! [`crate::Heartbeat::disconnected`] and, when it reconnects, passes
//! [`crate::Heartbeat::resume_points`] so missed blocks are replayed.
//!
//! While waiting on the chain the follower races the reply against the host's
//! sleep. When the sleep wins it sends a keep-alive; if the previous one is
//! still unanswered, the connection is judged dead and `follow` returns. A dead
//! socket is therefore noticed within two keep-alive intervals.
//!
//! A peer can also fail with the mux still healthy: every keep-alive answered,
//! no chain-sync data arriving, because the relay's own upstream is gone or it
//! lost sync. That looks exactly like a quiet chain, and an unanswered ping
//! never fires — so the follower ends the connection itself once no roll
//! forward or roll backward has arrived for [`FollowConfig::stall_after`].

use std::convert::Infallible;
use std::future::Future;
use std::pin::pin;
use std::time::Duration;

use futures_io::{AsyncRead, AsyncWrite};
use futures_util::future::{Either, select};
use ouroboros_mux::blockfetch::{self, BlockFetchError};
use ouroboros_mux::chainsync::{self, ChainSyncError, Intersect, Next};
use ouroboros_mux::handshake::{self, DiffusionMode, HandshakeError};
use ouroboros_mux::keepalive::{self, KeepAliveError};
use ouroboros_mux::{Mux, MuxError, Point, protocol};

use crate::beat::{BlockBeat, ChainEvent, ChainPoint, SyncState};
use crate::block::{BlockError, split_block};
use crate::header::{BlockHeader, HeaderError};
use crate::network::Network;

/// How much of each block to fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyDetail {
    /// Headers only: height, slot, producer and body size. One round trip per
    /// block.
    HeaderOnly,
    /// Also fetch the body: count the transactions, and hash each one for
    /// [`ChainEvent::BlockTransactions`]. A second round trip and up to ~90 KiB
    /// per block.
    CountTransactions,
}

#[derive(Debug, Clone)]
pub struct FollowConfig {
    pub network: Network,
    pub body: BodyDetail,
    pub keep_alive_every: Duration,
    /// End the connection when no chain-sync data — no roll forward, no roll
    /// backward — has arrived for this long, even while keep-alives are being
    /// answered. The failure it catches is a relay whose own upstream is down:
    /// the mux stays healthy and its chain view freezes, so a dead socket is
    /// never declared. The host treats the end like any other (rotate, retry).
    pub stall_after: Duration,
    /// Points to resume from, newest first. Empty, or none still on the chain,
    /// starts from the peer's tip.
    pub resume_from: Vec<ChainPoint>,
}

impl FollowConfig {
    pub fn new(network: Network) -> Self {
        Self {
            network,
            body: BodyDetail::CountTransactions,
            keep_alive_every: Duration::from_secs(30),
            // Mainnet's mean block interval is 20 s, so five minutes is a
            // fifteen-fold margin: the odds of a healthy chain being this
            // quiet are about 2 in 10 million.
            stall_after: Duration::from_secs(5 * 60),
            resume_from: Vec::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FollowError {
    #[error(transparent)]
    Mux(#[from] MuxError),
    #[error("handshake: {0}")]
    Handshake(#[from] HandshakeError),
    #[error("chain-sync: {0}")]
    ChainSync(#[from] ChainSyncError),
    #[error("block-fetch: {0}")]
    BlockFetch(#[from] BlockFetchError),
    #[error("keep-alive: {0}")]
    KeepAlive(#[from] KeepAliveError),
    #[error("header: {0}")]
    Header(#[from] HeaderError),
    #[error("block: {0}")]
    Block(#[from] BlockError),
    #[error("keep-alive {cookie} went unanswered for a full interval")]
    KeepAliveUnanswered { cookie: u16 },
    #[error("no chain-sync data for {silent:?} while the peer kept answering")]
    Stalled { silent: Duration },
    #[error("the peer's tip was not on its own chain when intersecting")]
    TipVanished,
    #[error("fetched block {fetched} is not the announced block {announced}")]
    BlockMismatch { announced: String, fetched: String },
}

/// Follow until the connection fails.
///
/// - `stream`: an open connection to a relay.
/// - `sleep`: returns a future that completes after the given duration (a
///   `worker::Delay`, a tokio sleep). It is only used to pace keep-alives.
/// - `on_event`: called for every [`ChainEvent`], synchronously, in order.
pub async fn follow<S, Sleep, Tick, OnEvent>(
    stream: S,
    config: &FollowConfig,
    mut sleep: Sleep,
    mut on_event: OnEvent,
) -> Result<Infallible, FollowError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    Sleep: FnMut(Duration) -> Tick,
    Tick: Future<Output = ()>,
    OnEvent: FnMut(ChainEvent),
{
    let mut mux = Mux::new(stream);
    let accepted = handshake::handshake(
        &mut mux,
        config.network.magic(),
        DiffusionMode::InitiatorOnly,
    )
    .await?;
    on_event(ChainEvent::Connected {
        version: accepted.version,
    });

    intersect(&mut mux, &config.resume_from).await?;

    let mut cookie: u16 = 0;
    let mut unanswered: Option<u16> = None;
    // Chain silence, measured in keep-alive intervals. Reset only by actual
    // chain data: a keep-alive answer or an `Await` reply proves the mux is up,
    // not that the peer is following the chain.
    let mut silent = Duration::ZERO;

    loop {
        mux.send(protocol::CHAIN_SYNC, &chainsync::encode_request_next())
            .await?;

        let next = loop {
            let tick = sleep(config.keep_alive_every);
            match recv_or_tick(&mut mux, tick).await? {
                Wake::Message(protocol::KEEP_ALIVE, bytes) => {
                    if unanswered == Some(keepalive::decode_response(&bytes)?) {
                        unanswered = None;
                        on_event(ChainEvent::KeepAliveAcknowledged);
                    }
                }
                Wake::Message(_, bytes) => match chainsync::decode_next(&bytes)? {
                    Next::Await => {}
                    reply => break reply,
                },
                Wake::Tick => {
                    silent = silent.saturating_add(config.keep_alive_every);
                    if silent >= config.stall_after {
                        return Err(FollowError::Stalled { silent });
                    }
                    if let Some(cookie) = unanswered {
                        return Err(FollowError::KeepAliveUnanswered { cookie });
                    }
                    cookie = cookie.wrapping_add(1);
                    mux.send(protocol::KEEP_ALIVE, &keepalive::encode_keep_alive(cookie))
                        .await?;
                    unanswered = Some(cookie);
                }
            }
        };

        // The peer is following the chain again; the stall clock restarts.
        silent = Duration::ZERO;

        match next {
            Next::RollForward(content, tip) => {
                if content.byron_subtag.is_some() {
                    return Err(HeaderError::Byron.into());
                }
                let header = BlockHeader::decode(content.variant, &content.cbor)?;
                let tx_count = match config.body {
                    BodyDetail::HeaderOnly => None,
                    BodyDetail::CountTransactions => {
                        let (count, txs) = fetch_transactions(&mut mux, &header).await?;
                        // Just before the block, so a host sends both in one
                        // frame and a subscriber can spot its own transaction
                        // the moment the block arrives.
                        on_event(ChainEvent::BlockTransactions { txs });
                        Some(count)
                    }
                };
                let sync = if header.height < tip.block_number {
                    SyncState::CatchingUp
                } else {
                    SyncState::AtTip
                };
                on_event(ChainEvent::RollForward {
                    beat: BlockBeat::new(config.network, &header, tx_count),
                    sync,
                });
            }
            Next::RollBackward(point, _) => on_event(ChainEvent::RollBackward {
                to: match point {
                    Point::Origin => None,
                    Point::Specific { slot, hash } => Some(ChainPoint { slot, hash }),
                },
            }),
            Next::Await => unreachable!("await replies are consumed by the wait loop"),
        }
    }
}

/// Intersect at the newest resume point still on the chain, else at the tip.
async fn intersect<S: AsyncRead + AsyncWrite + Unpin>(
    mux: &mut Mux<S>,
    resume_from: &[ChainPoint],
) -> Result<(), FollowError> {
    if !resume_from.is_empty() {
        let points: Vec<Point> = resume_from
            .iter()
            .map(|p| Point::Specific {
                slot: p.slot,
                hash: p.hash,
            })
            .collect();
        if let Intersect::Found(..) = chainsync::find_intersect(mux, &points).await? {
            return Ok(());
        }
    }
    let tip = chainsync::find_intersect(mux, &[]).await?.tip().clone();
    match tip.point {
        Point::Origin => Ok(()),
        point => match chainsync::find_intersect(mux, &[point]).await? {
            Intersect::Found(..) => Ok(()),
            Intersect::NotFound(_) => Err(FollowError::TipVanished),
        },
    }
}

/// Fetch a block's body: how many transactions it holds, and each one's hash
/// prefix and validity, for [`ChainEvent::BlockTransactions`].
async fn fetch_transactions<S: AsyncRead + AsyncWrite + Unpin>(
    mux: &mut Mux<S>,
    header: &BlockHeader,
) -> Result<(u32, crate::beat::BlockTxs), FollowError> {
    let point = Point::Specific {
        slot: header.slot,
        hash: header.hash,
    };
    let block = blockfetch::fetch_single(mux, &point).await?;
    let parts = split_block(&block)?;
    let fetched = BlockHeader::decode(parts.header_variant, parts.header_cbor)?;
    if fetched.hash != header.hash {
        return Err(FollowError::BlockMismatch {
            announced: hex::encode(header.hash),
            fetched: hex::encode(fetched.hash),
        });
    }
    let transactions = crate::block::block_transactions(&block)?;
    let txs = crate::beat::BlockTxs::new(
        header.height,
        header.slot,
        &transactions.hashes(),
        transactions.invalid,
    );
    Ok((parts.tx_count, txs))
}

enum Wake {
    Message(u16, Vec<u8>),
    Tick,
}

/// Wait for a chain-sync or keep-alive message, or the tick. Cancel-safe on the
/// receive side (see [`Mux::recv_any`]), so losing the race drops nothing.
async fn recv_or_tick<S: AsyncRead + Unpin, T: Future<Output = ()>>(
    mux: &mut Mux<S>,
    tick: T,
) -> Result<Wake, MuxError> {
    let recv = pin!(mux.recv_any(&[protocol::CHAIN_SYNC, protocol::KEEP_ALIVE]));
    let tick = pin!(tick);
    match select(recv, tick).await {
        Either::Left((received, _)) => {
            received.map(|(protocol, bytes)| Wake::Message(protocol, bytes))
        }
        Either::Right(((), _)) => Ok(Wake::Tick),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use futures::executor::block_on;
    use minicbor::data::Tag;
    use minicbor::{Decoder, Encoder};
    use ouroboros_mux::Tip;

    use super::*;

    const BLOCK: &[u8] = include_bytes!("../tests/fixtures/186000000.block.cbor");

    enum Step {
        Bytes(Vec<u8>),
        /// Reads never complete from here on.
        Hang,
    }

    #[derive(Default)]
    struct Peer {
        script: VecDeque<Step>,
        written: Vec<u8>,
        /// Answer keep-alive pings from what the follower wrote, so a test can
        /// script a peer whose mux stays healthy while its chain does not.
        answer_keep_alives: bool,
        answered: u16,
    }

    impl AsyncRead for Peer {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
            if let Some(response) = self.pending_keep_alive_response() {
                let n = response.len().min(buf.len());
                buf[..n].copy_from_slice(&response[..n]);
                if n < response.len() {
                    self.script.push_front(Step::Bytes(response[n..].to_vec()));
                }
                return Poll::Ready(Ok(n));
            }
            match self.script.pop_front() {
                None => Poll::Ready(Ok(0)),
                Some(Step::Hang) => {
                    self.script.push_front(Step::Hang);
                    Poll::Pending
                }
                Some(Step::Bytes(mut bytes)) => {
                    let n = bytes.len().min(buf.len());
                    buf[..n].copy_from_slice(&bytes[..n]);
                    if n < bytes.len() {
                        let rest = bytes.split_off(n);
                        self.script.push_front(Step::Bytes(rest));
                    }
                    Poll::Ready(Ok(n))
                }
            }
        }
    }

    impl AsyncWrite for Peer {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            self.written.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }
        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    impl Peer {
        /// The response segment for the newest keep-alive ping the follower has
        /// written and not yet had answered, if it has moved on since the last
        /// one answered. Echoing pings is what lets a test script the
        /// production failure: a peer whose mux is healthy while its chain view
        /// is frozen.
        fn pending_keep_alive_response(&mut self) -> Option<Vec<u8>> {
            if !self.answer_keep_alives {
                return None;
            }
            let mut newest = self.answered;
            let mut i = 0;
            while i + 8 <= self.written.len() {
                let proto = u16::from_be_bytes([self.written[i + 4], self.written[i + 5]]) & 0x7fff;
                let len = u16::from_be_bytes([self.written[i + 6], self.written[i + 7]]) as usize;
                let start = i + 8;
                if start + len > self.written.len() {
                    break;
                }
                if proto == protocol::KEEP_ALIVE {
                    newest = keep_alive_cookie(&self.written[start..start + len]).unwrap_or(newest);
                }
                i = start + len;
            }
            if newest == self.answered {
                return None;
            }
            self.answered = newest;
            let payload = cbor(|e| {
                e.array(2)
                    .and_then(|e| e.u8(1))
                    .and_then(|e| e.u16(newest))
                    .map(|_| ())
                    .unwrap()
            });
            Some(segment(protocol::KEEP_ALIVE, &payload))
        }
    }

    fn segment(protocol: u16, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0, 0, 0, 0];
        out.extend_from_slice(&(protocol | 0x8000).to_be_bytes());
        out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        out.extend_from_slice(payload);
        out
    }

    /// The cookie of a keep-alive request payload (`[0, cookie]`), when the
    /// payload is one.
    fn keep_alive_cookie(payload: &[u8]) -> Option<u16> {
        let mut decoder = Decoder::new(payload);
        if !matches!(decoder.array(), Ok(Some(2))) || decoder.u8().ok() != Some(0) {
            return None;
        }
        decoder.u16().ok()
    }

    fn cbor(build: impl FnOnce(&mut Encoder<&mut Vec<u8>>)) -> Vec<u8> {
        let mut out = Vec::new();
        build(&mut Encoder::new(&mut out));
        out
    }

    fn header_bytes() -> Vec<u8> {
        split_block(BLOCK).unwrap().header_cbor.to_vec()
    }

    fn fixture_tip() -> Tip {
        let header = BlockHeader::decode(6, &header_bytes()).unwrap();
        Tip {
            point: Point::Specific {
                slot: header.slot,
                hash: header.hash,
            },
            block_number: header.height,
        }
    }

    fn accept() -> Vec<u8> {
        cbor(|e| {
            e.array(3)
                .and_then(|e| e.u8(1))
                .and_then(|e| e.u64(14))
                .and_then(|e| e.array(4))
                .and_then(|e| e.u64(Network::Mainnet.magic()))
                .and_then(|e| e.bool(true))
                .and_then(|e| e.u8(0))
                .and_then(|e| e.bool(false))
                .map(|_| ())
                .unwrap()
        })
    }

    /// Handshake, then a tip intersect: not-found with the tip, then found.
    fn opening() -> Vec<u8> {
        let tip = fixture_tip();
        let mut stream = segment(protocol::HANDSHAKE, &accept());
        stream.extend(segment(
            protocol::CHAIN_SYNC,
            &cbor(|e| {
                e.array(2)
                    .and_then(|e| e.u8(6))
                    .and_then(|e| e.encode(&tip))
                    .map(|_| ())
                    .unwrap()
            }),
        ));
        stream.extend(segment(
            protocol::CHAIN_SYNC,
            &cbor(|e| {
                e.array(3)
                    .and_then(|e| e.u8(5))
                    .and_then(|e| e.encode(&tip.point))
                    .and_then(|e| e.encode(&tip))
                    .map(|_| ())
                    .unwrap()
            }),
        ));
        stream
    }

    #[test]
    fn follows_from_the_tip_and_reports_a_live_block() {
        let tip = fixture_tip();
        let mut stream = opening();
        // First reply after an intersect: roll back to the intersection.
        stream.extend(segment(
            protocol::CHAIN_SYNC,
            &cbor(|e| {
                e.array(3)
                    .and_then(|e| e.u8(3))
                    .and_then(|e| e.encode(&tip.point))
                    .and_then(|e| e.encode(&tip))
                    .map(|_| ())
                    .unwrap()
            }),
        ));
        // Then the peer is at its tip, and a block arrives.
        stream.extend(segment(protocol::CHAIN_SYNC, &[0x81, 0x01]));
        stream.extend(segment(
            protocol::CHAIN_SYNC,
            &cbor(|e| {
                e.array(3)
                    .and_then(|e| e.u8(2))
                    .and_then(|e| e.array(2))
                    .and_then(|e| e.u8(6))
                    .and_then(|e| e.tag(Tag::new(24)))
                    .and_then(|e| e.bytes(&header_bytes()))
                    .and_then(|e| e.encode(&tip))
                    .map(|_| ())
                    .unwrap()
            }),
        ));

        let peer = Peer {
            script: [Step::Bytes(stream)].into(),
            ..Peer::default()
        };
        let mut config = FollowConfig::new(Network::Mainnet);
        config.body = BodyDetail::HeaderOnly;
        let mut events = Vec::new();
        let result = block_on(follow(
            peer,
            &config,
            |_| futures::future::pending::<()>(),
            |event| events.push(event),
        ));

        // The script runs out, which reads as the peer closing.
        assert!(matches!(result, Err(FollowError::Mux(MuxError::Closed))));
        assert_eq!(events[0], ChainEvent::Connected { version: 14 });
        assert!(matches!(
            events[1],
            ChainEvent::RollBackward {
                to: Some(ChainPoint {
                    slot: 186_000_000,
                    ..
                })
            }
        ));
        match &events[2] {
            ChainEvent::RollForward { beat, sync } => {
                assert_eq!(beat.height, 13_358_656);
                assert_eq!(*sync, SyncState::AtTip);
                assert_eq!(beat.tx_count, None);
            }
            other => panic!("expected a block, got {other:?}"),
        }
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn an_unanswered_keep_alive_ends_the_follow() {
        let mut stream = opening();
        stream.extend(segment(protocol::CHAIN_SYNC, &[0x81, 0x01]));
        let peer = Peer {
            script: [Step::Bytes(stream), Step::Hang].into(),
            ..Peer::default()
        };
        let config = FollowConfig::new(Network::Mainnet);
        let mut events = Vec::new();
        // Every tick is due immediately: the first sends a ping, the second
        // finds it unanswered.
        let result = block_on(follow(
            peer,
            &config,
            |_| futures::future::ready(()),
            |event| events.push(event),
        ));
        assert!(matches!(
            result,
            Err(FollowError::KeepAliveUnanswered { cookie: 1 })
        ));
        assert_eq!(events, vec![ChainEvent::Connected { version: 14 }]);
    }

    #[test]
    fn a_peer_that_answers_but_sends_no_data_is_stalled() {
        // The production failure: a relay whose mux is healthy — every
        // keep-alive answered — while its chain view is frozen. Nothing the
        // connection itself does ends it, so the stall watchdog has to.
        let mut stream = opening();
        stream.extend(segment(protocol::CHAIN_SYNC, &[0x81, 0x01]));
        let peer = Peer {
            script: [Step::Bytes(stream), Step::Hang].into(),
            answer_keep_alives: true,
            ..Peer::default()
        };
        let mut config = FollowConfig::new(Network::Mainnet);
        config.stall_after = Duration::from_secs(90);
        let mut events = Vec::new();
        // Every tick is due immediately: three 30 s intervals pass with the
        // peer answering each keep-alive and no chain data arriving.
        let result = block_on(follow(
            peer,
            &config,
            |_| futures::future::ready(()),
            |event| events.push(event),
        ));
        assert!(matches!(
            result,
            Err(FollowError::Stalled { silent }) if silent == Duration::from_secs(90)
        ));
        assert_eq!(events[0], ChainEvent::Connected { version: 14 });
        assert!(
            !events[1..].is_empty()
                && events[1..]
                    .iter()
                    .all(|event| matches!(event, ChainEvent::KeepAliveAcknowledged)),
            "expected the peer to have answered keep-alives, got {events:?}"
        );
    }
}
