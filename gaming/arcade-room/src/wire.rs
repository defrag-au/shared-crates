//! The wire format between a client and a room: one binary message per
//! WebSocket frame, a tag byte then fields, integers little-endian.
//!
//! Hand-written rather than serde: the crate has no dependencies, every
//! message is small and fixed in shape, and a malformed frame from a client
//! must be refused, never misread.

use crate::sim::{Change, Seat};

/// From a client to its room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToRoom {
    /// The first message on a connection.
    Hello {
        game: String,
        version: u32,
        /// Who this is, stable across reconnects: the same player gets their
        /// seat back within the grace period.
        player: String,
        /// A person, rather than a bot or a load test. Only people keep a
        /// world running.
        real: bool,
        /// A login token the room's host can verify (a Discord session),
        /// or empty for a guest. Optional on the wire: written only when
        /// present, so a guest's hello is the same bytes as before it
        /// existed, and an older room still seats them.
        session: String,
    },
    /// This seat's input changes, each for the tick it should land on.
    /// Sent empty as a heartbeat.
    Inputs(Vec<(u64, Vec<u8>)>),
    /// The client's world hash at a tick, for desync checks.
    Hash { tick: u64, hash: u64 },
    /// For measuring the round trip.
    Ping { client_ms: u64 },
    /// A signalling message for the other players here: an offer, an answer, a
    /// candidate or a goodbye. The room stamps the sender's seat and broadcasts
    /// it; what the bytes mean is `arcade-peer`'s business, not the room's.
    Signal(Vec<u8>),
}

/// From a room to a client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToClient {
    /// Seated: start from `snapshot`, then apply every `Confirmed` from its
    /// tick on.
    Welcome {
        seat: Seat,
        tick_hz: u32,
        /// Where the room's clock was when it sent this.
        room_tick: u64,
        snapshot: Vec<u8>,
    },
    /// Not seated, and why. The room closes the connection after this.
    Refused {
        reason: String,
    },
    /// The changes for ticks `first ..`, one list per tick, final.
    Confirmed {
        first: u64,
        room_tick: u64,
        ticks: Vec<Vec<Change>>,
    },
    /// The client's world disagreed with the room's: start again from here.
    Resync {
        snapshot: Vec<u8>,
    },
    Pong {
        client_ms: u64,
        room_tick: u64,
    },
    /// Another player's signalling message, with the seat the room vouches for.
    ///
    /// The sender's seat is stamped here rather than carried inside `bytes`, so
    /// a peer cannot claim to be somebody else's seat.
    Signal {
        from: Seat,
        bytes: Vec<u8>,
    },
}

/// Why a frame did not decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    Truncated,
    UnknownTag(u8),
    TooLong,
    LeftOver,
    NotUtf8,
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => write!(f, "frame ends early"),
            Self::UnknownTag(t) => write!(f, "unknown message tag {t}"),
            Self::TooLong => write!(f, "a field is longer than allowed"),
            Self::LeftOver => write!(f, "bytes left over after the message"),
            Self::NotUtf8 => write!(f, "a string is not UTF-8"),
        }
    }
}

impl std::error::Error for WireError {}

/// Caps that keep a hostile frame from asking for a huge allocation.
const MAX_STRING: usize = 256;
/// A login token's longest: a session JWT with a name and entitlements is a
/// few hundred bytes.
const MAX_TOKEN: usize = 4_096;
const MAX_ITEMS: usize = 4_096;
const MAX_BLOB: usize = 1 << 20;
/// A signalling body: an SDP description or a candidate. Generous for the
/// former, and far below a blob, because a peer sending a megabyte of
/// signalling would be a problem rather than a message.
const MAX_SIGNAL: usize = 16 * 1024;

