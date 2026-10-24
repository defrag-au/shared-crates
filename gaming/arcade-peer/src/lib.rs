//! The peer plane beside a lockstep room: signalling between players, plus the
//! browser half that owns `RTCPeerConnection`.
//!
//! The room stays the authority. This crate is how two players who can see each
//! other end up with a data channel, and how they decide which of them makes
//! the offer — nothing here decides where anyone is, and nothing here is
//! allowed to reach a sim, a state hash or a replay.
//!
//! ## Signalling is broadcast
//!
//! Every [`Signal`] goes to everyone in the group; the receiver filters on the
//! sender's seat, which the room stamps. That is deliberate, and it is what
//! `rtc-signaller` settled on: a signalling server that only has to relay needs
//! no routing table, no per-peer bookkeeping, and no way to get a route wrong.
//! For the group sizes this overlay has (K ≤ 12) the extra copies are nothing,
//! and the room still scopes delivery to the group, so an offer never leaves it.
//!
//! ## One offers, and the other tolerates it anyway
//!
//! [`leads_offer`] gives a deterministic answer from the two seats alone — no
//! round trip, no election, no clock. But it is a *rule*, not a guarantee: both
//! ends learn their seats from the room, and during a group change those can be
//! briefly inconsistent. So the receiving side has [`accept_their_offer`], and
//! a peer that already has an offer outstanding rolls its own back and takes
//! the other's. The rule makes glare rare; the tolerance makes it harmless.
//!
//! `rtc-quickconnect` is the worked example of getting this right, and its
//! comment says as much: creating an offer on the master is "only really for
//! the sake of politeness, as rtc couple implementation handles the slave
//! attempting to create an offer".

/// A player's stable id in a world. The same `u16` the room seats people by:
/// an alias here so this crate need not depend on `arcade-room`.
pub type Seat = u16;

/// Bumped when the layout below changes. The room and both ends of a link must
/// agree; a mismatch is refused rather than misread.
pub const SIGNAL_VERSION: u32 = 1;

/// An SDP body is 1–3 KB in practice. The cap is generous enough for a
/// browser's full offer and small enough that a hostile frame cannot ask for a
/// large allocation.
pub const MAX_SDP: usize = 8 * 1024;

/// A single ICE candidate line, or whatever the browser hands us. mDNS
/// hostnames are UUIDs rather than addresses, so this stays opaque: see the
/// note on candidates in the design document.
pub const MAX_CANDIDATE: usize = 512;

/// Why a peer said goodbye. The number is the peer's to interpret; the room
/// never acts on it.
pub const BYE_NORMAL: u8 = 0;
pub const BYE_REPLACED: u8 = 1;
pub const BYE_FAILED: u8 = 2;

/// One signalling message about to be broadcast to a group.
///
/// There is deliberately no `from` field: the room stamps the sender from the
/// authenticated connection, so a peer cannot claim to be someone else's seat.
/// The receiving side learns who sent a message from the envelope the room
/// delivers it in, not from anything the sender wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signal {
    /// I am here, in this group, with this epoch. The only message that is
    /// sent unprompted, and the one that starts a negotiation.
    Announce {
        epoch: u16,
    },
    Offer {
        epoch: u16,
        sdp: Vec<u8>,
    },
    Answer {
        epoch: u16,
        sdp: Vec<u8>,
    },
    Candidate {
        epoch: u16,
        candidate: Vec<u8>,
    },
    Bye {
        epoch: u16,
        reason: u8,
    },
}

