//! The negotiation state machine, in Rust.
//!
//! Everything the peer plane *decides* is here: who offers, when a colliding
//! offer is rolled back, which attempt a signal belongs to, what is held until
//! the room's interest set arrives, and when an attempt has taken too long. The
//! browser half keeps the `RTCPeerConnection` and executes what this returns —
//! it holds the async WebRTC API, and it decides nothing.
//!
//! The split matters because the failure mode this replaces was an ordering
//! assumption: signals arrive on different frames from the set that authorises
//! them, and the candidates around a description arrive either side of it. Those
//! are properties of a sequence, so they belong somewhere a sequence can be
//! written down and replayed — a test — rather than in a browser scenario that
//! can only observe the outcome.
//!
//! # Shape
//!
//! * [`Event`] is everything that can happen: a room set, a signal, a promise
//!   completing, a browser state change, a candidate gathered.
//! * [`Action`] is everything the host must do: make an offer, accept one, add a
//!   candidate, send bytes, close, note something.
//! * [`Machine::on`] is a pure function of the machine's state, the event and
//!   the clock. It reads no wall clock and performs no I/O, which is what makes
//!   a whole negotiation replayable in microseconds.
//!
//! Time is a parameter (`now_ms`), never a clock, for the same reason
//! `macroquad-gamekit` takes `dt`: a capture and a soak must do the same work on
//! the same frame on every machine.
//!
//! # Restarts
//!
//! Under the current protocol an ICE restart is its own signal ([`Signal::Restart`])
//! that keeps the epoch of the attempt it belongs to. That is preserved here —
//! retiring it in favour of "a restart is an offer" is a separate change to the
//! wire and to the scenarios that count restarts.
//!
//! # Persistence
//!
//! Dormant links (`?persist=1`) are not modelled yet: leaving the set closes the
//! link, which is the default the scenarios measure. Adding it is a link flag
//! and one branch in [`Machine::on_group`], not a second machine.

use std::collections::BTreeMap;

use crate::{Seat, Signal, leads_offer};

/// The most links one page will hold. The room's set is the first bound on a
/// mesh; this is the second, for a malformed or oversized set.
pub const MAX_LINKS: usize = 12;

/// How long a signal refused because the set has not named its sender is held.
/// Long enough to cover the room's message reaching every seat, short enough
/// that a genuinely stale signal is never applied.
pub const DEFER_MS: u64 = 2_000;

/// How long an offer nobody answered is given before the attempt is abandoned.
pub const OFFER_MS: u64 = 12_000;

/// How long an *answered* attempt is given. Longer than [`OFFER_MS`] because
/// once the far side has answered the attempt belongs to ICE, whose own give-up
/// is around thirty seconds — a shorter deadline here would abandon a pair ICE
/// was still working on and re-offer it for ever.
pub const ANSWER_MS: u64 = 30_000;

/// How long a channel that closed under a live link is given for the far side's
/// `Bye`, so a deliberate close is not mistaken for a crash.
pub const LINK_SETTLE_MS: u64 = 1_000;

/// How long a link may sit `disconnected` before it is treated as dead.
pub const LINK_STALE_MS: u64 = 10_000;

/// The deferral buffer's bound. A gather is tens of candidates and only signals
/// from the last [`DEFER_MS`] survive, so this guards against a malformed sender
/// rather than bounding a working set.
pub const MAX_DEFERRED: usize = 256;

/// The browser's connection state, as it reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Ctrl,
    State,
}

/// Everything that can move the machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The room seated this page. A new seat is a new session.
    Joined(Seat),
    /// The room's interest set for this seat.
    Group { epoch: u32, members: Vec<Seat> },
    /// A signal, as the room delivered it: the sender is the envelope's, never
    /// the body's.
    Signal { from: Seat, bytes: Vec<u8> },
    /// A local candidate was gathered.
    CandidateGathered { seat: Seat, candidate: Vec<u8> },
    /// The browser's connection state changed.
    Pc { seat: Seat, state: PcState },
    /// A data channel opened.
    ChannelOpen { seat: Seat, which: Channel },
    /// A data channel closed.
    ChannelClosed { seat: Seat, which: Channel },
    /// `createOffer` + `setLocalDescription` completed.
    LocalOffer { seat: Seat, sdp: Vec<u8> },
    /// `createAnswer` + `setLocalDescription` completed.
    LocalAnswer { seat: Seat, sdp: Vec<u8> },
    /// `setRemoteDescription` completed. `answer` says which it was, because
    /// accepting an *offer* is what obliges us to produce an answer.
    RemoteSet { seat: Seat, answer: bool },
    /// `setLocalDescription({type: 'rollback'})` completed.
    RolledBack { seat: Seat },
    /// One of the async SDP calls failed.
    SdpError { seat: Seat, why: String },
    /// The frame clock: sweep deadlines.
    Tick,
}

/// Everything the host must do about an [`Event`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Create the channels if needed, then `createOffer` + `setLocalDescription`.
    /// `restart` asks ICE for new credentials on the connection already held.
    Negotiate {
        seat: Seat,
        restart: bool,
    },
    /// `createAnswer` + `setLocalDescription`.
    Answer {
        seat: Seat,
    },
    /// `setRemoteDescription` of an offer (`answer: false`) or an answer.
    Accept {
        seat: Seat,
        answer: bool,
        sdp: Vec<u8>,
    },
    AddCandidate {
        seat: Seat,
        candidate: Vec<u8>,
    },
    /// `setLocalDescription({type: 'rollback'})`.
    Rollback {
        seat: Seat,
    },
    Close {
        seat: Seat,
    },
    /// Send bytes to one seat, or broadcast them (`to: None`).
    Send {
        to: Option<Seat>,
        bytes: Vec<u8>,
    },
    /// The event ring. Never load-bearing; it is what a report is read from.
    Note {
        seat: Seat,
        what: String,
    },
}

/// Counters the scenarios read. One source of truth, in the machine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub offered: u32,
    pub answered: u32,
    pub expired: u32,
    pub failed: u32,
    pub restarts: u32,
    pub ignored: u32,
    /// The most offers this page had in flight at once.
    pub peak: u32,
}

