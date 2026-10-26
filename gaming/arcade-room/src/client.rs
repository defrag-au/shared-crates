//! `Client` — one player's view of a room: the confirmed world, the inputs
//! this player has sent ahead of it, and two smooth timelines to draw from.
//!
//! The edge (a macroquad game, or the [`testkit`](crate::testkit)) sends what
//! [`Client::drain`] returns and feeds back what arrives, with the time.
//!
//! # Two timelines
//!
//! Confirmed changes arrive in batches, every 100 ms, a one-way trip late.
//! Drawing the confirmed world directly would move everyone in jumps; guessing
//! ahead of it and correcting would make other players snap. So a game draws
//! from two timelines, each on a display clock that only moves forward and
//! glides (never more than 10% fast or slow) towards where it should be:
//!
//! - **Ahead** ([`Client::predicted`]): the confirmed world stepped forward to
//!   where this player's inputs land, applying them as they are pressed, so a
//!   press shows on the next frame. Draw *this player* from it.
//! - **Behind** ([`Client::behind`]): the confirmed stream played back one
//!   tick at a time, a little behind where batches arrive, so it never runs
//!   out. It is exact, so draw *everyone else* from it: they move smoothly and
//!   never snap, a fraction of a second in the past.
//!
//! An input is scheduled for the tick after the ahead clock's, which is far
//! enough ahead to reach the room in time: the ahead clock runs half a round
//! trip, plus a margin, in front of the room's.

use std::collections::{BTreeMap, VecDeque};

use crate::sim::{Change, RoomSim, Schema, Seat};
use crate::wire::{ToClient, ToRoom};

/// Ms between heartbeats when no input has been sent. Every client message
/// polls the room's clock, so this sets the room's pace when nobody presses
/// anything.
pub const HEARTBEAT_MS: u64 = 100;
/// Ms between round-trip measurements.
pub const PING_MS: u64 = 2_000;
/// The furthest a prediction steps past the confirmed world.
const MAX_PREDICT: u64 = 120;
/// Ticks a scheduled input is given beyond the one-way trip.
const MARGIN_TICKS: f64 = 2.0;
/// How far behind the room's clock, beyond the one-way trip, the behind
/// timeline plays: a batch (six ticks) plus room for jitter.
const BEHIND_TICKS: f64 = 9.0;
/// A display clock further than this from its target snaps forward to it
/// rather than gliding: a resume, a resync, the first frames.
const SNAP_TICKS: f64 = 30.0;
/// Within this many ticks of its target a display clock runs at exactly real
/// time. Chasing every small change in the estimate made the player's own
/// walking speed wobble, which reads as jank.
const DEAD_BAND: f64 = 2.0;
/// How far a display clock may run fast or slow while it glides back into
/// the dead band.
const GLIDE: f64 = 0.05;
/// The behind timeline is rebuilt from the confirmed world when it trails it
/// by more than this, rather than playing the gap back.
const MAX_BEHIND: u64 = 300;

/// A display clock: ticks, eased towards a target, never backwards.
#[derive(Clone, Copy, Debug)]
struct Glide {
    tick: f64,
    at_ms: u64,
}

impl Glide {
    fn at(tick: f64, now: u64) -> Self {
        Self { tick, at_ms: now }
    }

    fn advance(&mut self, now: u64, target: f64, hz: f64) {
        let nominal = now.saturating_sub(self.at_ms) as f64 * hz / 1_000.0;
        self.at_ms = now;
        if target - self.tick > SNAP_TICKS {
            self.tick = target;
            return;
        }
        let error = target - (self.tick + nominal);
        let adjust = if error.abs() <= DEAD_BAND {
            0.0
        } else {
            (error * 0.1).clamp(-GLIDE * nominal, GLIDE * nominal)
        };
        self.tick += nominal + adjust;
    }

    /// How far into the current tick the clock is, 0 to 1.
    fn frac(&self) -> f32 {
        self.tick.fract() as f32
    }
}

/// The opening message on a connection, as a guest.
pub fn hello<S: RoomSim>(player: &str, real: bool) -> Vec<u8> {
    hello_as::<S>(player, "", real)
}

