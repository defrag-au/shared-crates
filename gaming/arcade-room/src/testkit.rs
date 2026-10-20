//! A room and many clients in one process, over a simulated network, on a
//! simulated clock: how a game's sim is load-tested through a real room
//! before any Worker exists.
//!
//! Each link is a WebSocket, so it is ordered and lossless; what varies is
//! its latency, with jitter that never reorders. A dropped connection is a
//! [`Harness::disconnect`].

use std::collections::VecDeque;

use crate::client::{Client, hello};
use crate::room::{ConnId, Room, RoomConfig, To};
use crate::sim::RoomSim;

/// Ms a frame of the simulated clients lasts.
pub const FRAME_MS: u64 = 16;

struct Link {
    /// One-way latency, ms.
    latency: u64,
    jitter: u64,
    /// Frames on their way, with the time each arrives. Arrival times never
    /// decrease, as on a TCP stream.
    to_room: VecDeque<(u64, Vec<u8>)>,
    to_client: VecDeque<(u64, Vec<u8>)>,
}

/// What a simulated client is doing.
pub enum BotState<S: RoomSim> {
    /// Sent its hello, waiting on the reply.
    Joining,
    Playing(Client<S>),
    Refused(String),
    Gone,
}

pub struct Bot<S: RoomSim> {
    pub player: String,
    pub real: bool,
    pub conn: ConnId,
    pub state: BotState<S>,
    link: Link,
}

pub struct Harness<S: RoomSim> {
    pub now: u64,
    pub room: Room<S>,
    pub bots: Vec<Bot<S>>,
    cfg: RoomConfig,
    next_conn: ConnId,
    rng: u64,
}

impl<S: RoomSim> Harness<S> {
    pub fn new(seed: u64, cfg: RoomConfig) -> Self {
        Self {
            now: 1_000,
            room: Room::new(seed, cfg),
            bots: Vec::new(),
            cfg,
            next_conn: 1,
            rng: seed ^ 0x5DEE_CE66_D1CE_4E5B,
        }
    }

    fn random(&mut self, below: u64) -> u64 {
        // SplitMix64: the harness's own jitter, seeded, so a run repeats.
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) % below.max(1)
    }

    /// Connect a player with `latency` ms each way, ± `jitter`. Returns its
    /// index in [`Harness::bots`].
    pub fn connect(&mut self, player: &str, real: bool, latency: u64, jitter: u64) -> usize {
        let conn = self.next_conn;
        self.next_conn += 1;
        self.room.open(conn);
        let mut link = Link {
            latency,
            jitter,
            to_room: VecDeque::new(),
            to_client: VecDeque::new(),
        };
        let at = self.now + latency;
        link.to_room.push_back((at, hello::<S>(player, real)));
        self.bots.push(Bot {
            player: player.to_string(),
            real,
            conn,
            state: BotState::Joining,
            link,
        });
        self.bots.len() - 1
    }

    /// Drop a player's connection, as a closed tab would.
    pub fn disconnect(&mut self, i: usize) {
        let conn = self.bots[i].conn;
        self.bots[i].state = BotState::Gone;
        self.bots[i].link.to_room.clear();
        self.bots[i].link.to_client.clear();
        self.room.close(conn, self.now);
    }

    /// The playing client `i`, if it is.
    pub fn client(&self, i: usize) -> Option<&Client<S>> {
        match &self.bots[i].state {
            BotState::Playing(c) => Some(c),
            _ => None,
        }
    }

    fn arrival(&mut self, i: usize, to_room: bool) -> u64 {
        let (latency, jitter) = (self.bots[i].link.latency, self.bots[i].link.jitter);
        let mut at = self.now + latency + self.random(jitter * 2 + 1);
        at = at.saturating_sub(jitter);
        let queue = if to_room {
            &self.bots[i].link.to_room
        } else {
            &self.bots[i].link.to_client
        };
        at.max(queue.back().map_or(0, |(t, _)| *t))
    }

    /// Run for `ms`, a frame at a time. Each frame, `input` is asked for each
    /// playing client's controls (`None`: unchanged).
    pub fn run(
        &mut self,
        ms: u64,
        mut input: impl FnMut(usize, &Client<S>, u64) -> Option<Vec<u8>>,
    ) {
        let end = self.now + ms;
        while self.now < end {
            self.now += 1;
            self.deliver();
            if self.now.is_multiple_of(FRAME_MS) {
                for i in 0..self.bots.len() {
                    let BotState::Playing(client) = &self.bots[i].state else {
                        continue;
                    };
                    if let Some(bytes) = input(i, client, self.now) {
                        let now = self.now;
                        if let BotState::Playing(c) = &mut self.bots[i].state {
                            c.input(&bytes, now);
                        }
                    }
                    let now = self.now;
                    if let BotState::Playing(c) = &mut self.bots[i].state {
                        c.poll(now);
                    }
                    self.flush_client(i);
                }
            }
        }
    }

    fn flush_client(&mut self, i: usize) {
        let frames = match &mut self.bots[i].state {
            BotState::Playing(c) => c.drain(),
            _ => return,
        };
        for f in frames {
            let at = self.arrival(i, true);
            self.bots[i].link.to_room.push_back((at, f));
        }
    }

    fn deliver(&mut self) {
        let now = self.now;
        // Client → room.
        for i in 0..self.bots.len() {
            while let Some((at, _)) = self.bots[i].link.to_room.front() {
                if *at > now {
                    break;
                }
                let (_, bytes) = self.bots[i].link.to_room.pop_front().expect("front");
                let conn = self.bots[i].conn;
                self.room.message(conn, &bytes, now);
            }
        }
        self.room.poll(now);
        // Room → clients.
        for out in self.room.drain() {
            let conns = match out.to {
                To::One(c) => vec![c],
                To::Many(cs) => cs,
            };
            for conn in conns {
                if let Some(i) = self.bots.iter().position(|b| b.conn == conn) {
                    let at = self.arrival(i, false);
                    self.bots[i]
                        .link
                        .to_client
                        .push_back((at, out.bytes.clone()));
                }
            }
        }
        for i in 0..self.bots.len() {
            while let Some((at, _)) = self.bots[i].link.to_client.front() {
                if *at > now {
                    break;
                }
                let (_, bytes) = self.bots[i].link.to_client.pop_front().expect("front");
                let hash_every = self.cfg.hash_every;
                let bot = &mut self.bots[i];
                bot.state = match std::mem::replace(&mut bot.state, BotState::Gone) {
                    BotState::Joining => match Client::welcome(&bytes, now, hash_every) {
                        Ok(c) => BotState::Playing(c),
                        Err(why) => BotState::Refused(why),
                    },
                    BotState::Playing(mut c) => {
                        c.receive(&bytes, now).expect("the room sends valid frames");
                        BotState::Playing(c)
                    }
                    other => other,
                };
            }
            self.flush_client(i);
        }
    }
}

