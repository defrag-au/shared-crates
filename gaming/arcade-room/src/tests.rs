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

/// How close two walkers must be to keep a peer link: a standing-in for a
/// game's area of interest, small enough that a test can walk one dot out of
/// range in a second.
const NEAR: i64 = 4;

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
                // A walker's saved place: where they start from.
                Change::Profile(s, b) => {
                    if let (Some(d), [x, ..]) = (self.dots.get_mut(s), b.as_slice()) {
                        d.0 = i64::from(*x) * 1_000;
                    }
                }
                // The host says the world shifts: everyone moves down.
                Change::World(b) => {
                    if let [dy, ..] = b.as_slice() {
                        for d in self.dots.values_mut() {
                            d.1 += i64::from(*dy);
                        }
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

    /// Walkers that are within [`NEAR`] of each other keep a peer link, and no
    /// more: a small standing-in for a game's area of interest, so a group test
    /// can walk one dot out of range and watch the set change. No stickiness:
    /// the tests here are about the room publishing what the sim says, and
    /// `island-sim` is where hysteresis is measured.
    fn interest(&self, seat: Seat, k: usize, _held: &[Seat]) -> Option<Vec<Seat>> {
        let (x, y, _) = *self.dots.get(&seat)?;
        let mut near: Vec<(i64, Seat)> = self
            .dots
            .iter()
            .filter(|(other, _)| **other != seat)
            .filter_map(|(other, d)| {
                let (dx, dy) = (d.0 - x, d.1 - y);
                let d2 = dx * dx + dy * dy;
                (d2 <= NEAR * NEAR).then_some((d2, *other))
            })
            .collect();
        near.sort_by_key(|(d2, other)| (*d2, *other));
        Some(near.into_iter().take(k).map(|(_, other)| other).collect())
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

/// A room that accepts `token-<name>` as `<name>`.
fn verifying(seed: u64) -> Harness<Walkers> {
    let mut h = Harness::<Walkers>::new(seed, cfg());
    h.room.identify_with(Box::new(|token: &str| {
        token.strip_prefix("token-").map(str::to_string)
    }));
    h
}

#[test]
fn a_verified_player_keeps_their_seat_and_a_guest_cannot_claim_it() {
    let mut h = verifying(11);
    h.connect("anchor", true, 20, 0);
    let a = h.connect_as("tab-1", "token-alice", true, 20, 0);
    h.run(1_000, |_, _, _| None);
    let seat = h.client(a).expect("playing").seat();
    assert_eq!(h.room.player(seat), Some("id:alice"));
    h.disconnect(a);
    h.run(1_000, |_, _, _| None);
    // A guest naming themselves after alice's key is a different player.
    let spoof = h.connect("id:alice", true, 20, 0);
    h.run(1_000, |_, _, _| None);
    let guest_seat = h.client(spoof).expect("playing").seat();
    assert_ne!(guest_seat, seat);
    assert_eq!(h.room.player(guest_seat), Some("guest:id:alice"));
    // Alice, from another device under another name, is still alice.
    let back = h.connect_as("phone", "token-alice", true, 20, 0);
    h.run(1_000, |_, _, _| None);
    assert_eq!(h.client(back).expect("playing").seat(), seat);
}

#[test]
fn a_second_connection_of_one_identity_takes_the_seat_over() {
    let mut h = verifying(12);
    let first = h.connect_as("tab-1", "token-bob", true, 20, 0);
    h.run(1_000, |_, _, _| None);
    let seat = h.client(first).expect("playing").seat();
    let second = h.connect_as("tab-2", "token-bob", true, 20, 0);
    h.run(1_000, |_, _, _| None);
    assert_eq!(h.client(second).expect("playing").seat(), seat);
    assert!(
        matches!(h.bots[first].state, BotState::Refused(_)),
        "the older connection is closed"
    );
    assert_eq!(h.room.seated(), 1);
}

#[test]
fn a_hosts_profile_reaches_every_client_and_only_new_seats_are_listed() {
    let mut h = verifying(14);
    let watcher = h.connect("watcher", true, 30, 5);
    let a = h.connect_as("tab", "token-dana", true, 20, 0);
    h.run(1_000, |_, _, _| None);
    let joined = h.room.take_joined();
    let seat = h.client(a).expect("playing").seat();
    assert!(
        joined.contains(&(seat, "id:dana".to_string())),
        "{joined:?}"
    );
    assert!(h.room.take_joined().is_empty(), "listed once");

    // The host loads dana's saved place and tells the world.
    assert!(h.room.profile("id:dana", vec![7]));
    assert!(!h.room.profile("id:nobody", vec![1]), "no seat, no profile");
    h.run(1_000, |_, _, _| None);
    for client in [a, watcher] {
        let x = h.client(client).expect("playing").confirmed().dots[&seat].0;
        assert_eq!(x, 7_000, "every copy applied it");
    }

    // A fact about the whole world reaches every copy at the same tick.
    let before: Vec<i64> = [a, watcher]
        .map(|c| h.client(c).expect("playing").confirmed().dots[&seat].1)
        .to_vec();
    h.room.world_fact(vec![50]);
    h.run(1_000, |_, _, _| None);
    for (client, y0) in [a, watcher].into_iter().zip(before) {
        let y = h.client(client).expect("playing").confirmed().dots[&seat].1;
        assert_eq!(y, y0 + 50, "every copy applied it once");
    }

    // Back within the grace: the same seat, not a new arrival.
    h.disconnect(a);
    h.run(1_000, |_, _, _| None);
    h.connect_as("phone", "token-dana", true, 20, 0);
    h.run(1_000, |_, _, _| None);
    assert!(h.room.take_joined().is_empty());
}

#[test]
fn a_session_that_does_not_verify_plays_as_a_guest() {
    let mut h = verifying(13);
    let a = h.connect_as("carol", "forged", true, 20, 0);
    h.run(1_000, |_, _, _| None);
    let seat = h.client(a).expect("playing").seat();
    assert_eq!(h.room.player(seat), Some("guest:carol"));
}

#[test]
fn a_dropped_player_stands_still_through_their_grace() {
    let mut h = Harness::<Walkers>::new(10, cfg());
    h.connect("anchor", true, 20, 0);
    let walker = h.connect("walker", true, 20, 0);
    // Steer right and keep holding it.
    h.run(2_000, |i, _, _| (i == walker).then(|| vec![1, 0]));
    let seat = h.client(walker).expect("playing").seat();
    h.disconnect(walker);
    h.run(500, |_, _, _| None);
    let stopped = h.room.sim().dots[&seat].0;
    h.run(10_000, |_, _, _| None);
    let dot = h.room.sim().dots[&seat];
    assert_eq!(dot.0, stopped, "no ghost walking during the grace");
    assert_eq!(dot.2, [0, 0], "its input idled");
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
        session: String::new(),
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

/// A saved room comes back with nobody connected yet, so nobody's held input
/// carries on: a player walking when the room was saved stands still once it
/// loads, until they rejoin and steer again.
#[test]
fn a_loaded_room_idles_everyone_until_they_rejoin() {
    let mut h = Harness::<Walkers>::new(11, cfg());
    h.connect("anchor", true, 20, 0);
    let walker = h.connect("walker", true, 20, 0);
    h.run(2_000, |i, _, _| (i == walker).then(|| vec![1, 0]));
    let seat = h.client(walker).expect("playing").seat();
    assert_eq!(h.room.sim().dots[&seat].2, [1, 0], "walking when saved");
    let saved = h.room.save();

    let mut room = Room::<Walkers>::load(&saved, cfg(), 0).expect("loads");
    let before = room.sim().dots[&seat].0;
    room.open(9);
    room.message(9, &crate::client::hello::<Walkers>("anchor", true), 100);
    let start = room.sim().tick();
    for now in (100..5_000).step_by(50) {
        room.poll(now);
        room.drain();
    }
    assert!(room.sim().tick() > start + 100, "the room ran");
    let dot = room.sim().dots[&seat];
    assert_eq!(dot.0, before, "no ghost walking after a load");
    assert_eq!(dot.2, [0, 0], "its input idled");
}

/// A signal goes to the other seats and never back to its sender: the room
/// vouches for who sent it, and a peer's own message is not put in front of it.
#[test]
fn a_signal_reaches_the_other_seats_and_not_the_sender() {
    let mut room = Room::<Walkers>::new(1, cfg());
    room.open(1);
    room.open(2);
    room.message(1, &crate::client::hello::<Walkers>("a", true), 0);
    room.message(2, &crate::client::hello::<Walkers>("b", true), 0);

    let mut seat_of = std::collections::BTreeMap::new();
    for out in room.drain() {
        if let (To::One(conn), Ok(ToClient::Welcome { seat, .. })) =
            (out.to, ToClient::decode(&out.bytes))
        {
            seat_of.insert(conn, seat);
        }
    }
    let (a, b) = (seat_of[&1], seat_of[&2]);
    assert_ne!(a, b);

    let body = b"v=0\r\na=setup:actpass\r\n".to_vec();
    room.message(1, &ToRoom::Signal(body.clone()).encode(), 1_000);

    let mut heard = 0;
    for out in room.drain() {
        if let Ok(ToClient::Signal { from, bytes }) = ToClient::decode(&out.bytes) {
            heard += 1;
            assert_eq!(out.to, To::One(2), "the other seat, not the sender");
            assert_eq!(from, a, "the seat the room vouched for");
            assert_eq!(bytes, body, "carried verbatim");
        }
    }
    assert_eq!(heard, 1, "one recipient, and no echo to the sender");
    let _ = b;
}

/// One signal in is one frame; one signal out is one copy per seat it reaches.
/// The pair of counts is what the 64-seat measurement divides, so an off-by-one
/// here would make the peer plane look cheaper or dearer than it is.
#[test]
fn a_signal_is_counted_once_in_and_once_per_copy_out() {
    let mut h = Harness::<Walkers>::new(1, cfg());
    let a = h.connect("a", true, 10, 0);
    let b = h.connect("b", true, 10, 0);
    let c = h.connect("c", true, 10, 0);
    h.run(1_000, |_, _, _| None);

    assert_eq!(h.room.signals(), (0, 0), "nothing sent yet");

    let now = h.now;
    h.client_mut(a)
        .expect("seated")
        .send_announce(b"v=0\r\n".to_vec(), now);
    h.run(100, |_, _, _| None);

    // Walkers names every other seat, so one frame reaches two seats.
    assert_eq!(h.room.signals(), (1, 2), "one in, one copy per member");
    assert_eq!(
        h.client(a).expect("seated").signals(),
        (1, 0),
        "the sender counts what it sent and none of its own"
    );
    assert_eq!(
        h.client(b).expect("seated").signals(),
        (0, 1),
        "a recipient counts what arrived"
    );
    assert_eq!(
        h.client(c).expect("seated").signals(),
        (0, 1),
        "both recipients, not one"
    );
}

/// The player's own prediction is what the room later confirms: on a phone's
/// connection (120 ms each way, ±60 ms), once the client has measured the
/// link no input of theirs lands late, so their own character is never
/// corrected. (Before the first round trip comes back the client assumes
/// 100 ms, and a few of its first inputs may land late.)
#[test]
fn a_players_own_prediction_is_never_corrected_on_a_jittery_link() {
    use std::collections::BTreeMap;
    let mut h = Harness::<Walkers>::new(9, cfg());
    h.connect("steady", true, 30, 0);
    let phone = h.connect("phone", true, 120, 60);
    h.run(5_000, |i, _, now| wander(i, now));
    let late_while_measuring = h.room.stats().late_inputs;
    // This player's position as predicted, by tick, until confirmed.
    let mut predicted_x: BTreeMap<u64, i64> = BTreeMap::new();
    let (mut checked, mut wrong) = (0u32, 0u32);
    h.run(30_000, |i, c, now| {
        if i == phone {
            let seat = c.seat();
            let (now_world, next_world, _) = c.predicted_pair();
            for w in [&now_world, &next_world] {
                if let Some(d) = w.dots.get(&seat) {
                    predicted_x.insert(w.tick(), d.0);
                }
            }
            let done = c.confirmed().tick();
            if let (Some(px), Some(d)) = (predicted_x.remove(&done), c.confirmed().dots.get(&seat))
            {
                checked += 1;
                wrong += u32::from(px != d.0);
            }
            predicted_x.retain(|t, _| *t > done);
        }
        wander(i, now)
    });
    assert!(checked > 200, "{checked}");
    assert_eq!(wrong, 0, "of {checked}");
    assert_eq!(
        h.room.stats().late_inputs,
        late_while_measuring,
        "late once measured: {:?}",
        h.room.stats()
    );
}

/// The room publishes each seat's interest set, and it follows the world: two
/// walkers start in range of each other, and one that walks away leaves.
#[test]
fn a_group_is_the_seats_the_world_says_are_near() {
    let mut h = Harness::<Walkers>::new(15, cfg());
    let a = h.connect("a", true, 20, 0);
    let b = h.connect("b", true, 20, 0);
    h.run(1_200, |_, _, _| None);

    let (a_seat, b_seat) = (
        h.client(a).expect("playing").seat(),
        h.client(b).expect("playing").seat(),
    );
    let group = |h: &Harness<Walkers>, i: usize| {
        h.client(i)
            .and_then(|c| c.group())
            .map(|(_, members)| members.to_vec())
    };

    assert_eq!(group(&h, a), Some(vec![b_seat]), "a sees b");
    assert_eq!(group(&h, b), Some(vec![a_seat]), "and b sees a");
    let first_epoch = h.client(a).and_then(|c| c.group()).map(|(e, _)| e);
    assert!(first_epoch.is_some_and(|e| e > 0), "a group is named");

    // Twenty units a tick for three and a half seconds is far outside `NEAR`.
    h.run(3_500, |i, _, _| (i == a).then(|| vec![20, 0]));
    // The room re-ranks on an interval rather than every tick, so the wait is a
    // regroup plus a little, named rather than guessed.
    h.run(crate::room::GROUP_EVERY_SECS * 1_000 + 500, |_, _, _| None);

    assert_eq!(group(&h, a), Some(Vec::new()), "a has walked out of range");
    assert_eq!(
        group(&h, b),
        Some(Vec::new()),
        "so b's set no longer names a"
    );
    assert!(
        h.client(a)
            .and_then(|c| c.group())
            .is_some_and(|(e, _)| Some(e) != first_epoch),
        "a change is a new epoch, not a quiet edit"
    );
}

/// Three seats and a deliberately asymmetric interest set: seat 0 sees 1, and 1
/// sees nobody. Enough to pin group-scoped delivery and the union that makes an
/// asymmetric pair work.
#[derive(Clone)]
struct Islands {
    tick: u64,
}

impl RoomSim for Islands {
    const GAME: &'static str = "islands";
    const VERSION: u32 = 1;
    const TICK_HZ: u32 = 60;
    const MAX_SEATS: usize = 8;
    const INPUT_BYTES: usize = 1;

    fn new(_seed: u64) -> Self {
        Self { tick: 0 }
    }
    fn tick(&self) -> u64 {
        self.tick
    }
    fn step(&mut self, _changes: &[Change]) {
        self.tick += 1;
    }
    fn hash(&self) -> u64 {
        self.tick
    }
    fn snapshot(&self) -> Vec<u8> {
        Vec::new()
    }
    fn restore(_bytes: &[u8]) -> Result<Self, String> {
        Ok(Self { tick: 0 })
    }
    fn interest(&self, seat: Seat, _k: usize, _held: &[Seat]) -> Option<Vec<Seat>> {
        Some(match seat {
            0 => vec![1],
            _ => Vec::new(),
        })
    }
}

/// A targeted signal is delivered to its target once and to nobody else.
///
/// The group says who is worth a channel; it does not make every handshake in
/// it the group's business. Before this, one offer was copied to every member
/// and every receiver but the target decoded and dropped it.
#[test]
fn a_targeted_signal_reaches_only_its_target() {
    let mut h = Harness::<Walkers>::new(1, cfg());
    let a = h.connect("a", true, 10, 0);
    let b = h.connect("b", true, 10, 0);
    let c = h.connect("c", true, 10, 0);
    h.run(1_000, |_, _, _| None);

    let seat_c = h.client(c).expect("seated").seat();
    let now = h.now;
    h.client_mut(a)
        .expect("seated")
        .send_signal(seat_c, b"v=0\r\n".to_vec(), now);
    h.run(100, |_, _, _| None);

    assert_eq!(h.room.signals(), (1, 1), "one in, one copy");
    assert_eq!(
        h.client(a).expect("seated").signals(),
        (1, 0),
        "the sender counts what it sent and none of its own"
    );
    assert_eq!(
        h.client(b).expect("seated").signals(),
        (0, 0),
        "a member of the group that is not the target heard nothing"
    );
    assert_eq!(
        h.client(c).expect("seated").signals(),
        (0, 1),
        "the target heard it once"
    );
}

/// A targeted signal goes to its sender's group and to anyone who names the
/// sender, and no further — the same reach the broadcast path uses, as a
/// membership test. The second half is not decoration: a capped distance set is
/// not symmetric, and the far end of an asymmetric pair still has to answer.
#[test]
fn a_targeted_signal_outside_the_reach_set_is_dropped() {
    let mut room = Room::<Islands>::new(1, cfg());
    for conn in 1..=3 {
        room.open(conn);
    }
    for (conn, name) in [(1, "a"), (2, "b"), (3, "c")] {
        room.message(conn, &crate::client::hello::<Islands>(name, true), 0);
    }
    room.drain();

    let heard = |room: &mut Room<Islands>, from_conn: u64, to: Seat| {
        room.message(
            from_conn,
            &ToRoom::SignalTo {
                to,
                bytes: b"v=0\r\n".to_vec(),
            }
            .encode(),
            1_000,
        );
        room.drain()
            .into_iter()
            .filter_map(|out| match (out.to, ToClient::decode(&out.bytes)) {
                (To::One(conn), Ok(ToClient::Signal { from, .. })) => Some((conn, from)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(heard(&mut room, 1, 1), vec![(2, 0)], "0's group names 1");
    assert_eq!(
        heard(&mut room, 2, 0),
        vec![(1, 1)],
        "1 reaches 0 through 0's group, though 1 names nobody"
    );
    assert!(heard(&mut room, 1, 2).is_empty(), "0 does not name 2");
    assert!(heard(&mut room, 3, 1).is_empty(), "2 is in nobody's group");
    assert_eq!(
        room.signals(),
        (4, 2),
        "every attempt is counted in, and only the two deliveries out"
    );
}

/// A signal goes to its sender's group and to anyone who names the sender, and
/// no further. The second half is not decoration: a capped distance set is not
/// symmetric, and the far end of an asymmetric pair still has to answer.
#[test]
fn a_signal_reaches_its_groups_union_and_no_further() {
    let mut room = Room::<Islands>::new(1, cfg());
    for conn in 1..=3 {
        room.open(conn);
    }
    for (conn, name) in [(1, "a"), (2, "b"), (3, "c")] {
        room.message(conn, &crate::client::hello::<Islands>(name, true), 0);
    }

    let mut seat_of = BTreeMap::new();
    for out in room.drain() {
        if let (To::One(conn), Ok(ToClient::Welcome { seat, .. })) =
            (out.to, ToClient::decode(&out.bytes))
        {
            seat_of.insert(conn, seat);
        }
    }
    for (conn, seat) in [(1, 0), (2, 1), (3, 2)] {
        assert_eq!(seat_of.get(&conn), Some(&seat), "seated in hello order");
    }

    let heard = |room: &mut Room<Islands>, from: u64| {
        room.message(from, &ToRoom::Signal(b"v=0\r\n".to_vec()).encode(), 1_000);
        room.drain()
            .into_iter()
            .filter_map(|out| match (out.to, ToClient::decode(&out.bytes)) {
                (To::One(conn), Ok(ToClient::Signal { from, .. })) => Some((conn, from)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(heard(&mut room, 1), vec![(2, 0)], "0's group is 1");
    assert_eq!(
        heard(&mut room, 2),
        vec![(1, 1)],
        "1's reply reaches 0 through 0's group, though 1 names nobody"
    );
    assert!(heard(&mut room, 3).is_empty(), "2 is in nobody's group");
}
