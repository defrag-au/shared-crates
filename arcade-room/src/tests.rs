//! The room and its clients together, over the simulated network, with a
//! sim small enough to read: dots that walk the way their seat steers.

use std::collections::BTreeMap;

use crate::room::{Room, RoomConfig, To};
use crate::sim::{Change, RoomSim, Seat};
use crate::testkit::{BotState, Harness};
use crate::wire::{ToClient, ToRoom};

#[derive(Clone, Debug, PartialEq)]
struct Walkers {
    tick: u64,
    /// Seat → position and held step.
    dots: BTreeMap<Seat, (i64, i64, [u8; 2])>,
}

impl RoomSim for Walkers {
    const GAME: &'static str = "walkers";
    const VERSION: u32 = 1;
    const TICK_HZ: u32 = 60;
    const MAX_SEATS: usize = 64;
    const INPUT_BYTES: usize = 2;

    fn new(_seed: u64) -> Self {
        Self {
            tick: 0,
            dots: BTreeMap::new(),
        }
    }

    fn tick(&self) -> u64 {
        self.tick
    }

    fn step(&mut self, changes: &[Change]) {
        for c in changes {
            match c {
                Change::Join(s) => {
                    self.dots.entry(*s).or_insert((0, 0, [0, 0]));
                }
                Change::Leave(s) => {
                    self.dots.remove(s);
                }
                Change::Input(s, b) => {
                    if let Some(d) = self.dots.get_mut(s) {
                        d.2 = [b[0], b[1]];
                    }
                }
            }
        }
        for d in self.dots.values_mut() {
            d.0 += i64::from(d.2[0] as i8);
            d.1 += i64::from(d.2[1] as i8);
        }
        self.tick += 1;
    }

    fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |v: u64| {
            for b in v.to_le_bytes() {
                h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
        };
        eat(self.tick);
        for (s, d) in &self.dots {
            eat(u64::from(*s));
            eat(d.0 as u64);
            eat(d.1 as u64);
            eat(u64::from(d.2[0]) << 8 | u64::from(d.2[1]));
        }
        h
    }

    fn snapshot(&self) -> Vec<u8> {
        let mut out = self.tick.to_le_bytes().to_vec();
        for (s, d) in &self.dots {
            out.extend_from_slice(&s.to_le_bytes());
            out.extend_from_slice(&d.0.to_le_bytes());
            out.extend_from_slice(&d.1.to_le_bytes());
            out.extend_from_slice(&d.2);
        }
        out
    }

    fn restore(bytes: &[u8]) -> Result<Self, String> {
        let (tick, mut rest) = bytes.split_first_chunk::<8>().ok_or("short")?;
        let mut w = Self {
            tick: u64::from_le_bytes(*tick),
            dots: BTreeMap::new(),
        };
        while !rest.is_empty() {
            let (chunk, r) = rest.split_first_chunk::<20>().ok_or("short")?;
            let seat = u16::from_le_bytes([chunk[0], chunk[1]]);
            let x = i64::from_le_bytes(chunk[2..10].try_into().expect("8"));
            let y = i64::from_le_bytes(chunk[10..18].try_into().expect("8"));
            w.dots.insert(seat, (x, y, [chunk[18], chunk[19]]));
            rest = r;
        }
        Ok(w)
    }
}

fn cfg() -> RoomConfig {
    RoomConfig::default()
}

/// A steering that changes every half second or so, different per player.
fn wander(i: usize, now: u64) -> Option<Vec<u8>> {
    let phase = (now / 480 + i as u64 * 7) % 5;
    let step = [[1u8, 0], [0, 1], [255, 0], [0, 255], [0, 0]][phase as usize];
    Some(step.to_vec())
}

fn playing(h: &Harness<Walkers>) -> usize {
    (0..h.bots.len()).filter(|i| h.client(*i).is_some()).count()
}

#[test]
fn sixty_four_players_at_mixed_latencies_agree_with_the_room() {
    let mut h = Harness::<Walkers>::new(1, cfg());
    for i in 0..64 {
        let latency = 10 + (i as u64 * 37) % 240;
        h.connect(&format!("p{i}"), true, latency, latency / 5);
    }
    h.run(20_000, |i, _, now| wander(i, now));
    assert_eq!(playing(&h), 64);
    let stats = h.room.stats();
    assert!(stats.hashes_checked > 64 * 15, "{stats:?}");
    assert_eq!(stats.resyncs, 0, "{stats:?}");
    assert!(stats.ticks > 1_100, "the world ran: {stats:?}");
    // Every client's confirmed world is the room's, as far as it has got.
    for i in 0..64 {
        let c = h.client(i).expect("playing");
        assert!(c.confirmed().tick() + 40 >= h.room.sim().tick());
    }
}

