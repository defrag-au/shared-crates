//! `Room` — one world: seats, the clock that paces it, and the confirmed
//! stream every client steps on.
//!
//! The edge (a Durable Object, or the [`testkit`](crate::testkit)) feeds it
//! connections, frames and the time, and sends what [`Room::drain`] returns.
//! The room runs the same sim as its clients, which gives it a snapshot to
//! hand each joiner, an authoritative hash to check clients against, and
//! state to save.
//!
//! # The clock
//!
//! World time is ticks run, not wall time. While at least one real player is
//! connected the clock runs at the sim's rate, and ticks are confirmed in
//! batches as they fall due. Nothing waits for any seat: an input that
//! arrives for a tick already confirmed lands on the next open one. With no
//! real player connected the clock stops, and the next arrival resumes it
//! from the tick it stopped at rather than catching up.
//!
//! The room needs no timer while anyone plays: every client sends at least a
//! heartbeat every 100 ms, and every frame received polls the clock.

use std::collections::{BTreeMap, VecDeque};

use crate::sim::{Change, INTEREST_MAX, RoomSim, Seat};
use crate::wire::{ToClient, ToRoom};

/// A connection, numbered by the edge.
pub type ConnId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoomConfig {
    /// Ticks confirmed per broadcast: six at 60 Hz is a message every 100 ms.
    pub batch_ticks: u64,
    /// Ticks between the hashes clients send and the room checks.
    pub hash_every: u64,
    /// How long a dropped player keeps their seat, in ms.
    pub grace_ms: u64,
    /// The most ticks one poll confirms. After a longer stall the world skips
    /// ahead rather than replaying it at once.
    pub max_catch_up: u64,
}

impl Default for RoomConfig {
    fn default() -> Self {
        Self {
            batch_ticks: 6,
            hash_every: 60,
            grace_ms: 30_000,
            max_catch_up: 300,
        }
    }
}

/// Who a message is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum To {
    One(ConnId),
    Many(Vec<ConnId>),
}

/// A message for the edge to send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Out {
    pub to: To,
    pub bytes: Vec<u8>,
    /// Close the connection after sending.
    pub close: bool,
}

#[derive(Clone, Debug)]
struct Conn {
    seat: Option<Seat>,
    real: bool,
}

#[derive(Clone, Debug)]
struct SeatInfo {
    player: String,
    real: bool,
    conn: Option<ConnId>,
    /// When the connection dropped, while it is gone.
    gone_since: Option<u64>,
    /// The last tick a late input of this seat was moved to, so two late
    /// inputs (a press and its release) land on separate ticks.
    last_late: u64,
}

#[derive(Clone, Copy, Debug)]
struct Clock {
    anchor_ms: u64,
    anchor_tick: u64,
}

pub struct Room<S: RoomSim> {
    sim: S,
    cfg: RoomConfig,
    conns: BTreeMap<ConnId, Conn>,
    seats: BTreeMap<Seat, SeatInfo>,
    /// Changes for ticks not yet confirmed.
    pending: BTreeMap<u64, Vec<Change>>,
    clock: Option<Clock>,
    /// The room's own hashes, every `hash_every` ticks, newest last.
    hashes: VecDeque<(u64, u64)>,
    out: Vec<Out>,
    stats: RoomStats,
    /// Verifies a hello's session token: see [`Room::identify_with`].
    identify: Option<Identify>,
    /// Seats newly taken since the host last asked ([`Room::take_joined`]).
    joined: Vec<(Seat, String)>,
    /// Each seat's interest set as last published, and the epoch that names it.
    /// A set that has not changed is not re-sent.
    groups: BTreeMap<Seat, Vec<Seat>>,
    group_epoch: u32,
    /// The tick the groups were last recomputed at.
    grouped_at: u64,
    /// Signalling frames from seats, and the copies each produced. A payload
    /// shares the game's socket with its inputs, so these are the peer plane's
    /// whole cost to the object: one frame in, one per seat in the sender's
    /// group out. Counted for the 64-seat measurement (§9); never read for a
    /// rule.
    signals_in: u32,
    signal_copies: u32,
}