impl ToRoom {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        match self {
            Self::Hello {
                game,
                version,
                player,
                real,
                session,
            } => {
                w.u8(1);
                w.str(game);
                w.u32(*version);
                w.str(player);
                w.u8(u8::from(*real));
                // A token too long to send goes as a guest: a token cut
                // short would only fail to verify.
                if !session.is_empty() && session.len() <= MAX_TOKEN {
                    w.token(session);
                }
            }
            Self::Inputs(changes) => {
                w.u8(2);
                w.len(changes.len());
                for (tick, bytes) in changes {
                    w.u64(*tick);
                    w.bytes8(bytes);
                }
            }
            Self::Hash { tick, hash } => {
                w.u8(3);
                w.u64(*tick);
                w.u64(*hash);
            }
            Self::Ping { client_ms } => {
                w.u8(4);
                w.u64(*client_ms);
            }
            Self::Signal(bytes) => {
                w.u8(5);
                w.bytes16(bytes);
            }
        }
        w.0
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader { bytes, at: 0 };
        let msg = match r.u8()? {
            1 => Self::Hello {
                game: r.str()?,
                version: r.u32()?,
                player: r.str()?,
                real: r.u8()? != 0,
                session: if r.at == r.bytes.len() {
                    String::new()
                } else {
                    r.token()?
                },
            },
            2 => {
                let n = r.len()?;
                let mut changes = Vec::with_capacity(n);
                for _ in 0..n {
                    changes.push((r.u64()?, r.bytes8()?));
                }
                Self::Inputs(changes)
            }
            3 => Self::Hash {
                tick: r.u64()?,
                hash: r.u64()?,
            },
            4 => Self::Ping {
                client_ms: r.u64()?,
            },
            5 => Self::Signal(r.bytes16()?),
            t => return Err(WireError::UnknownTag(t)),
        };
        r.done()?;
        Ok(msg)
    }
}

impl ToClient {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        match self {
            Self::Welcome {
                seat,
                tick_hz,
                room_tick,
                snapshot,
            } => {
                w.u8(1);
                w.u16(*seat);
                w.u32(*tick_hz);
                w.u64(*room_tick);
                w.blob(snapshot);
            }
            Self::Refused { reason } => {
                w.u8(2);
                w.str(reason);
            }
            Self::Confirmed {
                first,
                room_tick,
                ticks,
            } => {
                w.u8(3);
                w.u64(*first);
                w.u64(*room_tick);
                w.len(ticks.len());
                for changes in ticks {
                    w.len(changes.len());
                    for c in changes {
                        w.change(c);
                    }
                }
            }
            Self::Resync { snapshot } => {
                w.u8(4);
                w.blob(snapshot);
            }
            Self::Pong {
                client_ms,
                room_tick,
            } => {
                w.u8(5);
                w.u64(*client_ms);
                w.u64(*room_tick);
            }
            Self::Signal { from, bytes } => {
                w.u8(6);
                w.u16(*from);
                w.bytes16(bytes);
            }
        }
        w.0
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader { bytes, at: 0 };
        let msg = match r.u8()? {
            1 => Self::Welcome {
                seat: r.u16()?,
                tick_hz: r.u32()?,
                room_tick: r.u64()?,
                snapshot: r.blob()?,
            },
            2 => Self::Refused { reason: r.str()? },
            3 => {
                let first = r.u64()?;
                let room_tick = r.u64()?;
                let n = r.len()?;
                let mut ticks = Vec::with_capacity(n);
                for _ in 0..n {
                    let m = r.len()?;
                    let mut changes = Vec::with_capacity(m);
                    for _ in 0..m {
                        changes.push(r.change()?);
                    }
                    ticks.push(changes);
                }
                Self::Confirmed {
                    first,
                    room_tick,
                    ticks,
                }
            }
            4 => Self::Resync {
                snapshot: r.blob()?,
            },
            5 => Self::Pong {
                client_ms: r.u64()?,
                room_tick: r.u64()?,
            },
            6 => Self::Signal {
                from: r.u16()?,
                bytes: r.bytes16()?,
            },
            t => return Err(WireError::UnknownTag(t)),
        };
        r.done()?;
        Ok(msg)
    }
}