/// One seat's link. Public so a shadow comparison can read it; the machine is
/// the only writer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub seat: Seat,
    /// Our epoch for this pair. A fresh attempt uses a new one; an ICE restart
    /// keeps it.
    pub epoch: u16,
    /// The far side's epoch for this pair, for deciding what is stale.
    pub their_epoch: u16,
    /// This end backs off when both offers collide. The lower seat is impolite.
    pub polite: bool,
    /// Our offer is written and the attempt is not finished with. Set when the
    /// local description is written, cleared by a reset or by connecting.
    pub making_offer: bool,
    /// We ignored a colliding offer, so its candidates are not ours to add.
    pub ignore_offer: bool,
    /// A remote description is applied, so candidates can be added and an
    /// answer belongs to the attempt rather than being stale.
    pub remote_set: bool,
    /// An answer is being applied. Perfect negotiation calls this
    /// `isSettingRemoteAnswerPending`: it is what tells a second answer for the
    /// same offer, arriving while the first is in flight, that it is stale.
    pub answering: bool,
    /// When our outstanding offer was written.
    pub offer_since: u64,
    pub stale_since: u64,
    pub stale_for: u64,
    /// Candidates that arrived before the description they belong to.
    pub pending: Vec<Vec<u8>>,
    pub opens: u16,
    pub ctrl: bool,
    pub state: bool,
    pub pc: PcState,
    pub restarts: u16,
}

impl Link {
    fn new(seat: Seat, epoch: u16, me: Seat) -> Self {
        Self {
            seat,
            epoch,
            their_epoch: 0,
            polite: !leads_offer(me, seat),
            making_offer: false,
            ignore_offer: false,
            remote_set: false,
            answering: false,
            offer_since: 0,
            stale_since: 0,
            stale_for: 0,
            pending: Vec::new(),
            opens: 0,
            ctrl: false,
            state: false,
            pc: PcState::New,
            restarts: 0,
        }
    }

    fn reset(&mut self) {
        self.making_offer = false;
        self.ignore_offer = false;
        self.remote_set = false;
        self.answering = false;
        self.offer_since = 0;
        self.stale_since = 0;
        self.stale_for = 0;
        self.pending.clear();
        self.ctrl = false;
        self.state = false;
        self.opens = 0;
        self.pc = PcState::New;
    }

    fn open(&self) -> bool {
        self.ctrl && self.state
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub epoch: u32,
    pub members: Vec<Seat>,
}

#[derive(Clone, Debug)]
struct Deferred {
    from: Seat,
    bytes: Vec<u8>,
    at: u64,
}

#[derive(Default)]
pub struct Machine {
    me: Option<Seat>,
    group: Option<Group>,
    links: BTreeMap<Seat, Link>,
    deferred: Vec<Deferred>,
    epoch: u16,
    pub stats: Stats,
}

impl Machine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn me(&self) -> Option<Seat> {
        self.me
    }

    pub fn group(&self) -> Option<&Group> {
        self.group.as_ref()
    }

    pub fn links(&self) -> &BTreeMap<Seat, Link> {
        &self.links
    }

    pub fn link(&self, seat: Seat) -> Option<&Link> {
        self.links.get(&seat)
    }

    /// How many links are open on both channels.
    pub fn open(&self) -> usize {
        self.links.values().filter(|link| link.open()).count()
    }

    /// Start a link to `seat`, whoever is asking.
    ///
    /// The room's set uses [`leads_offer`] so only one end of a pair starts by
    /// itself. This is the symmetric entry point: either end may ask, and a
    /// collision is what politeness resolves.
    pub fn connect(&mut self, seat: Seat) -> Vec<Action> {
        let Some(me) = self.me else {
            return Vec::new();
        };
        if seat == me || self.links.contains_key(&seat) || self.links.len() >= MAX_LINKS {
            return Vec::new();
        }
        let epoch = self.next_epoch();
        self.links.insert(seat, Link::new(seat, epoch, me));
        vec![Action::Negotiate {
            seat,
            restart: false,
        }]
    }

    /// Apply one event. Returns everything the host must do, in order.
    pub fn on(&mut self, event: Event, now_ms: u64) -> Vec<Action> {
        match event {
            Event::Joined(seat) => {
                // A seat change is a new session: the room's old set and every
                // record built under it describe a set that is not this seat's.
                self.me = Some(seat);
                self.links.clear();
                self.deferred.clear();
                self.group = None;
                vec![Action::Note {
                    seat,
                    what: "joined".into(),
                }]
            }
            Event::Group { epoch, members } => self.on_group(epoch, members, now_ms),
            Event::Signal { from, bytes } => self.on_signal(from, bytes, now_ms),
            Event::CandidateGathered { seat, candidate } => {
                let Some(link) = self.links.get(&seat) else {
                    return Vec::new();
                };
                // A `Bye` race can leave a candidate for a seat we no longer
                // hold; it is not an error, it is a frame too late.
                if !self.member(seat) {
                    return Vec::new();
                }
                let signal = Signal::Candidate {
                    epoch: link.epoch,
                    to: seat,
                    candidate,
                };
                vec![Action::Send {
                    to: Some(seat),
                    bytes: signal.encode(),
                }]
            }
            Event::Pc { seat, state } => self.on_pc(seat, state, now_ms),
            Event::ChannelOpen { seat, which } => {
                let Some(link) = self.links.get_mut(&seat) else {
                    return Vec::new();
                };
                link.opens += 1;
                match which {
                    Channel::Ctrl => link.ctrl = true,
                    Channel::State => link.state = true,
                }
                let name = match which {
                    Channel::Ctrl => "ctrl open",
                    Channel::State => "state open",
                };
                vec![Action::Note {
                    seat,
                    what: name.into(),
                }]
            }
            Event::ChannelClosed { seat, which } => {
                let Some(link) = self.links.get_mut(&seat) else {
                    return Vec::new();
                };
                match which {
                    Channel::Ctrl => link.ctrl = false,
                    Channel::State => link.state = false,
                }
                // A channel closing under a live link is the transport dying
                // without ICE saying so; a deliberate close has already nulled
                // the record before this queued event runs.
                self.suspect(seat, LINK_SETTLE_MS, now_ms);
                let name = match which {
                    Channel::Ctrl => "ctrl closed",
                    Channel::State => "state closed",
                };
                vec![Action::Note {
                    seat,
                    what: name.into(),
                }]
            }
            Event::LocalOffer { seat, sdp } => {
                let epoch = {
                    let Some(link) = self.links.get_mut(&seat) else {
                        return Vec::new();
                    };
                    link.making_offer = true;
                    link.offer_since = now_ms;
                    link.epoch
                };
                self.stats.offered += 1;
                self.stats.peak = self.stats.peak.max(self.offering());
                let signal = Signal::Offer {
                    epoch,
                    to: seat,
                    sdp,
                };
                vec![Action::Send {
                    to: Some(seat),
                    bytes: signal.encode(),
                }]
            }
            Event::LocalAnswer { seat, sdp } => {
                let Some(link) = self.links.get_mut(&seat) else {
                    return Vec::new();
                };
                self.stats.answered += 1;
                let signal = Signal::Answer {
                    epoch: link.epoch,
                    to: seat,
                    sdp,
                };
                vec![Action::Send {
                    to: Some(seat),
                    bytes: signal.encode(),
                }]
            }
            Event::RemoteSet { seat, answer } => {
                let Some(link) = self.links.get_mut(&seat) else {
                    return Vec::new();
                };
                link.remote_set = true;
                if answer {
                    link.answering = false;
                }
                let pending = std::mem::take(&mut link.pending);
                let mut actions: Vec<Action> = pending
                    .into_iter()
                    .map(|candidate| Action::AddCandidate { seat, candidate })
                    .collect();
                if !answer {
                    // Their offer is ours to answer.
                    actions.push(Action::Answer { seat });
                }
                actions
            }
            Event::RolledBack { seat } => {
                if let Some(link) = self.links.get_mut(&seat) {
                    link.making_offer = false;
                }
                vec![Action::Note {
                    seat,
                    what: "rolled back".into(),
                }]
            }
            Event::SdpError { seat, why } => {
                self.stats.failed += 1;
                let mut actions = vec![Action::Note {
                    seat,
                    what: format!("sdp: {why}"),
                }];
                actions.extend(self.drop_dead(seat, now_ms, "sdp error"));
                actions
            }
            Event::Tick => self.sweep(now_ms),
        }
    }

