//! The connection's lifecycle against a real room in this process: a socket
//! that routes frames to and from an `arcade_room::Room`, which can open late,
//! go silent, or drop.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

use arcade_room::{Change, ConnId, Room, RoomConfig, RoomSim, Seat, To};

use super::*;

/// Each seat's total of the bytes it has held: enough to see inputs land.
#[derive(Clone, Debug, PartialEq)]
struct Tally {
    tick: u64,
    seats: BTreeMap<Seat, (i64, u8)>,
}

impl RoomSim for Tally {
    const GAME: &'static str = "tally";
    const VERSION: u32 = 1;
    const TICK_HZ: u32 = 60;
    const MAX_SEATS: usize = 2;
    const INPUT_BYTES: usize = 1;

    fn new(_seed: u64) -> Self {
        Self {
            tick: 0,
            seats: BTreeMap::new(),
        }
    }

    fn tick(&self) -> u64 {
        self.tick
    }

    fn step(&mut self, changes: &[Change]) {
        for c in changes {
            match c {
                Change::Join(s) => {
                    self.seats.entry(*s).or_insert((0, 0));
                }
                Change::Leave(s) => {
                    self.seats.remove(s);
                }
                Change::Input(s, b) => {
                    if let Some(seat) = self.seats.get_mut(s) {
                        seat.1 = b[0];
                    }
                }
                // Nothing kept: a tally starts from nothing.
                Change::Profile(..) | Change::World(_) => {}
            }
        }
        for seat in self.seats.values_mut() {
            seat.0 += i64::from(seat.1);
        }
        self.tick += 1;
    }

    fn hash(&self) -> u64 {
        self.snapshot().iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
        })
    }

    fn snapshot(&self) -> Vec<u8> {
        let mut out = self.tick.to_le_bytes().to_vec();
        for (s, (total, held)) in &self.seats {
            out.extend_from_slice(&s.to_le_bytes());
            out.extend_from_slice(&total.to_le_bytes());
            out.push(*held);
        }
        out
    }

    fn restore(bytes: &[u8]) -> Result<Self, String> {
        let tick = u64::from_le_bytes(bytes.get(..8).ok_or("short")?.try_into().expect("8"));
        let mut seats = BTreeMap::new();
        for c in bytes[8..].chunks(11) {
            let c: &[u8; 11] = c.try_into().map_err(|_| "ragged")?;
            let seat = u16::from_le_bytes([c[0], c[1]]);
            let total = i64::from_le_bytes(c[2..10].try_into().expect("8"));
            seats.insert(seat, (total, c[10]));
        }
        Ok(Self { tick, seats })
    }
}

struct Link {
    /// The socket counts as open from this ms on.
    opens_at: u64,
    /// Frames still flow: false once the line goes dead.
    alive: bool,
    to_client: VecDeque<Vec<u8>>,
    to_room: VecDeque<Vec<u8>>,
}

struct Net {
    room: Room<Tally>,
    now: u64,
    next: ConnId,
    links: BTreeMap<ConnId, Link>,
    /// How long a new socket takes to open.
    open_delay: u64,
}

thread_local! {
    static NET: RefCell<Option<Net>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Net) -> R) -> R {
    NET.with(|n| f(n.borrow_mut().as_mut().expect("a net is set up")))
}

fn setup(open_delay: u64) {
    NET.with(|n| {
        *n.borrow_mut() = Some(Net {
            room: Room::new(1, RoomConfig::default()),
            now: 0,
            next: 1,
            links: BTreeMap::new(),
            open_delay,
        })
    });
}

/// Frames in, the room's clock on, frames out.
fn pump(now: u64) {
    with(|net| {
        net.now = now;
        let ids: Vec<ConnId> = net.links.keys().copied().collect();
        for id in ids {
            let link = net.links.get_mut(&id).expect("listed");
            let frames: Vec<Vec<u8>> = link.to_room.drain(..).collect();
            if !link.alive {
                continue;
            }
            for f in frames {
                net.room.message(id, &f, now);
            }
        }
        net.room.poll(now);
        for out in net.room.drain() {
            let to = match out.to {
                To::One(id) => vec![id],
                To::Many(ids) => ids,
            };
            for id in to {
                if let Some(link) = net.links.get_mut(&id).filter(|l| l.alive) {
                    link.to_client.push_back(out.bytes.clone());
                }
            }
        }
    });
}

/// The line to `id` goes dead: nothing either way, and no word of it to the
/// client, as quad-net reports no close. The room hears of it, as a Durable
/// Object would.
fn cut(id: ConnId) {
    with(|net| {
        if let Some(link) = net.links.get_mut(&id) {
            link.alive = false;
        }
        let now = net.now;
        net.room.close(id, now);
    });
}

struct TestSocket(ConnId);

impl Transport for TestSocket {
    fn connect(_url: &str) -> Result<Self, String> {
        with(|net| {
            let id = net.next;
            net.next += 1;
            net.room.open(id);
            let opens_at = net.now + net.open_delay;
            net.links.insert(
                id,
                Link {
                    opens_at,
                    alive: true,
                    to_client: VecDeque::new(),
                    to_room: VecDeque::new(),
                },
            );
            Ok(TestSocket(id))
        })
    }

    fn connected(&self) -> bool {
        with(|net| {
            net.links
                .get(&self.0)
                .is_some_and(|l| net.now >= l.opens_at)
        })
    }

    fn send(&self, bytes: &[u8]) -> Result<(), String> {
        with(|net| {
            if let Some(link) = net.links.get_mut(&self.0) {
                link.to_room.push_back(bytes.to_vec());
            }
            Ok(())
        })
    }

