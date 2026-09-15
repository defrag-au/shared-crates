//! Multiplexer framing over a byte stream.
//!
//! A segment on the wire is `[timestamp: u32][protocol: u16][length: u16]`
//! followed by `length` payload bytes, all big-endian. Bit 15 of the protocol
//! field marks the responder side.
//!
//! One mini-protocol message can span many segments, and one read can carry
//! many segments, so the mux keeps two buffers: raw bytes that are not yet a
//! whole segment, and per-protocol payload bytes that are not yet a whole CBOR
//! message.
//!
//! **Receiving is cancel-safe; sending is not.** All receive state lives in
//! `self`, and bytes returned by a read are stored before the future can
//! suspend again, so a receive can be dropped at any await point and resumed by
//! calling it again. A dropped `send` may have written part of a segment, which
//! corrupts the bearer, so never race a send.

use std::collections::HashMap;
use std::future::poll_fn;
use std::pin::Pin;

use futures_io::{AsyncRead, AsyncWrite};

const HEADER_LEN: usize = 8;
const RESPONDER_BIT: u16 = 0x8000;
const READ_CHUNK: usize = 16 * 1024;

/// Largest payload this client puts in one segment. Nodes accept larger
/// segments, but every node accepts this one, and client requests are tiny.
pub const MAX_SEGMENT_PAYLOAD: usize = 12_288;

/// Bytes one protocol may buffer before the peer is judged misbehaving. A full
/// block is under 100 KiB, so this leaves two orders of magnitude of headroom
/// while still bounding memory if a peer streams a protocol nobody reads.
pub const DEFAULT_INBOX_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum MuxError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("peer closed the connection")]
    Closed,
    #[error("protocol {protocol} buffered more than {limit} bytes without completing a message")]
    InboxOverflow { protocol: u16, limit: usize },
    #[error("malformed cbor on protocol {protocol}: {message}")]
    Malformed { protocol: u16, message: String },
}

/// Reads and writes mini-protocol messages over one bearer.
pub struct Mux<S> {
    stream: S,
    unframed: Vec<u8>,
    inboxes: HashMap<u16, Vec<u8>>,
    inbox_limit: usize,
    timestamp: u32,
}

impl<S> Mux<S> {
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            unframed: Vec::new(),
            inboxes: HashMap::new(),
            inbox_limit: DEFAULT_INBOX_LIMIT,
            timestamp: 0,
        }
    }

    /// Override [`DEFAULT_INBOX_LIMIT`].
    pub fn with_inbox_limit(mut self, limit: usize) -> Self {
        self.inbox_limit = limit;
        self
    }

    pub fn into_inner(self) -> S {
        self.stream
    }

    /// Take one complete message for `protocol` if one is already buffered.
    /// Never touches the stream.
    pub fn try_take(&mut self, protocol: u16) -> Result<Option<Vec<u8>>, MuxError> {
        let Some(inbox) = self.inboxes.get_mut(&protocol) else {
            return Ok(None);
        };
        if inbox.is_empty() {
            return Ok(None);
        }
        let length = {
            let mut decoder = minicbor::Decoder::new(inbox);
            match decoder.skip() {
                Ok(()) => decoder.position(),
                Err(e) if e.is_end_of_input() => return Ok(None),
                Err(e) => {
                    return Err(MuxError::Malformed {
                        protocol,
                        message: e.to_string(),
                    });
                }
            }
        };
        Ok(Some(inbox.drain(..length).collect()))
    }

    /// Move every whole segment out of `unframed` into its protocol's inbox.
    fn frame(&mut self) -> Result<(), MuxError> {
        let mut consumed = 0;
        while self.unframed.len() - consumed >= HEADER_LEN {
            let header = &self.unframed[consumed..consumed + HEADER_LEN];
            let protocol = u16::from_be_bytes([header[4], header[5]]) & !RESPONDER_BIT;
            let length = u16::from_be_bytes([header[6], header[7]]) as usize;
            let start = consumed + HEADER_LEN;
            if self.unframed.len() < start + length {
                break;
            }
            let inbox = self.inboxes.entry(protocol).or_default();
            if inbox.len() + length > self.inbox_limit {
                return Err(MuxError::InboxOverflow {
                    protocol,
                    limit: self.inbox_limit,
                });
            }
            inbox.extend_from_slice(&self.unframed[start..start + length]);
            consumed = start + length;
        }
        self.unframed.drain(..consumed);
        Ok(())
    }
}