    fn member(&self, seat: Seat) -> bool {
        self.group
            .as_ref()
            .is_none_or(|group| group.members.contains(&seat))
    }

    fn offering(&self) -> u32 {
        self.links.values().filter(|link| link.making_offer).count() as u32
    }

    fn on_group(&mut self, epoch: u32, members: Vec<Seat>, now_ms: u64) -> Vec<Action> {
        if let Some(group) = &self.group
            && epoch <= group.epoch
        {
            return Vec::new();
        }
        let mut members = members;
        members.sort_unstable();
        members.dedup();
        self.group = Some(Group { epoch, members });

        let Some(me) = self.me else {
            return Vec::new();
        };

        let mut actions = Vec::new();
        let members = self
            .group
            .as_ref()
            .map(|g| g.members.clone())
            .unwrap_or_default();

        // A seat the set no longer names has no link: closing it is what makes a
        // return cost a handshake (persistence would keep it dormant instead).
        let leaving: Vec<Seat> = self
            .links
            .keys()
            .copied()
            .filter(|seat| !members.contains(seat))
            .collect();
        for seat in leaving {
            self.links.remove(&seat);
            actions.push(Action::Note {
                seat,
                what: "left the group".into(),
            });
            actions.push(Action::Close { seat });
        }

        if !members.is_empty() {
            actions.push(Action::Send {
                to: None,
                bytes: Signal::Announce { epoch: self.epoch }.encode(),
            });
        }

        for seat in &members {
            if *seat == me || self.links.contains_key(seat) {
                continue;
            }
            // The set is the room's statement that a channel is worth having, so
            // the leading end opens it without waiting to be announced at.
            if leads_offer(me, *seat) && self.links.len() < MAX_LINKS {
                let epoch = self.next_epoch();
                self.links.insert(*seat, Link::new(*seat, epoch, me));
                actions.push(Action::Negotiate {
                    seat: *seat,
                    restart: false,
                });
            }
        }

        actions.extend(self.replay(now_ms));
        actions
    }

    fn on_signal(&mut self, from: Seat, bytes: Vec<u8>, now_ms: u64) -> Vec<Action> {
        let Some(me) = self.me else {
            return Vec::new();
        };
        if from == me {
            return Vec::new();
        }
        let signal = match Signal::decode(&bytes) {
            Ok(signal) => signal,
            Err(_) => {
                self.stats.ignored += 1;
                return vec![Action::Note {
                    seat: from,
                    what: "signal did not decode".into(),
                }];
            }
        };
        // The room routes a pairwise signal to one seat. The body names the
        // target as a second check, so a mis-routed body is refused here too.
        if let Some(to) = signal.to()
            && to != me
        {
            self.stats.ignored += 1;
            return Vec::new();
        }

        if !self.member(from) && !self.links.contains_key(&from) {
            // The set that names the sender has not arrived. Hold the whole
            // pairwise stream — a description without the candidates around it
            // is a link that never starts — and replay it when the set lands.
            match signal {
                Signal::Offer { .. }
                | Signal::Restart { .. }
                | Signal::Answer { .. }
                | Signal::Candidate { .. } => {
                    if self.deferred.len() < MAX_DEFERRED {
                        self.deferred.push(Deferred {
                            from,
                            bytes,
                            at: now_ms,
                        });
                    }
                    return vec![Action::Note {
                        seat: from,
                        what: "signal deferred: set has not arrived".into(),
                    }];
                }
                Signal::Announce { .. } => {
                    return vec![Action::Note {
                        seat: from,
                        what: "announce dropped: outside the group".into(),
                    }];
                }
                Signal::Bye { .. } => return Vec::new(),
            }
        }

        match signal {
            Signal::Announce { .. } => self.on_announce(from),
            Signal::Offer { epoch, sdp, .. } => self.on_offer(from, epoch, sdp, now_ms),
            Signal::Restart { epoch, sdp, .. } => self.on_restart(from, epoch, sdp, now_ms),
            Signal::Answer { epoch, sdp, .. } => self.on_answer(from, epoch, sdp),
            Signal::Candidate {
                epoch, candidate, ..
            } => self.on_candidate(from, epoch, candidate),
            Signal::Bye { epoch, .. } => self.on_bye(from, epoch, now_ms),
        }
    }