/// Verifies a session token from a hello, returning who it names (a stable
/// id, such as `discord:<user id>`), or `None` when it does not verify.
pub type Identify = Box<dyn Fn(&str) -> Option<String>>;

/// What the room has seen, for logs and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoomStats {
    /// Client hashes compared with the room's.
    pub hashes_checked: u64,
    /// Clients resynced because their hash disagreed.
    pub resyncs: u64,
    /// Inputs that arrived after their tick and were moved later.
    pub late_inputs: u64,
    /// Ticks confirmed.
    pub ticks: u64,
}

const SAVE_MAGIC: &[u8] = b"ARM1";

/// How often a seat's group is recomputed, in seconds.
///
/// Coarse on purpose. A membership change costs a `Bye` and a re-announce at
/// both ends, so recomputing it at tick rate would churn more than it buys;
/// players cross an area-of-interest boundary over a second or two of travel,
/// not over a tick.
const GROUP_EVERY_SECS: u64 = 1;

impl<S: RoomSim> Room<S> {
    pub fn new(seed: u64, cfg: RoomConfig) -> Self {
        Self::around(S::new(seed), cfg)
    }

    fn around(sim: S, cfg: RoomConfig) -> Self {
        Self {
            sim,
            cfg,
            conns: BTreeMap::new(),
            seats: BTreeMap::new(),
            pending: BTreeMap::new(),
            clock: None,
            hashes: VecDeque::new(),
            out: Vec::new(),
            stats: RoomStats::default(),
            identify: None,
            joined: Vec::new(),
            groups: BTreeMap::new(),
            group_epoch: 0,
            grouped_at: 0,
            signals_in: 0,
            signal_copies: 0,
        }
    }

    /// Signalling frames received from seats, and the copies the room made of
    /// them. A readout and a measurement, not an input to anything.
    pub fn signals(&self) -> (u32, u32) {
        (self.signals_in, self.signal_copies)
    }

    /// Seats taken by a new arrival since the last call, with the key each
    /// was seated under: for a host that loads what it keeps about a player
    /// (see [`profile`](Self::profile)). A player taking their own seat back
    /// within its grace is not new, and is not listed.
    pub fn take_joined(&mut self) -> Vec<(Seat, String)> {
        std::mem::take(&mut self.joined)
    }

    /// Every held seat and the key it is held under.
    pub fn players(&self) -> impl Iterator<Item = (Seat, &str)> {
        self.seats
            .iter()
            .map(|(seat, s)| (*seat, s.player.as_str()))
    }

    /// Tell the world something the host knows from outside it (a server's
    /// progress towards its goals) as a [`Change::World`] at the next tick.
    pub fn world_fact(&mut self, bytes: Vec<u8>) {
        let at = self.sim.tick();
        self.pending
            .entry(at)
            .or_default()
            .push(Change::World(bytes));
    }

    /// Tell the world what the host knows about `player` (their saved
    /// progress, in the game's encoding) as a [`Change::Profile`] on their
    /// seat, at the next tick. `false` if they hold no seat now: they left
    /// while it was being loaded.
    pub fn profile(&mut self, player: &str, bytes: Vec<u8>) -> bool {
        let Some(seat) = self
            .seats
            .iter()
            .find(|(_, s)| s.player == player)
            .map(|(seat, _)| *seat)
        else {
            return false;
        };
        let at = self.sim.tick();
        self.pending
            .entry(at)
            .or_default()
            .push(Change::Profile(seat, bytes));
        true
    }

    /// Key seats by verified identity. Without this, a seat belongs to
    /// whatever name the client says, which is fine for a room nobody
    /// persists anything from. With it:
    ///
    /// - a hello whose session `verify` accepts is seated as `id:<who>`, so a
    ///   logged-in player gets their seat back however they reconnect;
    /// - any other hello is a guest, `guest:<name>`, which can never collide
    ///   with a verified player's key, so a guest cannot take their seat by
    ///   claiming their name.
    ///
    /// A token that fails to verify (expired, or signed with another key)
    /// plays as a guest rather than being refused: the game stays open.
    pub fn identify_with(&mut self, verify: Identify) {
        self.identify = Some(verify);
    }

