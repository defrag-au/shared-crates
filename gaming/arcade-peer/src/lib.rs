//! The peer plane beside a lockstep room: signalling between players, plus the
//! browser half that owns `RTCPeerConnection`.
//!
//! The room stays the authority. This crate is how two players who can see each
//! other end up with a data channel, and how they decide which of them makes
//! the offer — nothing here decides where anyone is, and nothing here is
//! allowed to reach a sim, a state hash or a replay.
//!
//! ## Signalling is broadcast, and pairwise signals name their target
//!
//! Every [`Signal`] goes to everyone in the group; the receiver filters on the
//! sender's seat, which the room stamps. That is deliberate, and it is what
//! `rtc-signaller` settled on: a signalling server that only has to relay needs
//! no routing table, no per-peer bookkeeping, and no way to get a route wrong.
//! For the group sizes this overlay has (K ≤ 12) the extra copies are nothing,
//! and the room still scopes delivery to the group, so an offer never leaves it.
//!
//! Broadcast is not enough on its own, though: with three peers, C answers an
//! offer from A and an offer from B, and *both* answers reach both of them. Each
//! has one link to C and would apply both answers to it — the first wins and the
//! second is a `setRemoteDescription` in the wrong state. So the pairwise
//! messages ([`Signal::Offer`], `Answer`, `Candidate`, `Bye`, and the
//! `Restart` that renegotiates ICE without a teardown) carry a `to`, and
//! a receiver drops anything not addressed to it. The pair is `(from, to)`, and
//! `from` is the room's, so a peer cannot impersonate another; a wrong `to` only
//! misdelivers the sender's own message. `Announce` is genuinely broadcast and
//! carries no target.
//!
//! The room still never reads the body, and there is still no routing table.
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
//!
//! ## When a link dies
//!
//! A link can die without ICE saying so, and a `failed` connection used to be
//! terminal without persistence. The shim now has one exit — reset, tell the far
//! side, offer again — with an ICE **restart** as the cheap case: a
//! [`Signal::Restart`] carries new credentials for the connection both ends
//! already have, so the channels survive. It reuses the attempt's epoch
//! deliberately: a higher epoch means "start over", and starting over would
//! tear down the very channels the restart exists to keep.

/// A player's stable id in a world. The same `u16` the room seats people by:
/// an alias here so this crate need not depend on `arcade-room`.
pub type Seat = u16;

/// The negotiation state machine: every decision the browser half used to take
/// in JavaScript, as a pure function of events and an injected clock.
pub mod machine;

/// A candidate line, parsed: which end of a path is a relay, and which attempt
/// a candidate belongs to.
pub mod candidate;

/// The ICE server list a connection is built with.
pub mod ice;