    /// A peer we have not met says it is here. The leading end answers with an
    /// offer; the other end does nothing and waits to be offered to.
    fn on_announce(&mut self, from: Seat) -> Vec<Action> {
        let Some(me) = self.me else {
            return Vec::new();
        };
        if self.links.contains_key(&from) || self.links.len() >= MAX_LINKS {
            return Vec::new();
        }
        if !leads_offer(me, from) {
            return Vec::new();
        }
        let epoch = self.next_epoch();
        self.links.insert(from, Link::new(from, epoch, me));
        vec![Action::Negotiate {
            seat: from,
            restart: false,
        }]
    }

    fn on_offer(&mut self, from: Seat, epoch: u16, sdp: Vec<u8>, now_ms: u64) -> Vec<Action> {
        if !self.links.contains_key(&from) {
            let ours = self.next_epoch();
            let me = self.me.unwrap_or(0);
            self.links.insert(from, Link::new(from, ours, me));
        }
        let link = self.links.get_mut(&from).expect("inserted above");
        if link.open() {
            // A duplicate offer under a working connection. A renegotiation is a
            // higher epoch, and that tore the record down before reaching here.
            return Vec::new();
        }
        if epoch < link.their_epoch {
            // A signal from an attempt they have already replaced.
            return Vec::new();
        }
        let mut actions = Vec::new();
        if epoch > link.their_epoch {
            let first = link.their_epoch == 0;
            link.their_epoch = epoch;
            if !first {
                // They started over: everything held for them describes the old
                // attempt, so it is cleared rather than applied to this one.
                link.reset();
                actions.push(Action::Note {
                    seat: from,
                    what: format!("their epoch moved to {epoch}"),
                });
            }
        }

        if link.making_offer {
            if !link.polite {
                // Both offered and this end wins the collision: our offer stands
                // and theirs is ignored, which also means their candidates are
                // not ours to add.
                link.ignore_offer = true;
                actions.push(Action::Note {
                    seat: from,
                    what: "offer ignored: ours stands".into(),
                });
                return actions;
            }
            // Both offered and this end backs off.
            actions.push(Action::Rollback { seat: from });
        }

        // Taking their offer replaces the browser half of the link: a new
        // connection, new channels, and nothing held from the old attempt.
        link.reset();
        link.their_epoch = epoch;
        actions.push(Action::Accept {
            seat: from,
            answer: false,
            sdp,
        });
        let _ = now_ms;
        actions
    }

    /// An ICE restart on the connection both ends already hold: same pc, same
    /// channels, same epoch. Kept distinct from a fresh offer because the
    /// scenarios count them apart; a later change makes this an ordinary offer
    /// with `iceRestart`.
    fn on_restart(&mut self, from: Seat, epoch: u16, sdp: Vec<u8>, _now_ms: u64) -> Vec<Action> {
        let Some(link) = self.links.get_mut(&from) else {
            return Vec::new();
        };
        if epoch < link.their_epoch {
            return Vec::new();
        }
        link.remote_set = false;
        self.stats.restarts += 1;
        let seat = link.seat;
        link.restarts += 1;
        vec![
            Action::Note {
                seat,
                what: "restart offer".into(),
            },
            Action::Accept {
                seat,
                answer: false,
                sdp,
            },
        ]
    }

    fn on_answer(&mut self, from: Seat, epoch: u16, sdp: Vec<u8>) -> Vec<Action> {
        let Some(link) = self.links.get_mut(&from) else {
            return Vec::new();
        };
        if epoch < link.their_epoch || epoch > link.epoch {
            return Vec::new();
        }
        // An answer is only good for an offer that is still outstanding. One
        // that arrives after the attempt it belongs to was replaced lands on a
        // connection with no offer, and applying it throws — so it is dropped,
        // and the far side re-answers every offer it is sent.
        if !link.making_offer || link.remote_set || link.answering {
            return vec![Action::Note {
                seat: from,
                what: "answer ignored: no offer outstanding".into(),
            }];
        }
        link.answering = true;
        vec![Action::Accept {
            seat: from,
            answer: true,
            sdp,
        }]
    }

    fn on_candidate(&mut self, from: Seat, epoch: u16, candidate: Vec<u8>) -> Vec<Action> {
        let Some(link) = self.links.get_mut(&from) else {
            return Vec::new();
        };
        if epoch < link.their_epoch {
            // A candidate for an attempt that has been replaced. It describes a
            // transport that no longer exists, so it is dropped by identity
            // rather than by how long ago it arrived.
            return Vec::new();
        }
        if link.ignore_offer {
            return Vec::new();
        }
        if !link.remote_set {
            link.pending.push(candidate);
            return Vec::new();
        }
        vec![Action::AddCandidate {
            seat: from,
            candidate,
        }]
    }

    fn on_bye(&mut self, from: Seat, epoch: u16, now_ms: u64) -> Vec<Action> {
        let Some(link) = self.links.get(&from) else {
            return Vec::new();
        };
        if epoch < link.their_epoch {
            return Vec::new();
        }
        let mut actions = vec![Action::Note {
            seat: from,
            what: "they said bye".into(),
        }];
        self.links.remove(&from);
        actions.push(Action::Close { seat: from });
        // The room still names them, so the ordinary paths start again.
        actions.extend(self.reopen(from, now_ms));
        actions
    }

    fn on_pc(&mut self, seat: Seat, state: PcState, now_ms: u64) -> Vec<Action> {
        let Some(link) = self.links.get_mut(&seat) else {
            return Vec::new();
        };
        link.pc = state;
        match state {
            PcState::Failed | PcState::Closed => {
                let mut actions = vec![Action::Note {
                    seat,
                    what: format!("connection {state:?}").to_lowercase(),
                }];
                actions.extend(self.drop_dead(seat, now_ms, "connection failed"));
                actions
            }
            PcState::Disconnected => {
                // May come back on its own: wait out the stale timer, and ask
                // for new ICE credentials first, because that repair keeps the
                // channels.
                self.suspect(seat, LINK_STALE_MS, now_ms);
                vec![Action::Note {
                    seat,
                    what: "connection disconnected".into(),
                }]
            }
            _ => {
                if let Some(link) = self.links.get_mut(&seat) {
                    link.stale_since = 0;
                    link.stale_for = 0;
                    if state == PcState::Connected {
                        // The handshake connected; from here a failure is ICE's
                        // to report, not a deadline's.
                        link.making_offer = false;
                        link.offer_since = 0;
                    }
                }
                Vec::new()
            }
        }
    }