    /// Who holds `seat`: the key it was seated under (see
    /// [`identify_with`](Self::identify_with)).
    pub fn player(&self, seat: Seat) -> Option<&str> {
        self.seats.get(&seat).map(|s| s.player.as_str())
    }

    pub fn stats(&self) -> RoomStats {
        self.stats
    }

    /// The world as confirmed so far.
    pub fn sim(&self) -> &S {
        &self.sim
    }

    /// Whether the world is running: at least one real player is connected.
    pub fn running(&self) -> bool {
        self.clock.is_some()
    }

    /// Connections that are real players.
    pub fn real_connected(&self) -> usize {
        self.conns
            .values()
            .filter(|c| c.real && c.seat.is_some())
            .count()
    }

    /// Seats held, connected or within their grace.
    pub fn seated(&self) -> usize {
        self.seats.len()
    }

    /// Where the room's clock is now: the tick the world has reached in time,
    /// confirmed or not. The confirmed tick while paused.
    pub fn room_tick(&self, now: u64) -> u64 {
        match self.clock {
            Some(c) => {
                c.anchor_tick + now.saturating_sub(c.anchor_ms) * u64::from(S::TICK_HZ) / 1_000
            }
            None => self.sim.tick(),
        }
    }

    /// A new connection.
    pub fn open(&mut self, conn: ConnId) {
        self.conns.insert(
            conn,
            Conn {
                seat: None,
                real: false,
            },
        );
    }

    /// A frame from `conn`.
    pub fn message(&mut self, conn: ConnId, bytes: &[u8], now: u64) {
        let Some(c) = self.conns.get(&conn).cloned() else {
            return;
        };
        let Ok(msg) = ToRoom::decode(bytes) else {
            return;
        };
        match (msg, c.seat) {
            (
                ToRoom::Hello {
                    game,
                    version,
                    player,
                    real,
                    session,
                },
                None,
            ) => {
                let player = self.key(player, &session);
                self.hello(conn, &game, version, player, real, now)
            }
            (ToRoom::Inputs(changes), Some(seat)) => {
                for (tick, bytes) in changes {
                    self.input(seat, tick, bytes, now);
                }
            }
            (ToRoom::Hash { tick, hash }, Some(_)) => {
                let ours = self
                    .hashes
                    .iter()
                    .find(|(t, _)| *t == tick)
                    .map(|(_, h)| *h);
                if ours.is_some() {
                    self.stats.hashes_checked += 1;
                }
                if ours.is_some_and(|h| h != hash) {
                    self.stats.resyncs += 1;
                    self.send(
                        conn,
                        ToClient::Resync {
                            snapshot: self.sim.snapshot(),
                        },
                    );
                }
            }
            (ToRoom::Ping { client_ms }, _) => {
                let room_tick = self.room_tick(now);
                self.send(
                    conn,
                    ToClient::Pong {
                        client_ms,
                        room_tick,
                    },
                );
            }
            // Signalling: the room vouches for who sent it and puts it in front
            // of the sender's group. It never reads the body — that is
            // `arcade-peer`'s — and it does not echo to the sender.
            //
            // Scoped rather than broadcast: with groups, an offer never leaves
            // the pair of interest sets it belongs to, so a 64-player world
            // pays for the peers an eye can see rather than for every seat.
            (ToRoom::Signal(bytes), Some(seat)) => {
                self.signals_in = self.signals_in.saturating_add(1);
                for target in self.signal_targets(seat) {
                    self.signal_copies = self.signal_copies.saturating_add(1);
                    self.send(
                        target,
                        ToClient::Signal {
                            from: seat,
                            bytes: bytes.clone(),
                        },
                    );
                }
            }
            // Anything else out of order is ignored: inputs before a hello,
            // a second hello.
            _ => {}
        }
        self.poll(now);
    }