    fn try_recv(&mut self) -> Option<Vec<u8>> {
        with(|net| net.links.get_mut(&self.0)?.to_client.pop_front())
    }
}

type Conn = Connection<Tally, TestSocket>;

fn reconnecting() -> Config {
    Config {
        on_loss: OnLoss::Reconnect,
        ..Config::default()
    }
}

/// Run `conns` holding `input` from `from` to `to` ms, a frame at a time.
fn run(conns: &mut [&mut Conn], input: u8, from: u64, to: u64) {
    for now in (from..to).step_by(16) {
        for c in conns.iter_mut() {
            c.update(&[input], now);
        }
        pump(now);
    }
}

#[test]
fn it_joins_and_its_inputs_reach_the_room() {
    setup(0);
    let mut c = Conn::with_config("test://room", "ann", reconnecting());
    assert!(c.status().starts_with("connecting"));
    run(&mut [&mut c], 2, 0, 1_000);
    let seat = c.seat().expect("seated");
    assert!(c.status().starts_with(&format!("seat {seat}")));
    let total = with(|net| net.room.sim().seats[&seat].0);
    assert!(total > 50, "held 2 for most of a second: {total}");
}

#[test]
fn nothing_is_sent_until_the_socket_opens() {
    setup(500);
    let mut c = Conn::with_config("test://room", "ann", reconnecting());
    run(&mut [&mut c], 1, 0, 400);
    assert_eq!(with(|net| net.room.seated()), 0, "the hello waited");
    run(&mut [&mut c], 1, 400, 1_000);
    assert!(c.seat().is_some(), "and went once it opened");
}

#[test]
fn a_silent_line_reconnects_natively_and_gets_the_seat_back() {
    setup(0);
    let mut c = Conn::with_config("test://room", "ann", reconnecting());
    run(&mut [&mut c], 1, 0, 1_000);
    let seat = c.seat().expect("seated");
    cut(1);
    // Four seconds of silence, then a new connection: the pause before
    // retrying counts from the last frame heard, so it is already over.
    run(&mut [&mut c], 1, 1_000, 4_900);
    assert_eq!(with(|net| net.links.len()), 1, "not before the silence");
    run(&mut [&mut c], 1, 4_900, 9_000);
    assert_eq!(with(|net| net.links.len()), 2, "one reconnect");
    assert_eq!(c.seat(), Some(seat), "the same seat, within the grace");
}

#[test]
fn on_the_web_a_lost_line_stops_and_waits_to_reload() {
    setup(0);
    let cfg = Config {
        on_loss: OnLoss::Reload,
        ..Config::default()
    };
    let mut c = Conn::with_config("test://room", "ann", cfg);
    run(&mut [&mut c], 1, 0, 1_000);
    cut(1);
    run(&mut [&mut c], 1, 1_000, 6_000);
    assert!(c.stopped());
    assert!(c.status().contains("rejoining"), "{}", c.status());
    assert_eq!(
        with(|net| net.links.len()),
        1,
        "no second socket on the web"
    );
}

#[test]
fn a_bot_is_seated_but_keeps_nobody_s_world_awake() {
    setup(0);
    let bot = Config {
        real: false,
        ..reconnecting()
    };
    let mut b = Conn::with_config("test://room", "bot-0", bot);
    let mut a = Conn::with_config("test://room", "ann", reconnecting());
    run(&mut [&mut b, &mut a], 1, 0, 1_000);
    assert!(b.seat().is_some() && a.seat().is_some());
    assert_eq!(with(|net| net.room.real_connected()), 1, "only ann is real");
}

#[test]
fn a_session_joining_elsewhere_stops_the_older_connection_for_good() {
    setup(0);
    with(|net| {
        net.room.identify_with(Box::new(|t: &str| {
            t.strip_prefix("token-").map(String::from)
        }))
    });
    let web = Config {
        on_loss: OnLoss::Reload,
        ..Config::default()
    };
    let mut laptop = Conn::with_config("test://room", "a", web).with_session("token-ann");
    run(&mut [&mut laptop], 1, 0, 1_000);
    let seat = laptop.seat().expect("seated");
    let mut phone = Conn::with_config("test://room", "b", web).with_session("token-ann");
    run(&mut [&mut laptop, &mut phone], 1, 1_000, 2_000);
    assert_eq!(phone.seat(), Some(seat), "the phone has ann's seat");
    assert!(laptop.stopped(), "{}", laptop.status());
    assert!(
        laptop.status().contains("somewhere else"),
        "{}",
        laptop.status()
    );
    // And it stays stopped: no reload to take the seat back.
    run(&mut [&mut laptop, &mut phone], 1, 2_000, 10_000);
    assert!(laptop.stopped() && !laptop.status().contains("rejoining"));
    assert_eq!(phone.seat(), Some(seat));
}

#[test]
fn a_full_room_turns_the_next_player_away() {
    setup(0);
    let (mut a, mut b, mut late) = (
        Conn::with_config("test://room", "ann", reconnecting()),
        Conn::with_config("test://room", "bo", reconnecting()),
        Conn::with_config("test://room", "cy", reconnecting()),
    );
    run(&mut [&mut a, &mut b], 1, 0, 1_000);
    run(&mut [&mut a, &mut b, &mut late], 1, 1_000, 2_000);
    assert!(a.seat().is_some() && b.seat().is_some());
    assert!(late.stopped());
    assert!(late.status().starts_with("refused"), "{}", late.status());
}