    fn suspect(&mut self, seat: Seat, for_ms: u64, now_ms: u64) {
        if let Some(link) = self.links.get_mut(&seat)
            && link.stale_since == 0
        {
            link.stale_since = now_ms;
            link.stale_for = for_ms;
        }
    }

    /// A link the browser has given up on: forget the record, tell the far side,
    /// and start again if the room still names the seat.
    fn drop_dead(&mut self, seat: Seat, now_ms: u64, why: &str) -> Vec<Action> {
        if self.links.remove(&seat).is_none() {
            return Vec::new();
        }
        self.stats.failed += 1;
        let mut actions = vec![
            Action::Note {
                seat,
                what: format!("reset: {why}"),
            },
            Action::Close { seat },
        ];
        actions.push(Action::Send {
            to: Some(seat),
            bytes: Signal::Bye {
                epoch: self.epoch,
                to: seat,
                reason: crate::BYE_REPLACED,
            }
            .encode(),
        });
        actions.extend(self.reopen(seat, now_ms));
        actions
    }

    /// Start again with a seat the room still names: the leading end offers, the
    /// other announces.
    fn reopen(&mut self, seat: Seat, _now_ms: u64) -> Vec<Action> {
        let Some(me) = self.me else {
            return Vec::new();
        };
        if !self.member(seat) || self.links.contains_key(&seat) || self.links.len() >= MAX_LINKS {
            return Vec::new();
        }
        if leads_offer(me, seat) {
            let epoch = self.next_epoch();
            self.links.insert(seat, Link::new(seat, epoch, me));
            vec![Action::Negotiate {
                seat,
                restart: false,
            }]
        } else {
            vec![Action::Send {
                to: None,
                bytes: Signal::Announce { epoch: self.epoch }.encode(),
            }]
        }
    }

    fn replay(&mut self, now_ms: u64) -> Vec<Action> {
        if self.deferred.is_empty() {
            return Vec::new();
        }
        let mut keep = Vec::new();
        let mut replay = Vec::new();
        for item in std::mem::take(&mut self.deferred) {
            if self.member(item.from) || self.links.contains_key(&item.from) {
                replay.push(item);
            } else if now_ms.saturating_sub(item.at) < DEFER_MS {
                keep.push(item);
            }
        }
        self.deferred = keep;

        let mut actions = Vec::new();
        let mut told: Vec<Seat> = Vec::new();
        for item in replay {
            if !told.contains(&item.from) {
                told.push(item.from);
                actions.push(Action::Note {
                    seat: item.from,
                    what: "signals replayed: the set names it now".into(),
                });
            }
            actions.extend(self.on(
                Event::Signal {
                    from: item.from,
                    bytes: item.bytes,
                },
                now_ms,
            ));
        }
        actions
    }

    /// Deadlines. An offer nobody answered is ours to retry; an offer the far
    /// side answered belongs to ICE, whose own give-up is longer, so one
    /// deadline for both would replace a connection ICE was still working on.
    fn sweep(&mut self, now_ms: u64) -> Vec<Action> {
        let mut actions = Vec::new();

        if self
            .deferred
            .iter()
            .any(|item| now_ms.saturating_sub(item.at) >= DEFER_MS)
        {
            self.deferred
                .retain(|item| now_ms.saturating_sub(item.at) < DEFER_MS);
        }

        // A connection that came up ends the attempt's clock: from here a
        // failure is ICE's to report, not a deadline's.
        for link in self.links.values_mut() {
            if link.open() || link.pc == PcState::Connected {
                link.making_offer = false;
                link.offer_since = 0;
            }
        }

        // Decisions first, mutations after: `reopen` inserts a record for the
        // same seat, and visiting it in the same pass would sweep the new one.
        let mut dead: Vec<(Seat, bool)> = Vec::new();
        let mut stale: Vec<Seat> = Vec::new();
        for (&seat, link) in &self.links {
            if link.open() || link.pc == PcState::Connected {
                continue;
            }
            let deadline = if link.remote_set { ANSWER_MS } else { OFFER_MS };
            if link.making_offer && now_ms.saturating_sub(link.offer_since) >= deadline {
                dead.push((seat, link.remote_set));
                continue;
            }
            if link.stale_since > 0 && now_ms.saturating_sub(link.stale_since) >= link.stale_for {
                stale.push(seat);
            }
        }

        for (seat, answered) in dead {
            if answered {
                // The answer made it our problem no longer; a pair that never
                // found a path is reported as a failure, not as a retry.
                actions.extend(self.drop_dead(seat, now_ms, "no path after the answer"));
            } else {
                self.stats.expired += 1;
                if self.links.remove(&seat).is_some() {
                    actions.push(Action::Note {
                        seat,
                        what: "offer expired".into(),
                    });
                    actions.push(Action::Close { seat });
                    actions.extend(self.reopen(seat, now_ms));
                }
            }
        }
        for seat in stale {
            actions.extend(self.drop_dead(seat, now_ms, "stale"));
        }
        actions
    }

    fn next_epoch(&mut self) -> u16 {
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.epoch = 1;
        }
        self.epoch
    }
}

// ── The executor seam ────────────────────────────────────────────────────────
//
// The machine lives in Rust and the `RTCPeerConnection` lives in the browser, so
// two things cross a byte seam each frame: the events the browser observed, and
// the actions it must run. One tag byte, a seat, then the fields — the same
// length-prefixed shape `Signal` uses, and for the same reason: SDP is full of
// `\r\n` and a text format would double every one of them.
//
// `Action::Send` never crosses: the host turns it into a room message directly.
const EVENT_CANDIDATE: u8 = 1;
const EVENT_PC: u8 = 2;
const EVENT_CHANNEL_OPEN: u8 = 3;
const EVENT_CHANNEL_CLOSED: u8 = 4;
const EVENT_LOCAL_OFFER: u8 = 5;
const EVENT_LOCAL_ANSWER: u8 = 6;
const EVENT_REMOTE_SET: u8 = 7;
const EVENT_ROLLED_BACK: u8 = 8;
const EVENT_SDP_ERROR: u8 = 9;