    /// `conn` closed.
    pub fn close(&mut self, conn: ConnId, now: u64) {
        if let Some(Conn {
            seat: Some(seat), ..
        }) = self.conns.remove(&conn)
            && let Some(s) = self.seats.get_mut(&seat)
            && s.conn == Some(conn)
        {
            s.conn = None;
            s.gone_since = Some(now);
            // Inputs are held until changed, and nobody will change this
            // seat's now. Idle it, so a player who dropped mid-stride stands
            // still through their grace instead of walking on as a ghost (off
            // an edge, over and over).
            let at = self.sim.tick();
            let changes = self.pending.entry(at).or_default();
            changes.retain(|c| !matches!(c, Change::Input(s, _) if *s == seat));
            changes.push(Change::Input(seat, vec![0; S::INPUT_BYTES]));
        }
        self.poll(now);
    }

    /// The seat key for a hello: see [`identify_with`](Self::identify_with).
    fn key(&self, player: String, session: &str) -> String {
        let Some(verify) = &self.identify else {
            return player;
        };
        match (!session.is_empty()).then(|| verify(session)).flatten() {
            Some(who) => format!("id:{who}"),
            None => format!("guest:{player}"),
        }
    }

    fn hello(
        &mut self,
        conn: ConnId,
        game: &str,
        version: u32,
        player: String,
        real: bool,
        now: u64,
    ) {
        if game != S::GAME || version != S::VERSION {
            return self.refuse(
                conn,
                format!(
                    "this world runs {} v{}; reload for the current version",
                    S::GAME,
                    S::VERSION
                ),
            );
        }
        // Bring the world up to now *before* seating this connection: a
        // batch confirmed here goes to those already seated, and must not
        // reach this one ahead of its welcome. Its stream starts at the
        // snapshot below.
        self.poll(now);
        // The same player back within their grace gets their seat back. A
        // verified player gets it back even while it is connected (a second
        // tab, a new phone): one identity, one seat, and the newest
        // connection wins.
        let verified = self.identify.is_some() && player.starts_with("id:");
        let returning = self
            .seats
            .iter()
            .find(|(_, s)| s.player == player && (s.conn.is_none() || verified))
            .map(|(seat, s)| (*seat, s.conn));
        if let Some((_, Some(old))) = returning {
            self.refuse(old, "you joined this world somewhere else".to_string());
        }
        let seat = match returning.map(|(seat, _)| seat) {
            Some(seat) => seat,
            None => {
                let Some(free) = (0..S::MAX_SEATS as Seat).find(|s| !self.seats.contains_key(s))
                else {
                    return self.refuse(conn, "this world is full".to_string());
                };
                self.joined.push((free, player.clone()));
                self.seats.insert(
                    free,
                    SeatInfo {
                        player,
                        real,
                        conn: None,
                        gone_since: None,
                        last_late: 0,
                    },
                );
                let at = self.sim.tick();
                self.pending.entry(at).or_default().push(Change::Join(free));
                free
            }
        };
        let s = self.seats.get_mut(&seat).expect("seated above");
        s.conn = Some(conn);
        s.gone_since = None;
        s.real = real;
        self.conns.insert(
            conn,
            Conn {
                seat: Some(seat),
                real,
            },
        );
        let welcome = ToClient::Welcome {
            seat,
            tick_hz: S::TICK_HZ,
            room_tick: self.room_tick(now),
            snapshot: self.sim.snapshot(),
        };
        self.send(conn, welcome);
    }

    fn refuse(&mut self, conn: ConnId, reason: String) {
        self.out.push(Out {
            to: To::One(conn),
            bytes: ToClient::Refused { reason }.encode(),
            close: true,
        });
        self.conns.remove(&conn);
    }

    fn input(&mut self, seat: Seat, tick: u64, bytes: Vec<u8>, now: u64) {
        if bytes.len() != S::INPUT_BYTES {
            return;
        }
        let open = self.sim.tick();
        // Never further ahead than two seconds of the room's clock: a client
        // with a wild clock must not park inputs in the far future.
        let tick = tick.min(self.room_tick(now) + 2 * u64::from(S::TICK_HZ));
        let Some(s) = self.seats.get_mut(&seat) else {
            return;
        };
        let at = if tick < open {
            // Late: the tick it was meant for is already confirmed.
            let at = open.max(s.last_late + 1);
            s.last_late = at;
            self.stats.late_inputs += 1;
            at
        } else {
            tick
        };
        let changes = self.pending.entry(at).or_default();
        changes.retain(|c| !matches!(c, Change::Input(s, _) if *s == seat));
        changes.push(Change::Input(seat, bytes));
    }