/// The opening message on a connection, presenting `session` (a login
/// token the room's host verifies; empty for a guest).
pub fn hello_as<S: RoomSim>(player: &str, session: &str, real: bool) -> Vec<u8> {
    ToRoom::Hello {
        game: S::GAME.to_string(),
        version: S::VERSION,
        player: player.to_string(),
        real,
        session: session.to_string(),
    }
    .encode()
}

pub struct Client<S: RoomSim> {
    seat: Seat,
    /// The peer-plane format the room was built with, from the welcome. A game
    /// compares it with its own before it lets a peer's bytes near a decoder.
    schema: Schema,
    confirmed: S,
    /// This seat's inputs not yet confirmed, by the tick they were sent for.
    own: BTreeMap<u64, Vec<u8>>,
    last_input: Vec<u8>,
    last_target: u64,
    /// The room's clock as estimated here: the tick at `anchor_ms`.
    anchor_ms: u64,
    anchor_tick: f64,
    /// The round trip, smoothed, in ms.
    rtt_ms: f64,
    /// How much the round trip varies, smoothed, in ms: added to the input
    /// lead so a jittery link's inputs still arrive in time.
    jitter_ms: f64,
    hash_every: u64,
    last_sent_ms: u64,
    last_ping_ms: u64,
    out: Vec<Vec<u8>>,
    /// The display clocks of the two timelines.
    ahead: Glide,
    behind: Glide,
    /// The behind timeline's world, and the confirmed changes it has still to
    /// play: one list per tick from `playback.tick()` to `confirmed.tick()`.
    playback: S,
    to_play: VecDeque<Vec<Change>>,
    /// Why the room turned this seated client away, once it has: the same
    /// player joined from somewhere else.
    refused: Option<String>,
    /// Signalling from other players, in arrival order: the seat the room
    /// vouched for, and the `arcade-peer` bytes beside it.
    signals: VecDeque<(Seat, Vec<u8>)>,
    /// Who the room says this player should keep a peer link with, and the
    /// epoch that names the set: `None` until the room has said.
    group: Option<(u32, Vec<Seat>)>,
    /// Frames sent the room since this client was seated. For a readout, and
    /// for measuring what a full room would cost: 64 players at one frame per
    /// input change is the number that decides whether the rate split is
    /// mandatory. Never read for a rule.
    sent: u32,
    /// Signalling frames sent and received since this client was seated. The
    /// peer plane shares the same socket as the game, so this is the only
    /// count that says how much of the room's traffic it is. For a readout and
    /// for the 64-seat measurement; never read for a rule.
    signals_sent: u32,
    signals_received: u32,
}

impl<S: RoomSim> Client<S> {
    /// Start from the room's first reply: a client on `Welcome`, the reason
    /// on `Refused`.
    pub fn welcome(bytes: &[u8], now: u64, hash_every: u64) -> Result<Self, String> {
        match ToClient::decode(bytes).map_err(|e| e.to_string())? {
            ToClient::Welcome {
                seat,
                room_tick,
                schema,
                snapshot,
                ..
            } => {
                let confirmed = S::restore(&snapshot)?;
                let start = confirmed.tick() as f64;
                let mut client = Self {
                    seat,
                    schema,
                    playback: confirmed.clone(),
                    confirmed,
                    own: BTreeMap::new(),
                    last_input: vec![0; S::INPUT_BYTES],
                    last_target: 0,
                    anchor_ms: now,
                    anchor_tick: room_tick as f64,
                    rtt_ms: 100.0,
                    jitter_ms: 20.0,
                    hash_every: hash_every.max(1),
                    last_sent_ms: now,
                    last_ping_ms: 0,
                    out: Vec::new(),
                    ahead: Glide::at(room_tick as f64, now),
                    behind: Glide::at(start, now),
                    to_play: VecDeque::new(),
                    refused: None,
                    signals: VecDeque::new(),
                    group: None,
                    sent: 0,
                    signals_sent: 0,
                    signals_received: 0,
                };
                client.ahead = Glide::at(room_tick as f64 + client.lead(), now);
                client.ping(now);
                Ok(client)
            }
            ToClient::Refused { reason } => Err(reason),
            other => Err(format!("expected a welcome, got {other:?}")),
        }
    }

