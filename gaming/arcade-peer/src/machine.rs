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
//! # Telemetry
//!
//! A link's path is a decision too. [`PairRow`] is the browser's candidate-pair
//! report as data — every row, with its state — and the machine picks the one
//! carrying the link and converts the round trip to tenths of a millisecond.
//! [`Path`] is the answer, and [`Action::Path`] hands it back for the readout.
//! Nothing in the machine branches on it.
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

/// The most candidate-pair rows one report may carry. A browser reports one row
/// per local/remote pair, which is a dozen for a page that gathered host,
/// server-reflexive and relay candidates at once; this bounds a malformed one.
pub const MAX_PAIRS: usize = 64;

/// How often one link may be asked to restart its ICE. A restart is the cheap
/// repair, not a retry loop: one that does not take falls through to the stale
/// timer, which is the repair that costs the channels.
pub const RESTART_EVERY_MS: u64 = 5_000;

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

/// One end of a candidate pair, as the browser's report names it. `relay` on
/// either end is what makes a round trip longer than the distance between the
/// two players, and is what `p2p-turn.dsl` turns on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CandidateType {
    /// The report held no candidate for this side of the pair.
    #[default]
    Unknown,
    Host,
    /// Server reflexive: the address a STUN server saw.
    Srflx,
    /// Peer reflexive: an address learned from the far side's checks.
    Prflx,
    Relay,
}

/// A candidate pair's state in the browser's report. Only a `succeeded` pair
/// can be carrying the link; a report always holds the failed and in-progress
/// ones beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairState {
    Unknown,
    Waiting,
    InProgress,
    Failed,
    Succeeded,
    Frozen,
}

/// One row of a browser's candidate-pair report, as reported. Which row is the
/// path is the machine's decision, not the browser's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PairRow {
    pub state: PairState,
    /// Chrome marks the pair in use `selected`.
    pub selected: bool,
    /// Older builds and other browsers mark it `nominated` instead.
    pub nominated: bool,
    pub bytes_sent: u64,
    /// `currentRoundTripTime`, in seconds, as the browser measured it.
    pub rtt_seconds: Option<f64>,
    pub local: CandidateType,
    pub remote: CandidateType,
}

/// A link's chosen path, as the last report described it. Telemetry: nothing in
/// the machine branches on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Path {
    /// Tenths of a millisecond: loopback reads 0.1–0.5, and rounding to whole
    /// milliseconds would call every local pair zero.
    pub rtt_tenths: Option<u16>,
    pub local: CandidateType,
    pub remote: CandidateType,
}

/// Everything that can move the machine.
///
/// `PartialEq` but not `Eq`: a stats report carries the browser's own
/// `currentRoundTripTime`, which is a float.
#[derive(Clone, Debug, PartialEq)]
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
    /// A `getStats` report's candidate pairs, as the browser measured them.
    /// Which pair is the link, and what its round trip is in tenths of a
    /// millisecond, is this machine's to decide.
    StatsReport { seat: Seat, pairs: Vec<PairRow> },
    /// The page asked for an ICE restart on a link — the harness seam, and the
    /// same request [`Machine::restart`] takes. Whether it is this end's to ask
    /// is the machine's to judge.
    Restart { seat: Seat },
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
    /// `createAnswer` + `setLocalDescription`. Whether it answers a restart is
    /// the machine's bookkeeping — the page's counters come from [`Action::Stats`],
    /// not from the shim counting the actions it ran.
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
    /// The path the link is running on, for the readout. The machine has already
    /// decided it; the browser only assigns it.
    Path {
        seat: Seat,
        path: Path,
    },
    /// The counters, whenever they move. Not addressed to a link: this is the
    /// page's readout, and under `?machine=1` it is the machine's numbers rather
    /// than a second set the shim kept beside them.
    Stats(Stats),
}