    /// Advance the clock to `now`: drop seats past their grace, pause or
    /// resume, and confirm every tick that has fallen due, if a batch's worth
    /// has. Called by every other method; the edge also calls it on a timer
    /// as a backstop.
    pub fn poll(&mut self, now: u64) {
        self.regroup();
        let expired: Vec<Seat> = self
            .seats
            .iter()
            .filter(|(_, s)| s.gone_since.is_some_and(|t| now >= t + self.cfg.grace_ms))
            .map(|(seat, _)| *seat)
            .collect();
        for seat in expired {
            self.seats.remove(&seat);
            let at = self.sim.tick();
            self.pending
                .entry(at)
                .or_default()
                .push(Change::Leave(seat));
        }

        match (self.clock, self.real_connected() > 0) {
            (None, false) => return,
            (Some(_), false) => {
                self.clock = None;
                return;
            }
            (None, true) => {
                self.clock = Some(Clock {
                    anchor_ms: now,
                    anchor_tick: self.sim.tick(),
                });
            }
            (Some(_), true) => {}
        }

        let next = self.sim.tick();
        let mut due = self.room_tick(now);
        if due < next + self.cfg.batch_ticks {
            return;
        }
        if due - next > self.cfg.max_catch_up {
            // A long stall (the Durable Object was busy or asleep): skip the
            // world's clock ahead instead of replaying the gap in one go.
            due = next + self.cfg.max_catch_up;
            self.clock = Some(Clock {
                anchor_ms: now,
                anchor_tick: due,
            });
        }
        let mut ticks = Vec::with_capacity((due - next) as usize);
        while self.sim.tick() < due {
            let changes = self.pending.remove(&self.sim.tick()).unwrap_or_default();
            self.sim.step(&changes);
            let t = self.sim.tick();
            if t.is_multiple_of(self.cfg.hash_every) {
                self.hashes.push_back((t, self.sim.hash()));
                if self.hashes.len() > 64 {
                    self.hashes.pop_front();
                }
            }
            ticks.push(changes);
            self.stats.ticks += 1;
        }
        let to: Vec<ConnId> = self
            .conns
            .iter()
            .filter(|(_, c)| c.seat.is_some())
            .map(|(id, _)| *id)
            .collect();
        if !to.is_empty() {
            self.out.push(Out {
                to: To::Many(to),
                bytes: ToClient::Confirmed {
                    first: next,
                    room_tick: self.room_tick(now),
                    ticks,
                }
                .encode(),
                close: false,
            });
        }
    }

    fn send(&mut self, conn: ConnId, msg: ToClient) {
        self.out.push(Out {
            to: To::One(conn),
            bytes: msg.encode(),
            close: false,
        });
    }

    /// Recompute every seat's interest set, and tell the seats whose set
    /// changed.
    ///
    /// The room asks the sim, because the sim is the only thing that knows
    /// where anyone is; it intersects the answer with the seats it actually
    /// holds, so a stale answer cannot name a ghost. A sim with no opinion
    /// (`interest` returning `None`) gets the whole room, which is what every
    /// game had before groups existed.
    fn regroup(&mut self) {
        let every = u64::from(S::TICK_HZ).max(1) * GROUP_EVERY_SECS;
        let due = self.sim.tick().saturating_sub(self.grouped_at) >= every;
        if !due && self.groups.len() == self.seats.len() {
            return;
        }
        self.grouped_at = self.sim.tick();
        let held: Vec<Seat> = self.seats.keys().copied().collect();
        let mut next = BTreeMap::new();
        let mut changed = Vec::new();
        for seat in &held {
            let members: Vec<Seat> = match self.sim.interest(*seat, INTEREST_MAX) {
                Some(near) => near
                    .into_iter()
                    .filter(|s| s != seat && self.seats.contains_key(s))
                    .collect(),
                None => held.iter().copied().filter(|s| s != seat).collect(),
            };
            if self.groups.get(seat) != Some(&members) {
                changed.push((*seat, members.clone()));
            }
            next.insert(*seat, members);
        }
        self.groups = next;
        if changed.is_empty() {
            return;
        }
        self.group_epoch = self.group_epoch.wrapping_add(1);
        for (seat, members) in changed {
            let Some(conn) = self.seats.get(&seat).and_then(|s| s.conn) else {
                continue;
            };
            self.send(
                conn,
                ToClient::Group {
                    epoch: self.group_epoch,
                    members,
                },
            );
        }
    }