    pub fn seat(&self) -> Seat {
        self.seat
    }

    /// The peer-plane format the room was built with, as it published it in
    /// this client's welcome ([`Schema`]).
    pub fn schema(&self) -> Schema {
        self.schema
    }

    /// Why the room turned this client away after seating it, if it has:
    /// its player joined the world from somewhere else, and the room closes
    /// this connection. Stop, rather than reconnect and take the seat back.
    pub fn refused(&self) -> Option<&str> {
        self.refused.as_deref()
    }

    /// The world as confirmed by the room.
    pub fn confirmed(&self) -> &S {
        &self.confirmed
    }

    /// The confirmed world, mutably: for a game to take what its sim keeps
    /// outside the state (the events its steps produced, for sound). Never
    /// change what the hash covers through this, or the room will resync.
    pub fn confirmed_mut(&mut self) -> &mut S {
        &mut self.confirmed
    }

    /// The round trip, smoothed, in ms.
    pub fn rtt_ms(&self) -> u64 {
        self.rtt_ms.round() as u64
    }

    /// Frames this client has sent the room since it was seated, of every
    /// kind: inputs, pings, heartbeats and signals.
    pub fn sent(&self) -> u32 {
        self.sent
    }

    /// The room's clock, estimated.
    pub fn room_tick(&self, now: u64) -> f64 {
        self.anchor_tick
            + now.saturating_sub(self.anchor_ms) as f64 * f64::from(S::TICK_HZ) / 1_000.0
    }

    fn one_way(&self) -> f64 {
        self.rtt_ms / 2.0 * f64::from(S::TICK_HZ) / 1_000.0
    }

    /// How far ahead of the room's clock this player's timeline runs, in
    /// ticks: the one-way trip, the link's jitter, and a margin.
    fn lead(&self) -> f64 {
        let jitter = self.jitter_ms * f64::from(S::TICK_HZ) / 1_000.0;
        self.one_way() + jitter + MARGIN_TICKS
    }

    /// How far ahead of the room's clock an input is scheduled, in whole
    /// ticks.
    pub fn lead_ticks(&self) -> u64 {
        self.lead().ceil() as u64
    }

    /// How far behind the present the confirmed stream is drawn, in
    /// milliseconds: the one-way trip plus the margin the behind timeline
    /// plays at.
    ///
    /// The overlay's sample age is the same kind of quantity, so the difference
    /// between the two is what the overlay buys **in time** — no moving sprite,
    /// no world geometry, no rounding to tiles. On a loopback room this is the
    /// playback margin alone; with the room a continent away it grows by the
    /// trip.
    pub fn behind_ms(&self) -> u32 {
        let ms = (self.one_way() + BEHIND_TICKS) * 1_000.0 / f64::from(S::TICK_HZ);
        ms.round().max(0.0) as u32
    }

    /// Move both display clocks to `now`, and play the behind timeline up to
    /// its clock. [`poll`](Self::poll) and [`input`](Self::input) do this
    /// too; call it before drawing.
    pub fn display(&mut self, now: u64) {
        let hz = f64::from(S::TICK_HZ);
        let room = self.room_tick(now);
        self.ahead.advance(now, room + self.lead(), hz);
        self.behind
            .advance(now, room - self.one_way() - BEHIND_TICKS, hz);
        let confirmed = self.confirmed.tick();
        if confirmed.saturating_sub(self.playback.tick()) > MAX_BEHIND {
            self.playback = self.confirmed.clone();
            self.to_play.clear();
            self.behind = Glide::at(confirmed as f64, now);
        }
        self.behind.tick = self.behind.tick.min(confirmed as f64);
        let until = self.behind.tick.floor() as u64;
        while self.playback.tick() < until {
            let Some(changes) = self.to_play.pop_front() else {
                break;
            };
            self.playback.step(&changes);
        }
    }