/// Counters the scenarios read. One source of truth, in the machine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub offered: u32,
    pub answered: u32,
    pub expired: u32,
    pub failed: u32,
    /// Restarts this page asked for.
    pub restarts: u32,
    /// Restarts this page answered.
    pub restart_answered: u32,
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
    /// Restarts this link has been through, asked or taken.
    pub restarts: u16,
    /// We asked for an ICE restart and the offer it produced has not been sent.
    pub restart_asked: bool,
    /// We applied their restart, so the answer we send is a restart's answer
    /// rather than one to a new attempt.
    pub restart_taken: bool,
    /// When this link last asked for a restart, for the rate limit.
    pub restart_since: u64,
    pub path: Path,
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
            restart_asked: false,
            restart_taken: false,
            restart_since: 0,
            path: Path::default(),
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
        // A fresh attempt is a new browser half, so a restart owed on the old
        // one is moot and the path describes a transport that is gone. The rate
        // limit is kept: it belongs to the link, not to the attempt.
        self.restart_asked = false;
        self.restart_taken = false;
        self.path = Path::default();
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

    /// Ask for an ICE restart on a live link: new credentials, the same
    /// connection, the same data channels.
    ///
    /// Only the leading end asks, which is the rule that decides who offers —
    /// two ends cannot restart at each other. Nothing here is timed by
    /// [`OFFER_MS`]: a restart that does not take falls through to the stale
    /// timer, which tears the link down properly and costs the channels, so
    /// retrying one on the offer deadline would spend the repair that exists to
    /// keep them.
    pub fn restart(&mut self, now_ms: u64, seat: Seat) -> Vec<Action> {
        let Some(link) = self.links.get(&seat) else {
            return Vec::new();
        };
        // `polite` is `!leads_offer(me, seat)`, so this is the leading end.
        if !link.open() || link.polite || link.restart_asked {
            return Vec::new();
        }
        if link.restart_since > 0 && now_ms.saturating_sub(link.restart_since) < RESTART_EVERY_MS {
            return Vec::new();
        }
        let link = self.links.get_mut(&seat).expect("checked above");
        link.restart_since = now_ms;
        link.restart_asked = true;
        link.restarts += 1;
        self.stats.restarts += 1;
        vec![Action::Negotiate {
            seat,
            restart: true,
        }]
    }

    /// Apply one event. Returns everything the host must do, in order.
    ///
    /// The counters are published from here rather than from each site that
    /// bumps one: a `Stats` action is appended whenever a reduction moved them,
    /// so a new counter cannot be added and forgotten on the other side of the
    /// seam.
    pub fn on(&mut self, event: Event, now_ms: u64) -> Vec<Action> {
        let before = self.stats;
        let mut actions = self.reduce(event, now_ms);
        if self.stats != before {
            actions.push(Action::Stats(self.stats));
        }
        actions
    }

    fn reduce(&mut self, event: Event, now_ms: u64) -> Vec<Action> {
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
                let (epoch, restart) = {
                    let Some(link) = self.links.get_mut(&seat) else {
                        return Vec::new();
                    };
                    // A restart's offer is not a new attempt: it carries the
                    // epoch of the one it repairs and it is not counted as an
                    // offer. What it must not do is start the offer clock.
                    let restart = std::mem::take(&mut link.restart_asked);
                    if !restart {
                        link.making_offer = true;
                        link.offer_since = now_ms;
                    }
                    (link.epoch, restart)
                };
                if !restart {
                    self.stats.offered += 1;
                    self.stats.peak = self.stats.peak.max(self.offering());
                }
                let signal = if restart {
                    Signal::Restart {
                        epoch,
                        to: seat,
                        sdp,
                    }
                } else {
                    Signal::Offer {
                        epoch,
                        to: seat,
                        sdp,
                    }
                };
                vec![Action::Send {
                    to: Some(seat),
                    bytes: signal.encode(),
                }]
            }
            Event::LocalAnswer { seat, sdp } => {
                let (epoch, restart) = {
                    let Some(link) = self.links.get_mut(&seat) else {
                        return Vec::new();
                    };
                    (link.epoch, std::mem::take(&mut link.restart_taken))
                };
                if !restart {
                    self.stats.answered += 1;
                }
                let signal = Signal::Answer {
                    epoch,
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
            Event::StatsReport { seat, pairs } => self.on_stats(seat, &pairs),
            Event::Restart { seat } => self.restart(now_ms, seat),
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
        if link.pc == PcState::Closed {
            // Nothing to restart. The end that asked reaches its stale timer and
            // starts over, which is the only repair available from here.
            return Vec::new();
        }
        link.remote_set = false;
        link.restart_taken = true;
        link.restarts += 1;
        self.stats.restart_answered += 1;
        let seat = link.seat;
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

    /// The browser's own view of the path, reduced here rather than in the
    /// shim: which of the pairs it reported is the one carrying the link, and
    /// what it costs in tenths of a millisecond.
    fn on_stats(&mut self, seat: Seat, pairs: &[PairRow]) -> Vec<Action> {
        let Some(link) = self.links.get_mut(&seat) else {
            return Vec::new();
        };
        let Some(pair) = best_pair(pairs) else {
            // A report with no succeeded pair is ordinary: the candidate checks
            // are still running, and the last good sample still describes the
            // path the link was on.
            return Vec::new();
        };
        let path = Path {
            rtt_tenths: pair.rtt_seconds.and_then(tenths),
            local: pair.local,
            remote: pair.remote,
        };
        if link.path == path {
            return Vec::new();
        }
        link.path = path;
        vec![Action::Path { seat, path }]
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
                let mut actions = vec![Action::Note {
                    seat,
                    what: "connection disconnected".into(),
                }];
                actions.extend(self.restart(now_ms, seat));
                actions
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
            actions.extend(self.reduce(
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

/// The pair carrying the link, from a report the browser handed over whole.
///
/// Chrome marks the pair in use `selected`; older builds and other browsers use
/// `nominated`, so the marks are a score rather than a flag, and the pair that
/// has carried the most bytes breaks a tie. Only a `succeeded` pair can carry
/// anything — a report always holds the failed and in-progress ones beside it —
/// which is why the filter lives here and not in the shim.
fn best_pair(pairs: &[PairRow]) -> Option<&PairRow> {
    pairs
        .iter()
        .filter(|pair| pair.state == PairState::Succeeded)
        .max_by_key(|pair| {
            let marks = u8::from(pair.selected) * 2 + u8::from(pair.nominated);
            (marks, pair.bytes_sent)
        })
}

/// Tenths of a millisecond from the browser's seconds. A negative or
/// non-finite sample is not a round trip, so it is not reported rather than
/// reported as zero.
fn tenths(seconds: f64) -> Option<u16> {
    let tenths = (seconds * 10_000.0).round();
    if !tenths.is_finite() || !(0.0..=f64::from(u16::MAX)).contains(&tenths) {
        return None;
    }
    Some(tenths as u16)
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
// Every action but `Action::Stats` starts tag, seat — the counters name no link.
const EVENT_CANDIDATE: u8 = 1;
const EVENT_PC: u8 = 2;
const EVENT_CHANNEL_OPEN: u8 = 3;
const EVENT_CHANNEL_CLOSED: u8 = 4;
const EVENT_LOCAL_OFFER: u8 = 5;
const EVENT_LOCAL_ANSWER: u8 = 6;
const EVENT_REMOTE_SET: u8 = 7;
const EVENT_ROLLED_BACK: u8 = 8;
const EVENT_SDP_ERROR: u8 = 9;
const EVENT_STATS: u8 = 10;
const EVENT_RESTART: u8 = 11;

const ACTION_NEGOTIATE: u8 = 1;
const ACTION_ANSWER: u8 = 2;
const ACTION_ACCEPT: u8 = 3;
const ACTION_ADD_CANDIDATE: u8 = 4;
const ACTION_ROLLBACK: u8 = 5;
const ACTION_CLOSE: u8 = 6;
const ACTION_NOTE: u8 = 7;
const ACTION_PATH: u8 = 8;
const ACTION_STATS: u8 = 9;

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

impl CandidateType {
    fn code(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Host => 1,
            Self::Srflx => 2,
            Self::Prflx => 3,
            Self::Relay => 4,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Unknown),
            1 => Some(Self::Host),
            2 => Some(Self::Srflx),
            3 => Some(Self::Prflx),
            4 => Some(Self::Relay),
            _ => None,
        }
    }
}

impl PairState {
    /// Only a test builds a report by hand; the shim writes the real one.
    #[cfg(test)]
    fn code(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Waiting => 1,
            Self::InProgress => 2,
            Self::Failed => 3,
            Self::Succeeded => 4,
            Self::Frozen => 5,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Unknown),
            1 => Some(Self::Waiting),
            2 => Some(Self::InProgress),
            3 => Some(Self::Failed),
            4 => Some(Self::Succeeded),
            5 => Some(Self::Frozen),
            _ => None,
        }
    }
}

/// One row's flags: which of the two marks the report carries, and whether it
/// measured a round trip at all.
const PAIR_SELECTED: u8 = 1;
const PAIR_NOMINATED: u8 = 2;
const PAIR_RTT: u8 = 4;

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
            EVENT_STATS => {
                let rows = usize::from(reader.u16()?);
                if rows > MAX_PAIRS {
                    return Err(crate::SignalError::TooLong);
                }
                let mut pairs = Vec::with_capacity(rows);
                for _ in 0..rows {
                    let flags = reader.u8()?;
                    let state =
                        PairState::from_code(reader.u8()?).ok_or(crate::SignalError::TooLong)?;
                    let bytes_sent = reader.u64()?;
                    let rtt_seconds = reader.f64()?;
                    let local = CandidateType::from_code(reader.u8()?)
                        .ok_or(crate::SignalError::TooLong)?;
                    let remote = CandidateType::from_code(reader.u8()?)
                        .ok_or(crate::SignalError::TooLong)?;
                    pairs.push(PairRow {
                        state,
                        selected: flags & PAIR_SELECTED != 0,
                        nominated: flags & PAIR_NOMINATED != 0,
                        bytes_sent,
                        rtt_seconds: (flags & PAIR_RTT != 0).then_some(rtt_seconds),
                        local,
                        remote,
                    });
                }
                Event::StatsReport { seat, pairs }
            }
            EVENT_RESTART => Event::Restart { seat },
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
            Action::Path { seat, path } => {
                writer.u8(ACTION_PATH);
                writer.u16(*seat);
                writer.u8(u8::from(path.rtt_tenths.is_some()));
                writer.u16(path.rtt_tenths.unwrap_or(0));
                writer.u8(path.local.code());
                writer.u8(path.remote.code());
            }
            Action::Stats(stats) => {
                // Field order is `Stats`'s declaration order, and the shim's
                // `applyStats` reads the same eight in the same order.
                writer.u8(ACTION_STATS);
                writer.u32(stats.offered);
                writer.u32(stats.answered);
                writer.u32(stats.expired);
                writer.u32(stats.failed);
                writer.u32(stats.restarts);
                writer.u32(stats.restart_answered);
                writer.u32(stats.ignored);
                writer.u32(stats.peak);
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
        // `Stats` is the page's own counters and names no link, so it is the one
        // action that does not start tag, seat.
        if tag == ACTION_STATS {
            actions.push(Action::Stats(Stats {
                offered: reader.u32().expect("offered"),
                answered: reader.u32().expect("answered"),
                expired: reader.u32().expect("expired"),
                failed: reader.u32().expect("failed"),
                restarts: reader.u32().expect("restarts"),
                restart_answered: reader.u32().expect("restart answered"),
                ignored: reader.u32().expect("ignored"),
                peak: reader.u32().expect("peak"),
            }));
            continue;
        }
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
            ACTION_PATH => {
                let has_rtt = reader.u8().expect("rtt flag") != 0;
                let rtt = reader.u16().expect("rtt");
                let local =
                    CandidateType::from_code(reader.u8().expect("local")).expect("local type");
                let remote =
                    CandidateType::from_code(reader.u8().expect("remote")).expect("remote type");
                Action::Path {
                    seat,
                    path: Path {
                        rtt_tenths: has_rtt.then_some(rtt),
                        local,
                        remote,
                    },
                }
            }
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

    /// The one signal an action list asks the host to send, decoded.
    fn sent(actions: &[Action]) -> Option<Signal> {
        actions.iter().find_map(|action| match action {
            Action::Send { bytes, .. } => Signal::decode(bytes).ok(),
            _ => None,
        })
    }

    /// A machine with one link to `them`, open on both channels and connected —
    /// the state every restart case starts from. Built by whichever path the
    /// seats allow, because both ends need a link for the two halves of a restart.
    fn opened(me: Seat, them: Seat) -> Machine {
        let mut machine = machine(me);
        machine.on(group(1, &[me, them]), 10);
        if machine.link(them).is_none() {
            machine.connect(them);
        }
        let epoch = machine.link(them).expect("a link").epoch;
        machine.on(
            Event::LocalOffer {
                seat: them,
                sdp: b"offer".to_vec(),
            },
            20,
        );
        machine.on(answer(them, me, epoch), 30);
        machine.on(
            Event::RemoteSet {
                seat: them,
                answer: true,
            },
            31,
        );
        machine.on(
            Event::Pc {
                seat: them,
                state: PcState::Connected,
            },
            32,
        );
        machine.on(
            Event::ChannelOpen {
                seat: them,
                which: Channel::Ctrl,
            },
            33,
        );
        machine.on(
            Event::ChannelOpen {
                seat: them,
                which: Channel::State,
            },
            34,
        );
        assert!(machine.link(them).expect("a link").open());
        machine
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
            Action::Path {
                seat: 9,
                path: Path {
                    rtt_tenths: Some(12),
                    local: CandidateType::Relay,
                    remote: CandidateType::Host,
                },
            },
            Action::Path {
                seat: 10,
                path: Path::default(),
            },
            Action::Stats(Stats {
                offered: 3,
                answered: 2,
                expired: 1,
                failed: 1,
                restarts: 1,
                restart_answered: 1,
                ignored: 4,
                peak: 2,
            }),
            Action::Stats(Stats::default()),
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

    fn pair(
        state: PairState,
        selected: bool,
        nominated: bool,
        bytes_sent: u64,
        rtt_seconds: Option<f64>,
        local: CandidateType,
        remote: CandidateType,
    ) -> PairRow {
        PairRow {
            state,
            selected,
            nominated,
            bytes_sent,
            rtt_seconds,
            local,
            remote,
        }
    }

    /// The report is the browser's; which row is the link is ours. A failed pair
    /// with more bytes on it than the live one must not win, and the round trip
    /// is rounded here rather than there.
    #[test]
    fn the_report_is_reduced_to_the_pair_carrying_the_link() {
        let mut machine = opened(0, 1);

        let actions = machine.on(
            Event::StatsReport {
                seat: 1,
                pairs: vec![
                    pair(
                        PairState::Failed,
                        false,
                        false,
                        9_000,
                        Some(2.0),
                        CandidateType::Relay,
                        CandidateType::Relay,
                    ),
                    pair(
                        PairState::Succeeded,
                        false,
                        true,
                        900,
                        Some(0.05),
                        CandidateType::Srflx,
                        CandidateType::Host,
                    ),
                    pair(
                        PairState::Succeeded,
                        true,
                        false,
                        100,
                        Some(0.0123),
                        CandidateType::Host,
                        CandidateType::Srflx,
                    ),
                ],
            },
            1_000,
        );

        let expected = Path {
            rtt_tenths: Some(123),
            local: CandidateType::Host,
            remote: CandidateType::Srflx,
        };
        assert_eq!(
            actions,
            vec![Action::Path {
                seat: 1,
                path: expected
            }],
            "the report was reduced to the wrong pair"
        );
        assert_eq!(machine.link(1).expect("a link").path, expected);

        // A report that says nothing new is not worth a frame's action.
        let again = machine.on(
            Event::StatsReport {
                seat: 1,
                pairs: vec![pair(
                    PairState::Succeeded,
                    true,
                    false,
                    200,
                    Some(0.0123),
                    CandidateType::Host,
                    CandidateType::Srflx,
                )],
            },
            2_000,
        );
        assert!(again.is_empty(), "an unchanged path was re-reported");
    }

    /// A pair that never succeeded carries nothing, and a report full of them is
    /// the ordinary state of a connection whose checks are still running.
    #[test]
    fn a_report_with_no_succeeded_pair_leaves_the_path_alone() {
        let mut machine = opened(0, 1);
        let actions = machine.on(
            Event::StatsReport {
                seat: 1,
                pairs: vec![pair(
                    PairState::Waiting,
                    false,
                    false,
                    0,
                    None,
                    CandidateType::Host,
                    CandidateType::Host,
                )],
            },
            1_000,
        );
        assert!(actions.is_empty());
        assert_eq!(machine.link(1).expect("a link").path, Path::default());
    }

    /// A restart repairs the connection both ends already hold. The offer it
    /// produces is a `Restart`, not an offer, and it is not counted as one — a
    /// far side that read it as a new attempt would tear down the channels the
    /// restart exists to keep.
    #[test]
    fn a_restart_repairs_the_live_connection_and_is_not_an_offer() {
        let mut machine = opened(0, 1);
        let epoch = machine.link(1).expect("a link").epoch;
        let offered = machine.stats.offered;

        let asked = machine.restart(10_000, 1);
        assert_eq!(
            asked,
            vec![Action::Negotiate {
                seat: 1,
                restart: true
            }]
        );
        assert_eq!(machine.stats.restarts, 1);

        let restart_offer = machine.on(
            Event::LocalOffer {
                seat: 1,
                sdp: b"restart".to_vec(),
            },
            10_010,
        );
        let Some(Signal::Restart {
            epoch: carried,
            to: 1,
            ..
        }) = sent(&restart_offer)
        else {
            panic!("a restart was sent as something else: {restart_offer:?}");
        };
        assert_eq!(carried, epoch, "a restart moved the attempt's epoch");
        assert_eq!(
            machine.stats.offered, offered,
            "a restart was counted as an offer"
        );
    }

    /// The other half: a restart is answered on the connection already up, and
    /// answering one is not answering a new offer.
    #[test]
    fn the_far_side_answers_a_restart_without_answering_a_new_offer() {
        let mut machine = opened(1, 0);
        let epoch = machine.link(0).expect("a link").epoch;
        let answered = machine.stats.answered;

        let taken = machine.on(
            signal(
                0,
                Signal::Restart {
                    epoch,
                    to: 1,
                    sdp: b"restart".to_vec(),
                },
            ),
            10_000,
        );
        assert!(matches!(
            accepted(&taken),
            Some(Action::Accept { answer: false, .. })
        ));
        assert_eq!(machine.stats.restart_answered, 1);

        // Answering a restart is still an answer here; what must not move is
        // the counter, which the machine keeps rather than the page.
        let reply = machine.on(
            Event::RemoteSet {
                seat: 0,
                answer: false,
            },
            10_001,
        );
        assert!(
            reply.iter().any(|action| matches!(action, Action::Answer { .. })),
            "a restart's offer was never answered"
        );

        machine.on(
            Event::LocalAnswer {
                seat: 0,
                sdp: b"answer".to_vec(),
            },
            10_002,
        );
        assert_eq!(
            machine.stats.answered, answered,
            "answering a restart was counted as answering an offer"
        );
    }

    /// One end restarts, and not on every frame: a restart is the cheap repair,
    /// and one that does not take falls through to the stale timer instead.
    #[test]
    fn only_the_leading_end_restarts_and_not_on_every_frame() {
        let mut polite = opened(1, 0);
        assert!(
            polite.restart(10_000, 0).is_empty(),
            "the backing-off end asked to restart"
        );

        let mut leader = opened(0, 1);
        assert_eq!(leader.restart(10_000, 1).len(), 1);
        assert!(
            leader.restart(10_000, 1).is_empty(),
            "a restart was asked for while one was outstanding"
        );

        // The first restart's offer comes back, which is what lets another be
        // asked at all. Inside the window it is still refused.
        leader.on(
            Event::LocalOffer {
                seat: 1,
                sdp: b"restart".to_vec(),
            },
            10_010,
        );
        assert!(
            leader.restart(10_000 + RESTART_EVERY_MS - 1, 1).is_empty(),
            "a restart was asked for inside the rate limit"
        );
        assert_eq!(leader.restart(10_000 + RESTART_EVERY_MS, 1).len(), 1);
    }

    /// A connection the browser reports as disconnected is asked to restart
    /// first: that repair keeps the channels, and the stale timer is what a
    /// restart that does not take falls through to.
    #[test]
    fn a_disconnected_connection_is_asked_to_restart_first() {
        let mut machine = opened(0, 1);
        let actions = machine.on(
            Event::Pc {
                seat: 1,
                state: PcState::Disconnected,
            },
            10_000,
        );
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, Action::Negotiate { restart: true, .. })),
            "a disconnected link was left to the stale timer"
        );

        // The polite end cannot ask, so it waits out the timer instead.
        let mut polite = opened(1, 0);
        assert!(
            !polite
                .on(
                    Event::Pc {
                        seat: 0,
                        state: PcState::Disconnected,
                    },
                    10_000,
                )
                .iter()
                .any(|action| matches!(action, Action::Negotiate { .. }))
        );
    }

    /// The seam is two encodings written in two languages. A tag that moves on one
    /// side and not the other is not a compile error: the browser decodes the
    /// next action as a different one, or refuses the frame and the link never
    /// starts at all. This pins every tag on both sides.
    #[test]
    fn the_browser_half_uses_the_seams_tags() {
        let js = crate::PEER_JS;
        for (name, value) in [
            ("ME_CANDIDATE", EVENT_CANDIDATE),
            ("ME_PC", EVENT_PC),
            ("ME_CHANNEL_OPEN", EVENT_CHANNEL_OPEN),
            ("ME_CHANNEL_CLOSED", EVENT_CHANNEL_CLOSED),
            ("ME_LOCAL_OFFER", EVENT_LOCAL_OFFER),
            ("ME_LOCAL_ANSWER", EVENT_LOCAL_ANSWER),
            ("ME_REMOTE_SET", EVENT_REMOTE_SET),
            ("ME_ROLLED_BACK", EVENT_ROLLED_BACK),
            ("ME_SDP_ERROR", EVENT_SDP_ERROR),
            ("ME_STATS", EVENT_STATS),
            ("ME_RESTART", EVENT_RESTART),
            ("MA_NEGOTIATE", ACTION_NEGOTIATE),
            ("MA_ANSWER", ACTION_ANSWER),
            ("MA_ACCEPT", ACTION_ACCEPT),
            ("MA_ADD_CANDIDATE", ACTION_ADD_CANDIDATE),
            ("MA_ROLLBACK", ACTION_ROLLBACK),
            ("MA_CLOSE", ACTION_CLOSE),
            ("MA_NOTE", ACTION_NOTE),
            ("MA_PATH", ACTION_PATH),
            ("MA_STATS", ACTION_STATS),
        ] {
            assert!(
                js.contains(&format!("var {name} = {value};")),
                "`{name}` is {value} here and something else in the shim"
            );
        }
        assert!(
            js.contains(&format!("var MAX_PAIRS = {MAX_PAIRS};")),
            "the shim's row cap and the decoder's disagree"
        );
    }

    /// The two things the seam was extended for: the shim reports the whole
    /// candidate-pair list and the restart ask, and assigns what comes back. A
    /// shim that still ranked the pairs itself would be right until it disagreed.
    #[test]
    fn the_shim_reports_the_path_and_the_restart_ask() {
        let js = crate::PEER_JS;
        assert!(
            js.contains("machineStats(link.seat, report)"),
            "a machine build still reduces the report itself"
        );
        assert!(
            js.contains("window.__btSampleStats = function"),
            "the machine has no way to ask for a path sample"
        );
        assert!(
            js.contains("machineEmit(ME_RESTART, seat, null)"),
            "a restart request never reaches the machine"
        );
        assert!(
            js.contains("function machinePath("),
            "the path the machine decided is never assigned to the link"
        );
    }

    /// The counters are published from one place, so a counter that moves cannot
    /// be a counter the page never hears about — and one that does not move is
    /// not worth a frame's action.
    #[test]
    fn the_counters_are_published_when_they_move_and_only_then() {
        let mut machine = machine(0);
        machine.on(group(1, &[0, 1]), 10);

        let offered = machine.on(
            Event::LocalOffer {
                seat: 1,
                sdp: b"offer".to_vec(),
            },
            20,
        );
        assert_eq!(
            offered.last(),
            Some(&Action::Stats(Stats {
                offered: 1,
                peak: 1,
                ..Stats::default()
            })),
            "the offer was not published: {offered:?}"
        );

        // A frame that decides nothing publishes nothing.
        let idle = machine.on(Event::Tick, 21);
        assert!(
            idle.iter().all(|action| !matches!(action, Action::Stats(_))),
            "an unchanged set of counters was re-published"
        );
    }

    #[test]
    fn a_stats_report_decodes_with_its_rows() {
        let mut writer = crate::Writer::default();
        writer.u8(EVENT_STATS);
        writer.u16(2);
        writer.u16(2);
        writer.u8(PAIR_SELECTED | PAIR_RTT);
        writer.u8(PairState::Succeeded.code());
        writer.u64(4_096);
        writer.f64(0.0123);
        writer.u8(CandidateType::Host.code());
        writer.u8(CandidateType::Srflx.code());
        writer.u8(0);
        writer.u8(PairState::Failed.code());
        writer.u64(0);
        writer.f64(0.0);
        writer.u8(CandidateType::Relay.code());
        writer.u8(CandidateType::Relay.code());

        let events = decode_events(&writer.into_bytes()).expect("a well-formed report");
        let [Event::StatsReport { seat, pairs }] = events.as_slice() else {
            panic!("not a stats report: {events:?}");
        };
        assert_eq!(*seat, 2);
        assert_eq!(
            pairs.as_slice(),
            &[
                pair(
                    PairState::Succeeded,
                    true,
                    false,
                    4_096,
                    Some(0.0123),
                    CandidateType::Host,
                    CandidateType::Srflx,
                ),
                pair(
                    PairState::Failed,
                    false,
                    false,
                    0,
                    None,
                    CandidateType::Relay,
                    CandidateType::Relay,
                ),
            ]
        );

        // The row count is bounded before it is trusted.
        let mut over = crate::Writer::default();
        over.u8(EVENT_STATS);
        over.u16(2);
        over.u16(MAX_PAIRS as u16 + 1);
        assert!(decode_events(&over.into_bytes()).is_err());
    }
}