impl<S: AsyncRead + Unpin> Mux<S> {
    /// Receive the next complete message for `protocol`. Cancel-safe.
    pub async fn recv(&mut self, protocol: u16) -> Result<Vec<u8>, MuxError> {
        self.recv_any(&[protocol]).await.map(|(_, message)| message)
    }

    /// Receive the next complete message on any of `protocols`, earliest listed
    /// protocol first when several are ready. Messages for other protocols are
    /// kept for a later call. Cancel-safe.
    pub async fn recv_any(&mut self, protocols: &[u16]) -> Result<(u16, Vec<u8>), MuxError> {
        loop {
            for &protocol in protocols {
                if let Some(message) = self.try_take(protocol)? {
                    return Ok((protocol, message));
                }
            }
            self.fill().await?;
        }
    }

    /// One read from the stream, framed immediately. The read result is stored
    /// before this returns, with no await in between, which is what makes the
    /// receive path cancel-safe.
    async fn fill(&mut self) -> Result<(), MuxError> {
        let mut chunk = vec![0u8; READ_CHUNK];
        let stream = &mut self.stream;
        let read = poll_fn(|cx| Pin::new(&mut *stream).poll_read(cx, &mut chunk)).await?;
        if read == 0 {
            return Err(MuxError::Closed);
        }
        self.unframed.extend_from_slice(&chunk[..read]);
        self.frame()
    }
}

impl<S: AsyncWrite + Unpin> Mux<S> {
    /// Send one message, split into as many segments as it needs. NOT
    /// cancel-safe: see the module note.
    pub async fn send(&mut self, protocol: u16, message: &[u8]) -> Result<(), MuxError> {
        for chunk in message.chunks(MAX_SEGMENT_PAYLOAD) {
            // The timestamp field is informational (peers use it for RTT
            // estimates). A counter keeps this crate free of any clock, which
            // is what lets it build for every wasm target.
            self.timestamp = self.timestamp.wrapping_add(1);
            let mut segment = Vec::with_capacity(HEADER_LEN + chunk.len());
            segment.extend_from_slice(&self.timestamp.to_be_bytes());
            segment.extend_from_slice(&(protocol & !RESPONDER_BIT).to_be_bytes());
            segment.extend_from_slice(&(chunk.len() as u16).to_be_bytes());
            segment.extend_from_slice(chunk);
            write_all(&mut self.stream, &segment).await?;
        }
        let stream = &mut self.stream;
        poll_fn(|cx| Pin::new(&mut *stream).poll_flush(cx)).await?;
        Ok(())
    }
}