    /// The behind timeline: the confirmed world played back smoothly, a
    /// little in the past. Draw everyone but this player from it.
    pub fn behind(&self) -> &S {
        &self.playback
    }

    /// This player's controls now. Sent only when they change.
    pub fn input(&mut self, bytes: &[u8], now: u64) {
        self.display(now);
        if bytes.len() != S::INPUT_BYTES || bytes == self.last_input.as_slice() {
            return;
        }
        // The tick after the one this player is looking at: the press shows
        // on the next frame, and the ahead clock keeps it far enough ahead to
        // reach the room in time.
        let target = (self.ahead.tick.floor() as u64 + 1)
            .max(self.last_target)
            .max(self.confirmed.tick());
        self.last_target = target;
        self.last_input = bytes.to_vec();
        self.own.insert(target, bytes.to_vec());
        self.send(ToRoom::Inputs(vec![(target, bytes.to_vec())]), now);
    }

    /// Heartbeats and pings, as they fall due, and the display clocks. Call
    /// once a frame.
    pub fn poll(&mut self, now: u64) {
        self.display(now);
        if now >= self.last_ping_ms + PING_MS {
            self.ping(now);
        } else if now >= self.last_sent_ms + HEARTBEAT_MS {
            self.send(ToRoom::Inputs(Vec::new()), now);
        }
    }

    fn ping(&mut self, now: u64) {
        self.last_ping_ms = now;
        self.send(ToRoom::Ping { client_ms: now }, now);
    }

    fn send(&mut self, msg: ToRoom, now: u64) {
        self.last_sent_ms = now;
        self.sent = self.sent.saturating_add(1);
        self.out.push(msg.encode());
    }

    /// Send a signalling message to one other player.
    ///
    /// The room stamps this seat as the sender and routes the body to `to`
    /// alone, provided it considers the pair reachable — one of the two sets
    /// names the other. The bytes carry no identity of their own and a peer
    /// cannot claim another's seat by writing it down.
    pub fn send_signal(&mut self, to: Seat, bytes: Vec<u8>, now: u64) {
        self.signals_sent = self.signals_sent.saturating_add(1);
        self.send(ToRoom::SignalTo { to, bytes }, now);
    }

    /// Send a signalling message to everyone who can reach this seat.
    ///
    /// Only discovery uses this: an announce is how a peer learns who else is
    /// here, so it has nobody to name yet. Everything after it is targeted.
    pub fn send_announce(&mut self, bytes: Vec<u8>, now: u64) {
        self.signals_sent = self.signals_sent.saturating_add(1);
        self.send(ToRoom::Signal(bytes), now);
    }

    /// Signalling frames this client has sent and received. A readout number
    /// and a measurement, not an input to anything.
    pub fn signals(&self) -> (u32, u32) {
        (self.signals_sent, self.signals_received)
    }

    /// The next signalling message from another player, with the seat the room
    /// vouched for. Nothing here interprets the bytes: that is `arcade-peer`'s.
    pub fn take_signal(&mut self) -> Option<(Seat, Vec<u8>)> {
        self.signals.pop_front()
    }

    /// Who the room says this player should keep a peer link with, nearest
    /// first, and the epoch that names that set: `None` until the room has
    /// said, which is why the peer plane waits for it rather than assuming a
    /// mesh.
    pub fn group(&self) -> Option<(u32, &[Seat])> {
        self.group
            .as_ref()
            .map(|(epoch, members)| (*epoch, members.as_slice()))
    }