/// Step a world of `S` from `seed` for `ticks`, each tick's changes chosen by
/// `crowd` from the world as it stands, and fold its hash every `hash_every`
/// ticks into one number.
///
/// The cross-target determinism check: a game builds this into a small
/// crate twice, natively and as a wasm library a script runs under Node, and
/// the two numbers must match. If they do not, the sim steps differently in
/// a browser or a Durable Object than in a native test, and every shared
/// world of it will desync. It goes through [`RoomSim::step`], the path the
/// room and its clients take, input bytes and all.
///
/// The crowd should reach every mechanic the game has (not wander), or a
/// match proves little: hold it to that with a coverage test of its own.
pub fn crowd_hash<S: RoomSim>(
    seed: u64,
    ticks: u64,
    hash_every: u64,
    mut crowd: impl FnMut(&S) -> Vec<crate::sim::Change>,
) -> u64 {
    let mut world = S::new(seed);
    let mut folded = 0u64;
    for _ in 0..ticks {
        let changes = crowd(&world);
        world.step(&changes);
        if hash_every > 0 && world.tick().is_multiple_of(hash_every) {
            folded = folded.rotate_left(7) ^ world.hash();
        }
    }
    folded.rotate_left(7) ^ world.hash()
}

#[cfg(test)]
mod crowd_tests {
    use super::*;
    use crate::sim::{Change, Seat};

    /// A counter a seat's held byte adds to each tick.
    #[derive(Clone)]
    struct Sum(u64, u64);

    impl RoomSim for Sum {
        const GAME: &'static str = "sum";
        const VERSION: u32 = 1;
        const TICK_HZ: u32 = 60;
        const MAX_SEATS: usize = 1;
        const INPUT_BYTES: usize = 1;
        fn new(seed: u64) -> Self {
            Sum(0, seed)
        }
        fn tick(&self) -> u64 {
            self.0
        }
        fn step(&mut self, changes: &[Change]) {
            for c in changes {
                if let Change::Input(_, b) = c {
                    self.1 = self.1.wrapping_add(u64::from(b[0]));
                }
            }
            self.0 += 1;
        }
        fn hash(&self) -> u64 {
            self.1 ^ (self.0 << 32)
        }
        fn snapshot(&self) -> Vec<u8> {
            Vec::new()
        }
        fn restore(_: &[u8]) -> Result<Self, String> {
            Ok(Sum(0, 0))
        }
    }

    #[test]
    fn the_crowd_sees_the_world_and_its_changes_count() {
        let seat: Seat = 0;
        let run = |bump: u8| {
            crowd_hash::<Sum>(3, 600, 60, |w| {
                if w.tick() % 7 == 0 {
                    vec![Change::Input(seat, vec![bump])]
                } else {
                    Vec::new()
                }
            })
        };
        assert_eq!(run(1), run(1), "the same crowd, the same number");
        assert_ne!(run(1), run(2), "and a different one differs");
    }
}