impl Signal {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        match self {
            Self::Announce { epoch } => {
                w.u8(TAG_ANNOUNCE);
                w.u16(*epoch);
            }
            Self::Offer { epoch, sdp } => {
                w.u8(TAG_OFFER);
                w.u16(*epoch);
                w.blob(sdp);
            }
            Self::Answer { epoch, sdp } => {
                w.u8(TAG_ANSWER);
                w.u16(*epoch);
                w.blob(sdp);
            }
            Self::Candidate { epoch, candidate } => {
                w.u8(TAG_CANDIDATE);
                w.u16(*epoch);
                w.blob(candidate);
            }
            Self::Bye { epoch, reason } => {
                w.u8(TAG_BYE);
                w.u16(*epoch);
                w.u8(*reason);
            }
        }
        w.bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, SignalError> {
        let mut r = Reader::new(bytes);
        let tag = r.u8()?;
        let epoch = r.u16()?;
        let signal = match tag {
            TAG_ANNOUNCE => Self::Announce { epoch },
            TAG_OFFER => Self::Offer {
                epoch,
                sdp: r.blob(MAX_SDP)?,
            },
            TAG_ANSWER => Self::Answer {
                epoch,
                sdp: r.blob(MAX_SDP)?,
            },
            TAG_CANDIDATE => Self::Candidate {
                epoch,
                candidate: r.blob(MAX_CANDIDATE)?,
            },
            TAG_BYE => Self::Bye {
                epoch,
                reason: r.u8()?,
            },
            other => return Err(SignalError::UnknownTag(other)),
        };
        r.done()?;
        Ok(signal)
    }

    pub fn epoch(&self) -> u16 {
        match self {
            Self::Announce { epoch }
            | Self::Offer { epoch, .. }
            | Self::Answer { epoch, .. }
            | Self::Candidate { epoch, .. }
            | Self::Bye { epoch, .. } => *epoch,
        }
    }
}

/// Who makes the offer.
///
/// The lower seat leads. Deterministic from the two ids with no round trip:
/// each side computes the same answer from the same pair, and exactly one of
/// them is true. A tie returns `false` — a peer never offers to itself, and a
/// seat cannot be paired with itself in a group anyway.
pub fn leads_offer(me: Seat, them: Seat) -> bool {
    me < them
}

/// Whether to take an offer that arrived while we already had one outstanding.
///
/// The same comparison from the other side: the lower seat's offer wins, so the
/// higher seat rolls its own back and answers. Exactly one of a pair accepts,
/// which is what stops two peers rolling back at each other for ever.
pub fn accept_their_offer(me: Seat, them: Seat) -> bool {
    leads_offer(them, me)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalError {
    Truncated,
    UnknownTag(u8),
    TooLong,
    LeftOver,
}

impl std::fmt::Display for SignalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => write!(f, "signal ends early"),
            Self::UnknownTag(t) => write!(f, "unknown signal tag {t}"),
            Self::TooLong => write!(f, "a signal body is longer than allowed"),
            Self::LeftOver => write!(f, "bytes left over after the signal"),
        }
    }
}

impl std::error::Error for SignalError {}

const TAG_ANNOUNCE: u8 = 1;
const TAG_OFFER: u8 = 2;
const TAG_ANSWER: u8 = 3;
const TAG_CANDIDATE: u8 = 4;
const TAG_BYE: u8 = 5;

#[derive(Default)]
struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, v: u8) {
        self.bytes.push(v);
    }

    fn u16(&mut self, v: u16) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }

    /// A length-prefixed body. No escaping, which is the point: SDP is full of
    /// `\r\n` and JSON would double every one of them.
    fn blob(&mut self, body: &[u8]) {
        // Both callers cap well below this. Cutting rather than wrapping keeps
        // a caller's mistake from producing a frame that decodes as something
        // else entirely.
        let take = body.len().min(u16::MAX as usize);
        self.u16(take as u16);
        self.bytes.extend_from_slice(&body[..take]);
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], SignalError> {
        let end = self.at.checked_add(n).ok_or(SignalError::Truncated)?;
        let slice = self.bytes.get(self.at..end).ok_or(SignalError::Truncated)?;
        self.at = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, SignalError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, SignalError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn blob(&mut self, max: usize) -> Result<Vec<u8>, SignalError> {
        let len = usize::from(self.u16()?);
        if len > max {
            return Err(SignalError::TooLong);
        }
        Ok(self.take(len)?.to_vec())
    }

    fn done(&self) -> Result<(), SignalError> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(SignalError::LeftOver)
        }
    }
}