async fn write_all<S: AsyncWrite + Unpin>(stream: &mut S, mut bytes: &[u8]) -> Result<(), MuxError> {
    while !bytes.is_empty() {
        let written = poll_fn(|cx| Pin::new(&mut *stream).poll_write(cx, bytes)).await?;
        if written == 0 {
            return Err(MuxError::Io(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "stream accepted zero bytes",
            )));
        }
        bytes = &bytes[written..];
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use futures_io::{AsyncRead, AsyncWrite};

    /// One step of a scripted peer.
    pub enum Step {
        /// Bytes the next read returns (split across reads if the buffer is small).
        Bytes(Vec<u8>),
        /// The next read returns `Pending` once, waking immediately.
        Yield,
    }

    /// An in-memory bearer: reads follow a script, writes are recorded. An
    /// exhausted script reads as a closed connection.
    #[derive(Default)]
    pub struct ScriptedPeer {
        pub script: VecDeque<Step>,
        pub written: Vec<u8>,
    }

    impl ScriptedPeer {
        pub fn new(script: impl IntoIterator<Item = Step>) -> Self {
            Self {
                script: script.into_iter().collect(),
                written: Vec::new(),
            }
        }
    }

    impl AsyncRead for ScriptedPeer {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
            match self.script.pop_front() {
                None => Poll::Ready(Ok(0)),
                Some(Step::Yield) => {
                    cx.waker().wake_by_ref();
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

    impl AsyncWrite for ScriptedPeer {
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

    /// Frame `payload` as one responder segment for `protocol`.
    pub fn segment(protocol: u16, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0, 0, 0, 0];
        out.extend_from_slice(&(protocol | 0x8000).to_be_bytes());
        out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        out.extend_from_slice(payload);
        out
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll};

    use futures::executor::block_on;
    use futures::task::noop_waker;

    use super::test_support::{ScriptedPeer, Step, segment};
    use super::*;

    /// A CBOR byte string: header byte + payload, long enough to span segments.
    fn cbor_bytes(len: usize) -> Vec<u8> {
        let mut out = Vec::new();
        minicbor::Encoder::new(&mut out).bytes(&vec![7u8; len]).unwrap();
        out
    }

    #[test]
    fn reassembles_a_message_split_across_segments_and_keeps_other_protocols() {
        let message = cbor_bytes(100);
        let (first, second) = message.split_at(40);
        let keep_alive = vec![0x82, 0x01, 0x05];
        let mut stream = segment(2, first);
        stream.extend(segment(8, &keep_alive));
        stream.extend(segment(2, second));

        let mut mux = Mux::new(ScriptedPeer::new([Step::Bytes(stream)]));
        let received = block_on(mux.recv(2)).unwrap();
        assert_eq!(received, message);
        assert_eq!(block_on(mux.recv(8)).unwrap(), keep_alive);
    }

    #[test]
    fn recv_any_prefers_the_earlier_listed_protocol() {
        // Keep-alive arrives first on the wire, but both land in one read, so
        // the protocol listed first wins.
        let mut stream = segment(8, &[0x82, 0x01, 0x02]);
        stream.extend(segment(2, &[0x81, 0x01]));
        let mut mux = Mux::new(ScriptedPeer::new([Step::Bytes(stream)]));
        let (protocol, _) = block_on(mux.recv_any(&[2, 8])).unwrap();
        assert_eq!(protocol, 2);
        let (protocol, _) = block_on(mux.recv_any(&[2, 8])).unwrap();
        assert_eq!(protocol, 8);
    }

    #[test]
    fn a_dropped_receive_loses_no_bytes() {
        let message = cbor_bytes(60);
        let framed = segment(2, &message);
        let (first, second) = framed.split_at(20);
        let mut mux = Mux::new(ScriptedPeer::new([
            Step::Bytes(first.to_vec()),
            Step::Yield,
            Step::Bytes(second.to_vec()),
        ]));

        {
            let waker = noop_waker();
            let mut cx = Context::from_waker(&waker);
            let mut pending = pin!(mux.recv(2));
            // First poll reads the first 20 bytes, then hits the Yield.
            assert!(matches!(pending.as_mut().poll(&mut cx), Poll::Pending));
            // `pending` is dropped here, mid-message.
        }

        assert_eq!(block_on(mux.recv(2)).unwrap(), message);
    }

    #[test]
    fn send_splits_large_messages_into_bounded_segments() {
        let message = vec![0xAB; MAX_SEGMENT_PAYLOAD * 2 + 5];
        let mut mux = Mux::new(ScriptedPeer::default());
        block_on(mux.send(3, &message)).unwrap();
        let written = mux.into_inner().written;

        let mut offset = 0;
        let mut lengths = Vec::new();
        while offset < written.len() {
            let protocol = u16::from_be_bytes([written[offset + 4], written[offset + 5]]);
            assert_eq!(protocol, 3, "initiator segments carry no responder bit");
            let length = u16::from_be_bytes([written[offset + 6], written[offset + 7]]) as usize;
            lengths.push(length);
            offset += HEADER_LEN + length;
        }
        assert_eq!(lengths, vec![MAX_SEGMENT_PAYLOAD, MAX_SEGMENT_PAYLOAD, 5]);
    }

    #[test]
    fn an_unread_protocol_cannot_grow_without_bound() {
        let stream = segment(4, &cbor_bytes(64)[..32]);
        let mut mux = Mux::new(ScriptedPeer::new([Step::Bytes(stream)])).with_inbox_limit(16);
        let error = block_on(mux.recv(2)).unwrap_err();
        assert!(matches!(error, MuxError::InboxOverflow { protocol: 4, .. }));
    }

    #[test]
    fn a_closed_stream_is_reported_as_closed() {
        let mut mux = Mux::new(ScriptedPeer::default());
        assert!(matches!(block_on(mux.recv(2)), Err(MuxError::Closed)));
    }
}