#[test]
fn a_slow_players_late_inputs_land_without_a_desync() {
    let mut h = Harness::<Walkers>::new(2, cfg());
    h.connect("near", true, 20, 2);
    // 400 ms each way: its first inputs, scheduled before it has measured
    // the round trip, reach the room after their ticks.
    let far = h.connect("far", true, 400, 50);
    h.run(10_000, |i, _, now| wander(i, now));
    let stats = h.room.stats();
    assert!(stats.late_inputs > 0, "{stats:?}");
    assert_eq!(stats.resyncs, 0, "{stats:?}");
    // The far player's dot did move: its inputs were applied.
    let seat = h.client(far).expect("playing").seat();
    let (x, y, _) = h.room.sim().dots[&seat];
    assert!(x != 0 || y != 0);
}

#[test]
fn a_player_joining_a_running_world_starts_from_its_snapshot() {
    let mut h = Harness::<Walkers>::new(3, cfg());
    for i in 0..10 {
        h.connect(&format!("early{i}"), true, 30, 5);
    }
    h.run(5_000, |i, _, now| wander(i, now));
    let late = h.connect("late", true, 60, 5);
    h.run(5_000, |i, _, now| wander(i, now));
    let c = h.client(late).expect("playing");
    assert!(c.confirmed().dots.len() == 11);
    assert_eq!(h.room.stats().resyncs, 0);
}

#[test]
fn the_prediction_shows_a_press_at_once() {
    let mut h = Harness::<Walkers>::new(4, cfg());
    let i = h.connect("solo", true, 80, 0);
    h.run(1_000, |_, _, _| None);
    let c = h.client(i).expect("playing");
    let seat = c.seat();
    let before = c.predicted().dots[&seat].0;
    // Press right, and look at the prediction on the same frame.
    let now = h.now;
    if let BotState::Playing(c) = &mut h.bots[i].state {
        c.input(&[1, 0], now);
        let predicted = c.predicted();
        assert_eq!(
            predicted.dots[&seat].0,
            before + 1,
            "moved one tick's worth"
        );
        assert_eq!(
            c.confirmed().dots[&seat].0,
            before,
            "the confirmed world has not seen it yet"
        );
    }
}

#[test]
fn both_timelines_move_forward_smoothly_through_jitter() {
    let mut h = Harness::<Walkers>::new(9, cfg());
    h.connect("steady", true, 30, 0);
    let wobbly = h.connect("wobbly", true, 80, 40);
    // (now, behind tick, ahead tick, confirmed tick) every frame.
    let mut seen: Vec<(u64, u64, u64, u64)> = Vec::new();
    h.run(10_000, |i, c, now| {
        if i == wobbly {
            seen.push((
                now,
                c.behind().tick(),
                c.predicted().tick(),
                c.confirmed().tick(),
            ));
        }
        wander(i, now)
    });
    let settled: Vec<_> = seen.iter().filter(|s| s.0 > 3_000).collect();
    assert!(settled.len() > 300);
    for pair in settled.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        assert!(
            b.1 >= a.1 && b.2 >= a.2,
            "a timeline went backwards: {a:?} → {b:?}"
        );
        // A frame is 16 ms, a tick 16.7: a smooth timeline moves 0 to 2
        // ticks a frame, never a batch's worth at once.
        assert!(b.1 - a.1 <= 2, "the behind timeline jumped: {a:?} → {b:?}");
        assert!(b.2 - a.2 <= 2, "the ahead timeline jumped: {a:?} → {b:?}");
    }
    for s in &settled {
        assert!(s.1 <= s.3, "behind ran past the confirmed world: {s:?}");
        assert!(s.2 > s.3, "ahead is ahead of the confirmed world: {s:?}");
    }
    assert_eq!(h.room.stats().resyncs, 0);
}