    /// A frame from the room. Errors only for a frame that does not decode.
    pub fn receive(&mut self, bytes: &[u8], now: u64) -> Result<(), String> {
        match ToClient::decode(bytes).map_err(|e| e.to_string())? {
            ToClient::Signal { from, bytes } => {
                self.signals_received = self.signals_received.saturating_add(1);
                self.signals.push_back((from, bytes));
            }
            ToClient::Confirmed {
                first,
                room_tick,
                ticks,
            } => {
                for (i, changes) in ticks.iter().enumerate() {
                    let t = first + i as u64;
                    if t != self.confirmed.tick() {
                        // Already applied (a resync overlapped), or a gap,
                        // which a TCP stream never produces.
                        continue;
                    }
                    self.confirmed.step(changes);
                    self.to_play.push_back(changes.clone());
                    let after = self.confirmed.tick();
                    if after.is_multiple_of(self.hash_every) {
                        let hash = self.confirmed.hash();
                        self.send(ToRoom::Hash { tick: after, hash }, now);
                    }
                }
                let done = self.confirmed.tick();
                self.own.retain(|t, _| *t >= done);
                self.observe(room_tick, now);
            }
            ToClient::Pong {
                client_ms,
                room_tick,
            } => {
                // Smoothed, so one slow ping does not swing the lead; the
                // jitter rises at once and falls slowly, so the lead stays
                // big enough for the worst of a bad patch.
                let sample = now.saturating_sub(client_ms).max(1) as f64;
                let spread = (sample - self.rtt_ms).abs();
                self.jitter_ms = if spread > self.jitter_ms {
                    spread
                } else {
                    self.jitter_ms + (spread - self.jitter_ms) * 0.1
                };
                self.rtt_ms += (sample - self.rtt_ms) * 0.25;
                self.observe(room_tick, now);
            }
            ToClient::Resync { snapshot } => {
                self.confirmed = S::restore(&snapshot)?;
                self.playback = self.confirmed.clone();
                self.to_play.clear();
                self.behind = Glide::at(self.confirmed.tick() as f64, now);
                let done = self.confirmed.tick();
                self.own.retain(|t, _| *t >= done);
            }
            ToClient::Refused { reason } => self.refused = Some(reason),
            ToClient::Group { epoch, members } => self.group = Some((epoch, members)),
            ToClient::Welcome { .. } => {}
        }
        Ok(())
    }

    /// Fold in the room's clock as reported, half a round trip ago.
    fn observe(&mut self, room_tick: u64, now: u64) {
        let sample = room_tick as f64 + self.one_way();
        let ours = self.room_tick(now);
        let error = sample - ours;
        self.anchor_ms = now;
        // Snap on a big error (a pause, a resume, the first samples); drift
        // gently otherwise, so the prediction does not jitter.
        self.anchor_tick = if error.abs() > 6.0 {
            sample
        } else {
            ours + error * 0.1
        };
    }

    /// The ahead timeline: the confirmed world stepped forward through the
    /// tick this player's newest input lands on, at the ahead clock as of the
    /// last [`display`](Self::display). Draw this player from it.
    pub fn predicted(&self) -> S {
        self.predicted_pair().1
    }

    /// The ahead timeline as two worlds a tick apart, and how far between
    /// them the ahead clock is (0 to 1): draw this player interpolated
    /// between the two, and they move every frame, however a frame and a
    /// tick line up.
    pub fn predicted_pair(&self) -> (S, S, f32) {
        let mut world = self.confirmed.clone();
        let until = (self.ahead.tick.floor() as u64 + 2).min(world.tick() + MAX_PREDICT);
        let step = |world: &mut S| {
            let changes = match self.own.get(&world.tick()) {
                Some(bytes) => vec![Change::Input(self.seat, bytes.clone())],
                None => Vec::new(),
            };
            world.step(&changes);
        };
        while world.tick() + 1 < until {
            step(&mut world);
        }
        let earlier = world.clone();
        if world.tick() < until {
            step(&mut world);
        }
        (earlier, world, self.ahead.frac())
    }

    /// The behind timeline's next tick, when it has been confirmed, and how
    /// far towards it the behind clock is (0 to 1): draw everyone else
    /// interpolated between [`behind`](Self::behind) and this.
    pub fn behind_next(&self) -> (Option<S>, f32) {
        let next = self.to_play.front().map(|changes| {
            let mut world = self.playback.clone();
            world.step(changes);
            world
        });
        (next, self.behind.frac())
    }

    /// Frames to send, in order.
    pub fn drain(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.out)
    }
}