const ACTION_NEGOTIATE: u8 = 1;
const ACTION_ANSWER: u8 = 2;
const ACTION_ACCEPT: u8 = 3;
const ACTION_ADD_CANDIDATE: u8 = 4;
const ACTION_ROLLBACK: u8 = 5;
const ACTION_CLOSE: u8 = 6;
const ACTION_NOTE: u8 = 7;

/// The longest note or failure reason the seam carries. A browser's error text
/// is short; this bounds a malformed sender rather than a working message.
const MAX_TEXT: usize = 1024;

impl PcState {
    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::New),
            1 => Some(Self::Connecting),
            2 => Some(Self::Connected),
            3 => Some(Self::Disconnected),
            4 => Some(Self::Failed),
            5 => Some(Self::Closed),
            _ => None,
        }
    }
}

impl Channel {
    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Ctrl),
            1 => Some(Self::State),
            _ => None,
        }
    }
}

/// Decode what the browser observed since the last frame. An unknown tag is an
/// error rather than a skip: the two halves are built together, and a mismatch
/// is a version skew worth seeing.
pub fn decode_events(bytes: &[u8]) -> Result<Vec<Event>, crate::SignalError> {
    let mut reader = crate::Reader::new(bytes);
    let mut events = Vec::new();
    while !reader.at_end() {
        let tag = reader.u8()?;
        let seat = reader.u16()?;
        let event = match tag {
            EVENT_CANDIDATE => Event::CandidateGathered {
                seat,
                candidate: reader.blob(crate::MAX_CANDIDATE)?,
            },
            EVENT_PC => {
                let code = reader.u8()?;
                let state = PcState::from_code(code).ok_or(crate::SignalError::UnknownTag(code))?;
                Event::Pc { seat, state }
            }
            EVENT_CHANNEL_OPEN | EVENT_CHANNEL_CLOSED => {
                let code = reader.u8()?;
                let which = Channel::from_code(code).ok_or(crate::SignalError::UnknownTag(code))?;
                if tag == EVENT_CHANNEL_OPEN {
                    Event::ChannelOpen { seat, which }
                } else {
                    Event::ChannelClosed { seat, which }
                }
            }
            EVENT_LOCAL_OFFER => Event::LocalOffer {
                seat,
                sdp: reader.blob(crate::MAX_SDP)?,
            },
            EVENT_LOCAL_ANSWER => Event::LocalAnswer {
                seat,
                sdp: reader.blob(crate::MAX_SDP)?,
            },
            EVENT_REMOTE_SET => Event::RemoteSet {
                seat,
                answer: reader.u8()? != 0,
            },
            EVENT_ROLLED_BACK => Event::RolledBack { seat },
            EVENT_SDP_ERROR => Event::SdpError {
                seat,
                why: String::from_utf8_lossy(&reader.blob(MAX_TEXT)?).into_owned(),
            },
            other => return Err(crate::SignalError::UnknownTag(other)),
        };
        events.push(event);
    }
    Ok(events)
}

/// Encode what the browser must do next. `Action::Send` is the host's, and is
/// dropped here rather than silently sent to the wrong side.
pub fn encode_actions(actions: &[Action]) -> Vec<u8> {
    let mut writer = crate::Writer::default();
    for action in actions {
        match action {
            Action::Negotiate { seat, restart } => {
                writer.u8(ACTION_NEGOTIATE);
                writer.u16(*seat);
                writer.u8(u8::from(*restart));
            }
            Action::Answer { seat } => {
                writer.u8(ACTION_ANSWER);
                writer.u16(*seat);
            }
            Action::Accept { seat, answer, sdp } => {
                writer.u8(ACTION_ACCEPT);
                writer.u16(*seat);
                writer.u8(u8::from(*answer));
                writer.blob(sdp);
            }
            Action::AddCandidate { seat, candidate } => {
                writer.u8(ACTION_ADD_CANDIDATE);
                writer.u16(*seat);
                writer.blob(candidate);
            }
            Action::Rollback { seat } => {
                writer.u8(ACTION_ROLLBACK);
                writer.u16(*seat);
            }
            Action::Close { seat } => {
                writer.u8(ACTION_CLOSE);
                writer.u16(*seat);
            }
            Action::Note { seat, what } => {
                writer.u8(ACTION_NOTE);
                writer.u16(*seat);
                writer.blob(what.as_bytes());
            }
            Action::Send { .. } => {}
        }
    }
    writer.into_bytes()
}