    /// The connections a signal from `seat` goes to: its own group, plus every
    /// seat that names `seat` in theirs.
    ///
    /// The union is not belt-and-braces. A distance set capped at
    /// [`INTEREST_MAX`] is not symmetric — A's twelve nearest can exclude B
    /// while B's include A — and a pair where only one end knows the other still
    /// has to negotiate: A offers because it sees B, and B's answer has to reach
    /// A.
    ///
    /// Before the first group has been computed (a signal racing a hello) this
    /// falls back to every other seated connection, which is what the room did
    /// before groups existed.
    fn signal_targets(&self, seat: Seat) -> Vec<ConnId> {
        let Some(members) = self.groups.get(&seat) else {
            return self
                .conns
                .iter()
                .filter(|(_, c)| c.seat.is_some() && c.seat != Some(seat))
                .map(|(id, _)| *id)
                .collect();
        };
        let mut reaches = members.clone();
        for (other, theirs) in &self.groups {
            if *other != seat && theirs.contains(&seat) && !reaches.contains(other) {
                reaches.push(*other);
            }
        }
        reaches
            .into_iter()
            .filter_map(|s| self.seats.get(&s).and_then(|i| i.conn))
            .collect()
    }

    /// Messages to send, in order.
    pub fn drain(&mut self) -> Vec<Out> {
        std::mem::take(&mut self.out)
    }

    /// The room as bytes, for storage: the world, and who held each seat, so
    /// players reconnecting after the room was evicted get their seats back.
    /// Unconfirmed inputs are not kept.
    pub fn save(&self) -> Vec<u8> {
        let mut out = SAVE_MAGIC.to_vec();
        out.extend_from_slice(&(self.seats.len() as u16).to_le_bytes());
        for (seat, s) in &self.seats {
            out.extend_from_slice(&seat.to_le_bytes());
            out.push(u8::from(s.real));
            let player = &s.player.as_bytes()[..s.player.len().min(255)];
            out.push(player.len() as u8);
            out.extend_from_slice(player);
        }
        out.extend_from_slice(&self.sim.snapshot());
        out
    }

    /// A room from [`save`](Self::save), at `now`. Everyone is disconnected:
    /// each seat waits out its grace for its player to return.
    pub fn load(bytes: &[u8], cfg: RoomConfig, now: u64) -> Result<Self, String> {
        let bad = || "not a saved room".to_string();
        let rest = bytes.strip_prefix(SAVE_MAGIC).ok_or_else(bad)?;
        let (n, mut rest) = rest.split_first_chunk::<2>().ok_or_else(bad)?;
        let mut seats = BTreeMap::new();
        for _ in 0..u16::from_le_bytes(*n) {
            let (seat, r) = rest.split_first_chunk::<2>().ok_or_else(bad)?;
            let (&[real, len], r) = r.split_first_chunk::<2>().ok_or_else(bad)?;
            let (player, r) = r.split_at_checked(usize::from(len)).ok_or_else(bad)?;
            seats.insert(
                u16::from_le_bytes(*seat),
                SeatInfo {
                    player: String::from_utf8_lossy(player).into_owned(),
                    real: real != 0,
                    conn: None,
                    gone_since: Some(now),
                    last_late: 0,
                },
            );
            rest = r;
        }
        let mut room = Self::around(S::restore(rest)?, cfg);
        // Everyone is disconnected now, and the world holds each seat's last
        // input: idle them all, as a drop would, so nobody walks on as a
        // ghost while their grace runs.
        let at = room.sim.tick();
        let idle = seats
            .keys()
            .map(|seat| Change::Input(*seat, vec![0; S::INPUT_BYTES]));
        room.pending.entry(at).or_default().extend(idle);
        room.seats = seats;
        Ok(room)
    }
}