#[test]
fn the_world_pauses_without_a_real_player_and_resumes_where_it_stopped() {
    let mut h = Harness::<Walkers>::new(5, cfg());
    let person = h.connect("person", true, 20, 0);
    // A bot is not a real player: it cannot hold the world open.
    h.connect("bot", false, 20, 0);
    h.run(3_000, |i, _, now| wander(i, now));
    assert!(h.room.running());
    h.disconnect(person);
    h.run(200, |_, _, _| None);
    assert!(!h.room.running(), "only a bot left");
    let stopped = h.room.sim().tick();
    h.run(60_000, |i, _, now| wander(i, now));
    assert_eq!(
        h.room.sim().tick(),
        stopped,
        "a minute later, not a tick on"
    );
    // Someone arrives: the world goes on from the tick it stopped at.
    h.connect("person2", true, 20, 0);
    h.run(1_000, |i, _, now| wander(i, now));
    assert!(h.room.running());
    let t = h.room.sim().tick();
    assert!(t > stopped && t < stopped + 70, "{stopped} → {t}");
}

#[test]
fn a_reconnect_within_the_grace_keeps_the_seat_and_after_it_frees_it() {
    let mut h = Harness::<Walkers>::new(6, cfg());
    h.connect("anchor", true, 20, 0);
    let a = h.connect("alice", true, 20, 0);
    h.run(1_000, |_, _, _| None);
    let seat = h.client(a).expect("playing").seat();
    h.disconnect(a);
    h.run(5_000, |_, _, _| None);
    // Back while the world is running, so batches are being confirmed as the
    // hello arrives: the welcome must still come first.
    let back = h.connect("alice", true, 20, 0);
    h.run(1_000, |_, _, _| None);
    assert_eq!(h.client(back).expect("playing").seat(), seat);

    h.disconnect(back);
    h.run(31_000, |_, _, _| None);
    assert!(
        !h.room.sim().dots.contains_key(&seat),
        "left after the grace"
    );
}

#[test]
fn a_full_world_and_another_version_are_refused() {
    let mut h = Harness::<Walkers>::new(7, cfg());
    for i in 0..64 {
        h.connect(&format!("p{i}"), true, 10, 0);
    }
    let extra = h.connect("extra", true, 10, 0);
    h.run(1_000, |_, _, _| None);
    assert!(matches!(&h.bots[extra].state, BotState::Refused(why) if why.contains("full")));

    let mut room = Room::<Walkers>::new(1, cfg());
    room.open(1);
    let hello = ToRoom::Hello {
        game: "walkers".into(),
        version: 2,
        player: "x".into(),
        real: true,
    };
    room.message(1, &hello.encode(), 0);
    let out = room.drain();
    assert!(out[0].close);
    assert!(matches!(
        ToClient::decode(&out[0].bytes),
        Ok(ToClient::Refused { reason }) if reason.contains("reload")
    ));
}

#[test]
fn a_wrong_hash_gets_a_resync() {
    let mut room = Room::<Walkers>::new(1, cfg());
    room.open(1);
    room.message(1, &crate::client::hello::<Walkers>("p", true), 0);
    room.poll(2_000);
    room.drain();
    room.message(1, &ToRoom::Hash { tick: 60, hash: 1 }.encode(), 2_000);
    let out = room.drain();
    assert!(
        out.iter().any(|o| o.to == To::One(1)
            && matches!(ToClient::decode(&o.bytes), Ok(ToClient::Resync { .. })))
    );
    assert_eq!(room.stats().resyncs, 1);
}

#[test]
fn a_saved_room_loads_and_its_players_get_their_seats_back() {
    let mut h = Harness::<Walkers>::new(8, cfg());
    let a = h.connect("alice", true, 20, 0);
    h.connect("bob", true, 20, 0);
    h.run(2_000, |i, _, now| wander(i, now));
    let seat = h.client(a).expect("playing").seat();
    let saved = h.room.save();
    let tick = h.room.sim().tick();

    let mut room = Room::<Walkers>::load(&saved, cfg(), 0).expect("loads");
    assert_eq!(room.sim().tick(), tick);
    assert_eq!(room.seated(), 2);
    room.open(9);
    room.message(9, &crate::client::hello::<Walkers>("alice", true), 100);
    let welcome = room.drain().into_iter().next().expect("welcome");
    assert!(matches!(
        ToClient::decode(&welcome.bytes),
        Ok(ToClient::Welcome { seat: s, .. }) if s == seat
    ));
}