#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn len(&mut self, n: usize) {
        self.u32(n as u32);
    }
    fn str(&mut self, s: &str) {
        let s = &s.as_bytes()[..s.len().min(MAX_STRING)];
        self.u16(s.len() as u16);
        self.0.extend_from_slice(s);
    }
    /// A login token, up to [`MAX_TOKEN`]: longer than [`MAX_STRING`] allows.
    fn token(&mut self, s: &str) {
        self.u16(s.len() as u16);
        self.0.extend_from_slice(s.as_bytes());
    }
    fn bytes8(&mut self, b: &[u8]) {
        self.u8(b.len().min(255) as u8);
        self.0.extend_from_slice(&b[..b.len().min(255)]);
    }
    fn blob(&mut self, b: &[u8]) {
        self.len(b.len());
        self.0.extend_from_slice(b);
    }
    /// A signalling body, up to [`MAX_SIGNAL`].
    fn bytes16(&mut self, b: &[u8]) {
        let take = b.len().min(MAX_SIGNAL);
        self.u16(take as u16);
        self.0.extend_from_slice(&b[..take]);
    }
    fn change(&mut self, c: &Change) {
        match c {
            Change::Join(seat) => {
                self.u8(0);
                self.u16(*seat);
            }
            Change::Leave(seat) => {
                self.u8(1);
                self.u16(*seat);
            }
            Change::Input(seat, bytes) => {
                self.u8(2);
                self.u16(*seat);
                self.bytes8(bytes);
            }
            Change::Profile(seat, bytes) => {
                self.u8(3);
                self.u16(*seat);
                self.bytes8(bytes);
            }
            Change::World(bytes) => {
                self.u8(4);
                self.blob(bytes);
            }
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        let end = self.at.checked_add(n).ok_or(WireError::Truncated)?;
        let out = self.bytes.get(self.at..end).ok_or(WireError::Truncated)?;
        self.at = end;
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8, WireError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, WireError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().expect("2")))
    }
    fn u32(&mut self) -> Result<u32, WireError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4")))
    }
    fn u64(&mut self) -> Result<u64, WireError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("8")))
    }
    fn len(&mut self) -> Result<usize, WireError> {
        let n = self.u32()? as usize;
        if n > MAX_ITEMS {
            return Err(WireError::TooLong);
        }
        Ok(n)
    }
    fn str(&mut self) -> Result<String, WireError> {
        let n = self.u16()? as usize;
        if n > MAX_STRING {
            return Err(WireError::TooLong);
        }
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| WireError::NotUtf8)
    }
    fn token(&mut self) -> Result<String, WireError> {
        let n = self.u16()? as usize;
        if n > MAX_TOKEN {
            return Err(WireError::TooLong);
        }
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| WireError::NotUtf8)
    }
    fn bytes8(&mut self) -> Result<Vec<u8>, WireError> {
        let n = self.u8()? as usize;
        Ok(self.take(n)?.to_vec())
    }
    fn blob(&mut self) -> Result<Vec<u8>, WireError> {
        let n = self.u32()? as usize;
        if n > MAX_BLOB {
            return Err(WireError::TooLong);
        }
        Ok(self.take(n)?.to_vec())
    }
    fn bytes16(&mut self) -> Result<Vec<u8>, WireError> {
        let n = self.u16()? as usize;
        if n > MAX_SIGNAL {
            return Err(WireError::TooLong);
        }
        Ok(self.take(n)?.to_vec())
    }
    fn change(&mut self) -> Result<Change, WireError> {
        Ok(match self.u8()? {
            0 => Change::Join(self.u16()?),
            1 => Change::Leave(self.u16()?),
            2 => Change::Input(self.u16()?, self.bytes8()?),
            3 => Change::Profile(self.u16()?, self.bytes8()?),
            4 => Change::World(self.blob()?),
            t => return Err(WireError::UnknownTag(t)),
        })
    }
    fn done(&self) -> Result<(), WireError> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(WireError::LeftOver)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_round_trips() {
        let to_room = [
            ToRoom::Hello {
                game: "island-funk".into(),
                version: 3,
                player: "damo".into(),
                real: true,
                session: String::new(),
            },
            ToRoom::Hello {
                game: "island-funk".into(),
                version: 3,
                player: "damo".into(),
                real: true,
                session: "e30.".repeat(150),
            },
            ToRoom::Inputs(vec![(10, vec![1, 2, 3]), (11, vec![])]),
            ToRoom::Inputs(vec![]),
            ToRoom::Hash { tick: 60, hash: 99 },
            ToRoom::Ping { client_ms: 7 },
            ToRoom::Signal(b"v=0\r\na=setup:actpass\r\n".to_vec()),
            ToRoom::Signal(Vec::new()),
        ];
        for m in to_room {
            assert_eq!(ToRoom::decode(&m.encode()), Ok(m));
        }
        let to_client = [
            ToClient::Welcome {
                seat: 17,
                tick_hz: 60,
                room_tick: 1234,
                snapshot: vec![9; 3000],
            },
            ToClient::Refused {
                reason: "full".into(),
            },
            ToClient::Confirmed {
                first: 100,
                room_tick: 106,
                ticks: vec![
                    vec![Change::Join(3), Change::Input(3, vec![1, 2, 3])],
                    vec![],
                    vec![
                        Change::Leave(9),
                        Change::Profile(4, vec![1, 0, 0, 0, 7]),
                        Change::World(vec![2; 600]),
                    ],
                ],
            },
            ToClient::Resync {
                snapshot: vec![1, 2],
            },
            ToClient::Pong {
                client_ms: 5,
                room_tick: 6,
            },
            ToClient::Signal {
                from: 3,
                bytes: b"candidate:1 1 udp 2113 10.0.0.2 5000 typ host".to_vec(),
            },
        ];
        for m in to_client {
            assert_eq!(ToClient::decode(&m.encode()), Ok(m));
        }
    }

    /// A signalling body travels verbatim — SDP's carriage returns are not
    /// escaped — and one past the cap is refused before it is allocated.
    #[test]
    fn a_signal_body_is_verbatim_and_capped() {
        let sdp = b"v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\n".to_vec();
        let sent = ToClient::Signal {
            from: 9,
            bytes: sdp.clone(),
        };
        match ToClient::decode(&sent.encode()) {
            Ok(ToClient::Signal { from, bytes }) => {
                assert_eq!(from, 9);
                assert_eq!(bytes, sdp);
            }
            other => panic!("expected a signal, got {other:?}"),
        }

        // A body claiming more than the cap, assembled by hand because the
        // writer would never make one.
        let mut huge = vec![6u8, 9, 0];
        huge.extend_from_slice(&(MAX_SIGNAL as u16 + 1).to_le_bytes());
        huge.resize(huge.len() + MAX_SIGNAL + 1, 0);
        assert_eq!(ToClient::decode(&huge), Err(WireError::TooLong));
    }

    #[test]
    fn a_bad_frame_is_refused_not_misread() {
        assert_eq!(ToRoom::decode(&[]), Err(WireError::Truncated));
        assert_eq!(ToRoom::decode(&[99]), Err(WireError::UnknownTag(99)));
        let mut ping = ToRoom::Ping { client_ms: 1 }.encode();
        ping.push(0);
        assert_eq!(ToRoom::decode(&ping), Err(WireError::LeftOver));
        // A claimed list length past the cap is refused before allocating.
        let huge = [2u8, 0xff, 0xff, 0xff, 0x7f];
        assert_eq!(ToRoom::decode(&huge), Err(WireError::TooLong));
    }

    #[test]
    fn a_guests_hello_is_the_same_bytes_as_before_sessions() {
        let hello = |session: String| ToRoom::Hello {
            game: "g".into(),
            version: 1,
            player: "p".into(),
            real: true,
            session,
        };
        let guest = hello(String::new()).encode();
        // tag, "g", version, "p", real: nothing after.
        assert_eq!(guest, [1, 1, 0, b'g', 1, 0, 0, 0, 1, 0, b'p', 1]);
        // A token too long to send goes as a guest rather than cut short.
        assert_eq!(hello("x".repeat(MAX_TOKEN + 1)).encode(), guest);
    }
}