/// The browser half: one `RTCPeerConnection` per peer, a reliable `ctrl`
/// channel and an unreliable `state` one. Stamped into a game's `web/`
/// directory by its build script, the same way `miniquad-platform`'s plugin is.
///
/// It never parses a game message. Bytes go in and come out; the schema lives
/// in the game's own wire crate.
pub const PEER_JS: &str = include_str!("../js/peer.js");

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<Signal> {
        vec![
            Signal::Announce { epoch: 3 },
            Signal::Offer {
                epoch: 3,
                sdp: b"v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\n".to_vec(),
            },
            Signal::Answer {
                epoch: 3,
                sdp: b"v=0\r\na=setup:active\r\n".to_vec(),
            },
            Signal::Candidate {
                epoch: 3,
                candidate: b"candidate:1 1 udp 2113 10.0.0.2 5000 typ host".to_vec(),
            },
            Signal::Bye {
                epoch: 3,
                reason: BYE_REPLACED,
            },
        ]
    }

    #[test]
    fn every_signal_round_trips() {
        for signal in all() {
            assert_eq!(Signal::decode(&signal.encode()), Ok(signal));
        }
    }

    /// The two messages that carry nothing: a header and no body.
    #[test]
    fn a_bare_announce_is_three_bytes() {
        assert_eq!(Signal::Announce { epoch: 1 }.encode().len(), 3);
    }

    /// SDP is carried verbatim: no escaping, so what went in comes out byte for
    /// byte including its carriage returns.
    #[test]
    fn sdp_is_carried_verbatim() {
        let sdp = b"v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-\r\n".to_vec();
        let signal = Signal::Offer {
            epoch: 1,
            sdp: sdp.clone(),
        };
        match Signal::decode(&signal.encode()) {
            Ok(Signal::Offer { sdp: back, .. }) => assert_eq!(back, sdp),
            other => panic!("expected an offer, got {other:?}"),
        }
    }

    #[test]
    fn a_truncated_signal_is_refused() {
        for signal in all() {
            let full = signal.encode();
            for cut in 0..full.len() {
                assert!(
                    Signal::decode(&full[..cut]).is_err(),
                    "a {cut}-byte signal decoded"
                );
            }
        }
    }

    #[test]
    fn an_oversized_body_is_refused() {
        // A candidate claiming 4096 bytes is over MAX_CANDIDATE. Hand-built
        // because the writer would never make one.
        let mut bytes = vec![TAG_CANDIDATE, 1, 0];
        bytes.extend_from_slice(&4096u16.to_le_bytes());
        bytes.resize(bytes.len() + 4096, 0);
        assert_eq!(Signal::decode(&bytes), Err(SignalError::TooLong));
    }

    #[test]
    fn trailing_bytes_are_refused() {
        let mut bytes = Signal::Announce { epoch: 1 }.encode();
        bytes.push(0);
        assert_eq!(Signal::decode(&bytes), Err(SignalError::LeftOver));
    }

    /// Exactly one end of every pair leads, and one accepts. A rule that made
    /// both or neither true would deadlock half the connections.
    #[test]
    fn exactly_one_end_leads_and_accepts() {
        for me in 1..=64 {
            for them in 1..=64 {
                if me == them {
                    assert!(!leads_offer(me, them), "a seat led against itself");
                    assert!(!accept_their_offer(me, them));
                    continue;
                }
                assert_ne!(
                    leads_offer(me, them),
                    leads_offer(them, me),
                    "{me} and {them} disagreed about who leads"
                );
                assert_ne!(
                    accept_their_offer(me, them),
                    accept_their_offer(them, me),
                    "{me} and {them} disagreed about who accepts"
                );
                // The one that does not lead is the one that accepts.
                assert_eq!(
                    leads_offer(me, them),
                    accept_their_offer(them, me),
                    "leading and accepting disagree for {me}/{them}"
                );
            }
        }
    }

    #[test]
    fn an_unknown_tag_is_named() {
        assert_eq!(
            Signal::decode(&[99, 0, 0]),
            Err(SignalError::UnknownTag(99))
        );
    }

    /// The browser half ships in the crate, so a game stamps one file rather
    /// than keeping its own copy that drifts.
    #[test]
    fn the_browser_half_is_present() {
        assert!(PEER_JS.contains("window.__bt ="), "peer.js is not the shim");
        assert!(
            PEER_JS.contains("maxRetransmits: 0"),
            "no unreliable channel"
        );
    }
}