/// The other half, for the tests: the browser decodes this in JavaScript, and a
/// round trip here is what says the two encodings agree.
#[cfg(test)]
fn decode_actions(bytes: &[u8]) -> Vec<Action> {
    let mut reader = crate::Reader::new(bytes);
    let mut actions = Vec::new();
    while !reader.at_end() {
        let tag = reader.u8().expect("tag");
        let seat = reader.u16().expect("seat");
        actions.push(match tag {
            ACTION_NEGOTIATE => Action::Negotiate {
                seat,
                restart: reader.u8().expect("restart") != 0,
            },
            ACTION_ANSWER => Action::Answer { seat },
            ACTION_ACCEPT => Action::Accept {
                seat,
                answer: reader.u8().expect("answer") != 0,
                sdp: reader.blob(crate::MAX_SDP).expect("sdp"),
            },
            ACTION_ADD_CANDIDATE => Action::AddCandidate {
                seat,
                candidate: reader.blob(crate::MAX_CANDIDATE).expect("candidate"),
            },
            ACTION_ROLLBACK => Action::Rollback { seat },
            ACTION_CLOSE => Action::Close { seat },
            ACTION_NOTE => Action::Note {
                seat,
                what: String::from_utf8(reader.blob(MAX_TEXT).expect("note")).expect("utf8"),
            },
            other => panic!("unknown action tag {other}"),
        });
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(me: Seat) -> Machine {
        let mut machine = Machine::new();
        machine.on(Event::Joined(me), 0);
        machine
    }

    fn group(epoch: u32, members: &[Seat]) -> Event {
        Event::Group {
            epoch,
            members: members.to_vec(),
        }
    }

    fn signal(from: Seat, signal: Signal) -> Event {
        Event::Signal {
            from,
            bytes: signal.encode(),
        }
    }

    fn offer(from: Seat, to: Seat, epoch: u16) -> Event {
        signal(
            from,
            Signal::Offer {
                epoch,
                to,
                sdp: b"offer".to_vec(),
            },
        )
    }

    fn answer(from: Seat, to: Seat, epoch: u16) -> Event {
        signal(
            from,
            Signal::Answer {
                epoch,
                to,
                sdp: b"answer".to_vec(),
            },
        )
    }

    fn candidate(from: Seat, to: Seat, epoch: u16, value: &str) -> Event {
        signal(
            from,
            Signal::Candidate {
                epoch,
                to,
                candidate: value.as_bytes().to_vec(),
            },
        )
    }

    fn accepted(actions: &[Action]) -> Option<&Action> {
        actions
            .iter()
            .find(|action| matches!(action, Action::Accept { .. }))
    }

    /// The room's set and the signals it implies arrive on different frames, and
    /// the candidates around a description arrive either side of it. Holding the
    /// offer alone is not enough: a description with no candidates is a link
    /// that never starts, and nothing re-gathers them.
    #[test]
    fn a_signal_that_lands_before_the_set_is_replayed_with_its_candidates() {
        let mut machine = machine(3);
        machine.on(group(1, &[3]), 10);

        let before = machine.on(offer(0, 3, 7), 20);
        assert!(
            accepted(&before).is_none(),
            "an offer from outside the set was accepted"
        );
        let held = machine.on(candidate(0, 3, 7, "c1"), 21);
        assert!(
            held.iter()
                .all(|action| !matches!(action, Action::AddCandidate { .. }))
        );

        let replayed = machine.on(group(2, &[0, 3]), 30);
        let accept = accepted(&replayed).expect("the held offer was not replayed");
        assert!(matches!(accept, Action::Accept { answer: false, .. }));
        assert!(
            replayed
                .iter()
                .all(|action| !matches!(action, Action::AddCandidate { .. })),
            "a held candidate was added before the description it belongs to"
        );

        let set = machine.on(
            Event::RemoteSet {
                seat: 0,
                answer: false,
            },
            40,
        );
        let add = set
            .iter()
            .position(|action| matches!(action, Action::AddCandidate { .. }));
        let reply = set
            .iter()
            .position(|action| matches!(action, Action::Answer { .. }));
        assert!(add.is_some(), "the held candidate was never added");
        assert!(reply.is_some(), "accepting an offer did not answer it");
        assert!(add < reply, "the candidate was added after the answer");
    }

    /// Either end may start. Seat order decides only who backs off when both do.
    #[test]
    fn either_end_may_start_a_link() {
        let mut machine = machine(3);
        let actions = machine.connect(0);
        assert!(matches!(
            actions.as_slice(),
            [Action::Negotiate {
                seat: 0,
                restart: false
            }]
        ));
    }

    /// Both offered. The polite end backs its own offer off and answers.
    #[test]
    fn the_polite_end_rolls_back_and_answers_a_colliding_offer() {
        let mut machine = machine(1);
        machine.connect(0);
        machine.on(
            Event::LocalOffer {
                seat: 0,
                sdp: b"ours".to_vec(),
            },
            10,
        );

        let actions = machine.on(offer(0, 1, 1), 20);
        let rollback = actions
            .iter()
            .position(|action| matches!(action, Action::Rollback { .. }));
        let accept = actions
            .iter()
            .position(|action| matches!(action, Action::Accept { answer: false, .. }));
        assert!(rollback.is_some(), "the polite end kept its own offer");
        assert!(accept.is_some(), "the polite end did not take theirs");
        assert!(
            rollback < accept,
            "the offer was taken before its own was rolled back"
        );
    }

    /// Both offered. The impolite end keeps its offer and ignores theirs — and
    /// with it their candidates, which describe a transport nobody will use.
    #[test]
    fn the_impolite_end_keeps_its_offer_and_ignores_the_collision() {
        let mut machine = machine(0);
        machine.connect(1);
        machine.on(
            Event::LocalOffer {
                seat: 1,
                sdp: b"ours".to_vec(),
            },
            10,
        );

        let actions = machine.on(offer(1, 0, 1), 20);
        assert!(
            accepted(&actions).is_none(),
            "the impolite end took a colliding offer"
        );
        assert!(
            machine
                .link(1)
                .map(|link| link.ignore_offer)
                .unwrap_or(false)
        );

        let after = machine.on(candidate(1, 0, 1, "theirs"), 21);
        assert!(
            after
                .iter()
                .all(|action| !matches!(action, Action::AddCandidate { .. })),
            "a candidate from the ignored offer was added"
        );
    }

    /// One offer, one answer. A second answer is a stale one and applying it
    /// would throw; the far side re-answers every offer it is sent.
    #[test]
    fn an_answer_is_taken_once_and_only_for_the_offer_it_belongs_to() {
        let mut machine = machine(0);
        machine.connect(1);
        let epoch = machine.link(1).expect("no link").epoch;
        machine.on(
            Event::LocalOffer {
                seat: 1,
                sdp: b"offer".to_vec(),
            },
            10,
        );

        let first = machine.on(answer(1, 0, epoch), 20);
        assert!(matches!(
            accepted(&first),
            Some(Action::Accept { answer: true, .. })
        ));

        let second = machine.on(answer(1, 0, epoch), 21);
        assert!(
            accepted(&second).is_none(),
            "a second answer was applied to the same offer"
        );
    }

    /// Candidates belong to an attempt, not to a moment: one from an attempt the
    /// far side has replaced describes a transport that no longer exists.
    #[test]
    fn a_candidate_from_a_replaced_attempt_is_dropped_not_applied() {
        let mut machine = machine(0);
        machine.on(offer(1, 0, 1), 10);
        machine.on(
            Event::RemoteSet {
                seat: 1,
                answer: false,
            },
            11,
        );

        machine.on(offer(1, 0, 2), 20);
        let stale = machine.on(candidate(1, 0, 1, "old"), 21);
        assert!(
            stale
                .iter()
                .all(|action| !matches!(action, Action::AddCandidate { .. })),
            "an old attempt's candidate was added"
        );

        let early = machine.on(candidate(1, 0, 2, "new"), 22);
        assert!(
            early
                .iter()
                .all(|action| !matches!(action, Action::AddCandidate { .. })),
            "a candidate was added before the description it belongs to"
        );
        let set = machine.on(
            Event::RemoteSet {
                seat: 1,
                answer: false,
            },
            23,
        );
        assert!(
            set.iter()
                .any(|action| matches!(action, Action::AddCandidate { .. }))
        );
    }

    /// An offer nobody answered is ours to retry on its own deadline.
    #[test]
    fn an_unanswered_offer_expires_and_is_offered_again() {
        let mut machine = machine(0);
        machine.on(group(1, &[0, 1]), 10);
        machine.on(
            Event::LocalOffer {
                seat: 1,
                sdp: b"offer".to_vec(),
            },
            20,
        );

        let early = machine.on(Event::Tick, 20 + OFFER_MS - 1);
        assert!(
            early
                .iter()
                .all(|action| !matches!(action, Action::Negotiate { .. }))
        );

        let late = machine.on(Event::Tick, 20 + OFFER_MS);
        assert!(
            late.iter()
                .any(|action| matches!(action, Action::Negotiate { .. })),
            "the expired offer was not retried"
        );
        assert_eq!(machine.stats.expired, 1);
    }

    /// An answered attempt belongs to ICE, whose own give-up is longer than the
    /// offer deadline. Replacing it on that deadline would tear down a pair ICE
    /// was still working on.
    #[test]
    fn an_answered_attempt_is_given_ice_s_own_time_to_fail() {
        let mut machine = machine(0);
        machine.connect(1);
        let epoch = machine.link(1).expect("no link").epoch;
        machine.on(
            Event::LocalOffer {
                seat: 1,
                sdp: b"offer".to_vec(),
            },
            10,
        );
        machine.on(answer(1, 0, epoch), 20);
        machine.on(
            Event::RemoteSet {
                seat: 1,
                answer: true,
            },
            21,
        );

        let early = machine.on(Event::Tick, 20 + OFFER_MS);
        assert!(
            early
                .iter()
                .all(|action| !matches!(action, Action::Negotiate { .. })),
            "an answered attempt was retried on the offer deadline"
        );

        let late = machine.on(Event::Tick, 20 + ANSWER_MS);
        assert!(
            late.iter()
                .any(|action| matches!(action, Action::Negotiate { .. })),
            "an answered attempt that never connected was not replaced"
        );
        assert_eq!(machine.stats.failed, 1);
    }

    #[test]
    fn a_seat_that_leaves_the_set_loses_its_link() {
        let mut machine = machine(0);
        machine.on(group(1, &[0, 1]), 10);
        assert!(machine.link(1).is_some());

        let actions = machine.on(group(2, &[0]), 20);
        assert!(machine.link(1).is_none());
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, Action::Close { seat: 1 }))
        );
    }

    /// The seam is two encodings that must agree. This round trip is the whole
    /// contract: every action the machine can return survives it, except `Send`,
    /// which the host consumes rather than the browser.
    #[test]
    fn the_actions_round_trip_through_the_executor_seam() {
        let actions = vec![
            Action::Negotiate {
                seat: 3,
                restart: false,
            },
            Action::Answer { seat: 1 },
            Action::Accept {
                seat: 2,
                answer: true,
                sdp: b"v=0\r\n".to_vec(),
            },
            Action::AddCandidate {
                seat: 4,
                candidate: b"candidate:1 1 udp 1 1.2.3.4 5 typ host".to_vec(),
            },
            Action::Rollback { seat: 5 },
            Action::Close { seat: 6 },
            Action::Note {
                seat: 7,
                what: "offer expired".into(),
            },
        ];
        let encoded = encode_actions(&actions);
        assert_eq!(decode_actions(&encoded), actions);
    }

    #[test]
    fn a_send_action_is_the_hosts_and_never_crosses_the_seam() {
        let encoded = encode_actions(&[Action::Send {
            to: Some(1),
            bytes: b"x".to_vec(),
        }]);
        assert!(encoded.is_empty(), "{encoded:?}");
    }

    #[test]
    fn the_browser_events_decode_in_order() {
        // Built by hand, the way the shim writes them: tag, seat, then fields.
        let mut writer = crate::Writer::default();
        writer.u8(EVENT_PC);
        writer.u16(2);
        writer.u8(2);
        writer.u8(EVENT_CHANNEL_OPEN);
        writer.u16(2);
        writer.u8(0);
        writer.u8(EVENT_LOCAL_OFFER);
        writer.u16(2);
        writer.blob(b"v=0\r\n");
        writer.u8(EVENT_REMOTE_SET);
        writer.u16(2);
        writer.u8(1);
        writer.u8(EVENT_ROLLED_BACK);
        writer.u16(2);
        writer.u8(EVENT_SDP_ERROR);
        writer.u16(2);
        writer.blob(b"called in wrong state");

        let events = decode_events(&writer.into_bytes()).expect("a well-formed stream");
        assert_eq!(
            events,
            vec![
                Event::Pc {
                    seat: 2,
                    state: PcState::Connected
                },
                Event::ChannelOpen {
                    seat: 2,
                    which: Channel::Ctrl
                },
                Event::LocalOffer {
                    seat: 2,
                    sdp: b"v=0\r\n".to_vec()
                },
                Event::RemoteSet {
                    seat: 2,
                    answer: true
                },
                Event::RolledBack { seat: 2 },
                Event::SdpError {
                    seat: 2,
                    why: "called in wrong state".into()
                },
            ]
        );
    }

    #[test]
    fn a_truncated_event_stream_is_refused() {
        let mut writer = crate::Writer::default();
        writer.u8(EVENT_LOCAL_OFFER);
        writer.u16(1);
        writer.u16(64); // says 64 bytes follow; none do
        assert!(decode_events(&writer.into_bytes()).is_err());
    }

    #[test]
    fn an_unknown_event_tag_is_refused_rather_than_skipped() {
        let mut writer = crate::Writer::default();
        writer.u8(99);
        writer.u16(1);
        assert!(decode_events(&writer.into_bytes()).is_err());
    }
}