/// Bumped when the layout below changes. The room and both ends of a link must
/// agree; a mismatch is refused rather than misread.
///
/// 2: pairwise signals carry a `to`, because a broadcast bus delivers one
/// peer's answer to every other peer, not only the one that offered.
pub const SIGNAL_VERSION: u32 = 2;

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
///
/// `to` is the other half of the pair, and it is the sender's to name — it says
/// which of *their* links the message is for, so a broadcast reaches one link
/// per receiver rather than every link from that sender. See the module note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signal {
    /// I am here, in this group, with this epoch. The only message that is
    /// sent unprompted, and the one that starts a negotiation. Broadcast: it
    /// has no target.
    Announce {
        epoch: u16,
    },
    Offer {
        epoch: u16,
        to: Seat,
        sdp: Vec<u8>,
    },
    /// An offer that restarts ICE on the connection both ends already have: new
    /// credentials, the same `RTCPeerConnection` and the same data channels. It
    /// carries the epoch of the attempt it belongs to rather than a new one — a
    /// higher epoch means "start over", which is the teardown this exists to
    /// avoid — and the far side answers it with an ordinary [`Signal::Answer`].
    Restart {
        epoch: u16,
        to: Seat,
        sdp: Vec<u8>,
    },
    Answer {
        epoch: u16,
        to: Seat,
        sdp: Vec<u8>,
    },
    Candidate {
        epoch: u16,
        to: Seat,
        candidate: Vec<u8>,
    },
    Bye {
        epoch: u16,
        to: Seat,
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
            Self::Offer { epoch, to, sdp } => {
                w.u8(TAG_OFFER);
                w.u16(*epoch);
                w.u16(*to);
                w.blob(sdp);
            }
            Self::Restart { epoch, to, sdp } => {
                w.u8(TAG_RESTART);
                w.u16(*epoch);
                w.u16(*to);
                w.blob(sdp);
            }
            Self::Answer { epoch, to, sdp } => {
                w.u8(TAG_ANSWER);
                w.u16(*epoch);
                w.u16(*to);
                w.blob(sdp);
            }
            Self::Candidate {
                epoch,
                to,
                candidate,
            } => {
                w.u8(TAG_CANDIDATE);
                w.u16(*epoch);
                w.u16(*to);
                w.blob(candidate);
            }
            Self::Bye { epoch, to, reason } => {
                w.u8(TAG_BYE);
                w.u16(*epoch);
                w.u16(*to);
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
                to: r.u16()?,
                sdp: r.blob(MAX_SDP)?,
            },
            TAG_RESTART => Self::Restart {
                epoch,
                to: r.u16()?,
                sdp: r.blob(MAX_SDP)?,
            },
            TAG_ANSWER => Self::Answer {
                epoch,
                to: r.u16()?,
                sdp: r.blob(MAX_SDP)?,
            },
            TAG_CANDIDATE => Self::Candidate {
                epoch,
                to: r.u16()?,
                candidate: r.blob(MAX_CANDIDATE)?,
            },
            TAG_BYE => Self::Bye {
                epoch,
                to: r.u16()?,
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
            | Self::Restart { epoch, .. }
            | Self::Answer { epoch, .. }
            | Self::Candidate { epoch, .. }
            | Self::Bye { epoch, .. } => *epoch,
        }
    }

    /// Who this message is for, or `None` for one that is broadcast.
    pub fn to(&self) -> Option<Seat> {
        match self {
            Self::Announce { .. } => None,
            Self::Offer { to, .. }
            | Self::Restart { to, .. }
            | Self::Answer { to, .. }
            | Self::Candidate { to, .. }
            | Self::Bye { to, .. } => Some(*to),
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
const TAG_RESTART: u8 = 6;

#[derive(Default)]
pub(crate) struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub(crate) fn u8(&mut self, v: u8) {
        self.bytes.push(v);
    }

    pub(crate) fn u16(&mut self, v: u16) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }

    /// A length-prefixed body. No escaping, which is the point: SDP is full of
    /// `\r\n` and JSON would double every one of them.
    pub(crate) fn blob(&mut self, body: &[u8]) {
        // Both callers cap well below this. Cutting rather than wrapping keeps
        // a caller's mistake from producing a frame that decodes as something
        // else entirely.
        let take = body.len().min(u16::MAX as usize);
        self.u16(take as u16);
        self.bytes.extend_from_slice(&body[..take]);
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], SignalError> {
        let end = self.at.checked_add(n).ok_or(SignalError::Truncated)?;
        let slice = self.bytes.get(self.at..end).ok_or(SignalError::Truncated)?;
        self.at = end;
        Ok(slice)
    }

    pub(crate) fn at_end(&self) -> bool {
        self.at >= self.bytes.len()
    }

    pub(crate) fn u8(&mut self) -> Result<u8, SignalError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16, SignalError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub(crate) fn blob(&mut self, max: usize) -> Result<Vec<u8>, SignalError> {
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
                to: 7,
                sdp: b"v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\n".to_vec(),
            },
            Signal::Restart {
                epoch: 3,
                to: 7,
                sdp: b"v=0\r\na=ice-ufrag:CvWn\r\n".to_vec(),
            },
            Signal::Answer {
                epoch: 3,
                to: 7,
                sdp: b"v=0\r\na=setup:active\r\n".to_vec(),
            },
            Signal::Candidate {
                epoch: 3,
                to: 7,
                candidate: b"candidate:1 1 udp 2113 10.0.0.2 5000 typ host".to_vec(),
            },
            Signal::Bye {
                epoch: 3,
                to: 7,
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

    /// The exact bytes, for the three shapes. The browser half writes this
    /// layout a second time in `peer.js`, and a change here that is not there
    /// is two ends disagreeing about what a byte means.
    #[test]
    fn the_layout_is_fixed() {
        assert_eq!(Signal::Announce { epoch: 3 }.encode(), [TAG_ANNOUNCE, 3, 0]);
        assert_eq!(
            Signal::Offer {
                epoch: 3,
                to: 7,
                sdp: b"hi".to_vec()
            }
            .encode(),
            [TAG_OFFER, 3, 0, 7, 0, 2, 0, b'h', b'i']
        );
        assert_eq!(
            Signal::Restart {
                epoch: 3,
                to: 7,
                sdp: b"hi".to_vec()
            }
            .encode(),
            [TAG_RESTART, 3, 0, 7, 0, 2, 0, b'h', b'i']
        );
        assert_eq!(
            Signal::Bye {
                epoch: 3,
                to: 7,
                reason: 1
            }
            .encode(),
            [TAG_BYE, 3, 0, 7, 0, 1]
        );
    }

    /// The two messages that carry nothing but their header.
    #[test]
    fn a_bare_announce_is_three_bytes() {
        assert_eq!(Signal::Announce { epoch: 1 }.encode().len(), 3);
    }

    /// An announce has no target; everything else is pairwise.
    #[test]
    fn only_announce_is_broadcast() {
        assert_eq!(Signal::Announce { epoch: 1 }.to(), None);
        for signal in all() {
            if !matches!(signal, Signal::Announce { .. }) {
                assert_eq!(signal.to(), Some(7), "{signal:?} lost its target");
            }
        }
    }

    /// SDP is carried verbatim: no escaping, so what went in comes out byte for
    /// byte including its carriage returns.
    #[test]
    fn sdp_is_carried_verbatim() {
        let sdp = b"v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-\r\n".to_vec();
        let signal = Signal::Offer {
            epoch: 1,
            to: 2,
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
        let mut bytes = vec![TAG_CANDIDATE, 1, 0, 2, 0];
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

    /// Host candidates alone cannot cross a NAT, and a browser disguises them as
    /// mDNS names, so a shim that ships with no STUN server looks connected on
    /// one machine and never connects anywhere else. This pins the two measured
    /// hosts — anycast, different operators — and fails if the list is emptied
    /// again.
    #[test]
    fn the_browser_half_configures_ice() {
        assert!(
            PEER_JS.contains("stun:stun.cloudflare.com:3478"),
            "the Cloudflare STUN server is gone"
        );
        assert!(
            PEER_JS.contains("stun:stun.l.google.com:19302"),
            "the Google STUN server is gone"
        );
        assert!(
            !PEER_JS.contains("var ICE_SERVERS = [];"),
            "ICE defaults to empty again: peers on different networks cannot connect"
        );
        assert!(
            PEER_JS.contains("window.__btIceProbe = function"),
            "no ICE diagnostic, so a run cannot tell a configured list from an answered one"
        );
    }

    /// One function scope: a second `function` declaration of the same name is
    /// not an error, it silently replaces the first. That is how a `getStats`
    /// helper called `candidateType` replaced the counter's, and every gathered
    /// candidate started being counted as `null` — a run of `p2p-turn.dsl` was
    /// the only thing that noticed.
    #[test]
    fn the_browser_half_declares_each_helper_once() {
        let mut seen: Vec<String> = Vec::new();
        for line in PEER_JS.lines() {
            let Some(rest) = line.trim_start().strip_prefix("function ") else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            // `function (` is an anonymous expression; a declaration names a
            // function and opens its arguments.
            if name.is_empty() || !rest[name.len()..].starts_with('(') {
                continue;
            }
            assert!(
                !seen.contains(&name),
                "`{name}` is declared twice, and the second declaration silently replaces the first"
            );
            seen.push(name);
        }
        assert!(seen.len() > 50, "the scan found {seen:?}");
    }

    /// An offer the far side answered belongs to ICE; an offer it did not is
    /// ours to retry. One deadline for both reset the connection every twelve
    /// seconds — shorter than ICE's own give-up — so a pair that could never
    /// connect re-offered forever and `negotiations.failed` never counted one
    /// (design §15). This pins the two deadlines.
    #[test]
    fn the_browser_half_gives_an_answered_offer_time_to_fail() {
        assert!(
            PEER_JS.contains("var ANSWER_MS = answerMs("),
            "an answered attempt has no deadline of its own"
        );
        assert!(
            PEER_JS.contains("var answered = link.remoteSet;"),
            "the sweep cannot tell an answered attempt from an unanswered one"
        );
        assert!(
            PEER_JS.contains("answered ? ANSWER_MS : OFFER_MS"),
            "both cases share one deadline again"
        );
        assert!(
            PEER_JS.contains("'no path after the answer'"),
            "an answered attempt that never connects is not named"
        );
        assert!(
            PEER_JS.contains("link.pc.signalingState !== 'have-local-offer'"),
            "a late answer is applied to a connection with no offer outstanding"
        );
    }

    /// A channel's `error` event is the transport dying, not a fault in the
    /// page: when one end's ICE goes it fires on the end that is still healthy,
    /// and `fail` never clears. Reporting it left `error` set for the rest of a
    /// run after a link had been repaired, so the next reader saw a failure that
    /// was already fixed. Found by the cross-region mesh's repair phase (§12.2):
    /// the link came back in 4.9 s with `failed: 1`, `offered: 4` and `error`
    /// still naming an `RTCErrorEvent`.
    #[test]
    fn the_browser_half_does_not_call_a_dead_channel_an_error() {
        let handler = PEER_JS
            .split("channel.onerror = function")
            .nth(1)
            .expect("the channel has no error handler");
        let body = handler
            .split("channel.onmessage")
            .next()
            .expect("the error handler never ends");
        assert!(
            !body.contains("fail(link"),
            "a channel error is reported as a page failure, which no repair clears"
        );
        assert!(
            body.contains("suspect(link, LINK_SETTLE_MS)"),
            "a channel that errors under a live link is not treated as suspect"
        );
    }

    /// The room's set and the signals it implies arrive on different frames, so
    /// a seat returning to a group can offer before the other end has been told
    /// the set names it. Holding only the offer was not enough: the candidates
    /// that followed it were refused by the same guard and dropped, and by the
    /// time the held offer was replayed the sender's gather had finished — so
    /// the pair had a description, no candidates, and sat in `new` until ICE's
    /// own ~18 s timeout. Found by the cross-region churn run (design §12.2).
    #[test]
    fn the_browser_half_holds_every_signal_that_lands_before_the_set() {
        assert!(
            PEER_JS.contains("deferred.push({ from: from, bytes: bytes, at: performance.now() })"),
            "a signal from a seat the set has not named yet is dropped again"
        );
        assert!(
            PEER_JS.contains(
                "msg.tag === TAG_OFFER || msg.tag === TAG_ANSWER || msg.tag === TAG_CANDIDATE"
            ),
            "only the offer is held, so the candidates around it are lost"
        );
        assert!(
            PEER_JS.contains("signals replayed: the set names it now"),
            "nothing replays a held signal when the set arrives"
        );
        assert!(
            PEER_JS.contains("now - item.at < DEFER_MS"),
            "a held signal never expires, so a stale one could be applied"
        );
    }

    /// §12.2 decides whether colo belongs in the interest set, and it needs the
    /// path a link is actually using. A shim with no `getStats` can only report
    /// that a link is open, which is the same answer for a peer next door and a
    /// peer on another continent.
    #[test]
    fn the_browser_half_samples_the_path() {
        assert!(
            PEER_JS.contains("getStats()"),
            "no candidate-pair statistics, so a report cannot say how far a peer is"
        );
        assert!(
            PEER_JS.contains("candidate-pair"),
            "the sample does not read the pair in use"
        );
        assert!(
            PEER_JS.contains("currentRoundTripTime"),
            "the sample does not read the round trip"
        );
        assert!(
            PEER_JS.contains("pairCandidateType"),
            "the sample does not say whether the pair is direct or relayed"
        );
        assert!(
            PEER_JS.contains("    samplePaths(now);"),
            "the per-frame sweep never samples, so nothing is ever filled in"
        );
    }

    /// TURN's credentials are short-lived and cannot ship in this file, so the
    /// shim asks the room for them (`GET /ice`). A shim that stops asking still
    /// connects — it simply never relays — which is the failure this pins.
    #[test]
    fn the_browser_half_asks_the_room_for_turn() {
        assert!(
            PEER_JS.contains("fetch('/ice'"),
            "the shim does not ask the room for TURN credentials"
        );
        assert!(
            PEER_JS.contains("iceTransportPolicy"),
            "no relay-only mode, so a run cannot prove it crossed a relay"
        );
        assert!(PEER_JS.contains("relay=1"), "the ?relay=1 switch is gone");
        assert!(
            PEER_JS.contains("if (!iceSettled)"),
            "a join no longer waits for the credential list, so a link can be built without TURN"
        );
    }

    /// The shim keeps links only to the room's interest set. A shim that ignores
    /// it still works for two peers and quietly becomes a mesh at sixty-four,
    /// which is the failure this pins.
    #[test]
    fn the_browser_half_keeps_only_the_rooms_group() {
        assert!(
            PEER_JS.contains("window.__btGroup = function"),
            "the room's group is not taken"
        );
        assert!(
            PEER_JS.contains("group.members.has(from)"),
            "a signal from outside the group is accepted"
        );
        assert!(
            PEER_JS.contains("left the group"),
            "a link outside the group is not closed"
        );
    }

    /// The group is the room saying a channel is worth having, so the end that
    /// leads offers on it rather than waiting to be announced at. Waiting is
    /// what left two named peers permanently unlinked: an announce that crosses
    /// the far side's group filter in flight is dropped, and the `seen` guard
    /// then suppresses the echo that would recover it.
    #[test]
    fn the_browser_half_offers_on_the_group() {
        assert!(
            PEER_JS.contains("held === undefined && leadsOffer(seat)"),
            "a group naming an unlinked seat no longer starts an offer"
        );
    }

    /// Persistence keeps a link the room has stopped naming (design §5). The
    /// risk is not the channel, it is the game: a dormant link that still hands
    /// its samples over would draw a player outside the interest set, and one
    /// that still accepts sends would carry state the room has decided is not
    /// worth the bytes. Both directions are refused at the shim's edge.
    #[test]
    fn a_dormant_link_is_silent_in_both_directions() {
        assert!(
            PEER_JS.contains("var PERSIST ="),
            "the persistence switch is gone"
        );
        assert!(
            PEER_JS.contains("window.__bt.idleDropped += 1;"),
            "a dormant link's samples are handed to the game"
        );
        assert!(
            PEER_JS.contains("refuse(seat, kind, 'dormant');"),
            "a dormant link accepts the game's bytes"
        );
        assert!(
            PEER_JS.contains("setIdle(link);"),
            "a seat leaving the group is not held dormant"
        );
    }

    /// The shim has to write the wire format down a second time — the browser
    /// cannot read a Rust constant — so a change to either side must fail here
    /// rather than produce two ends that disagree about what a byte means.
    #[test]
    fn the_browser_half_agrees_with_the_protocol() {
        for (name, value) in [
            ("TAG_ANNOUNCE", TAG_ANNOUNCE),
            ("TAG_OFFER", TAG_OFFER),
            ("TAG_ANSWER", TAG_ANSWER),
            ("TAG_CANDIDATE", TAG_CANDIDATE),
            ("TAG_BYE", TAG_BYE),
            ("TAG_RESTART", TAG_RESTART),
        ] {
            assert!(
                PEER_JS.contains(&format!("var {name} = {value};")),
                "peer.js disagrees about {name}"
            );
        }
        assert!(
            PEER_JS.contains(&format!("var MAX_SDP = {MAX_SDP};")),
            "peer.js disagrees about MAX_SDP"
        );
        assert!(
            PEER_JS.contains(&format!("var MAX_CANDIDATE = {MAX_CANDIDATE};")),
            "peer.js disagrees about MAX_CANDIDATE"
        );
        assert!(
            PEER_JS.contains(&format!("var BYE_REPLACED = {BYE_REPLACED};")),
            "peer.js disagrees about BYE_REPLACED"
        );
    }
}
