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

use crate::sim::{Change, RoomSim, Seat};
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
}

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
        }
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
                },
                None,
            ) => self.hello(conn, &game, version, player, real, now),
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
        // The same player back within their grace gets their seat back.
        let returning = self
            .seats
            .iter()
            .find(|(_, s)| s.player == player && s.conn.is_none())
            .map(|(seat, _)| *seat);
        let seat = match returning {
            Some(seat) => seat,
            None => {
                let Some(free) = (0..S::MAX_SEATS as Seat).find(|s| !self.seats.contains_key(s))
                else {
                    return self.refuse(conn, "this world is full".to_string());
                };
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
