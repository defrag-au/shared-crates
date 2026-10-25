//! `RoomSim` — what a game supplies to run in a room: a deterministic world
//! stepped one tick at a time on everyone's changes.

/// A player's stable id in a world. Not an index: a departure never
/// renumbers anyone.
pub type Seat = u16;

/// The most peers one seat is asked to keep in touch with.
///
/// The room caps every interest set at this, and the browser shim caps its own
/// links at the same number (`MAX_LINKS` in `peer.js`). Full mesh is not the
/// plan: eight to twelve channels to players you can actually see is (§5 of
/// `docs/P2P_OVERLAY_DESIGN.md` in augminted-bots).
pub const INTEREST_MAX: usize = 12;

/// What changes in a world on one tick. A seat's input is held from one tick
/// to the next until a change replaces it, so only changes travel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// A player arrives.
    Join(Seat),
    /// A player leaves.
    Leave(Seat),
    /// A seated player's controls from this tick on, in the game's own
    /// encoding of exactly [`RoomSim::INPUT_BYTES`] bytes.
    Input(Seat, Vec<u8>),
    /// What the room's host knows about a seat's player from outside the
    /// world, in the game's own encoding: their saved progress, loaded when
    /// a verified player is seated ([`crate::Room::profile`]). Only the host
    /// sends it, never a client, and it arrives a moment after the
    /// [`Change::Join`], once the host has read it. A game that keeps nothing
    /// ignores it.
    Profile(Seat, Vec<u8>),
    /// What the room's host knows about the world from outside it, in the
    /// game's own encoding: a server's progress towards its goals, an event
    /// from the chain. No seat's. Only the host sends it
    /// ([`crate::Room::world_fact`]), and every client applies it at the
    /// same tick, so it is as deterministic as anything a player does.
    World(Vec<u8>),
}

/// A deterministic world a room can host.
///
/// Every client and the room step their own copy on the same confirmed
/// changes, so `step` must give the same result on every target: wasm32 in
/// browsers and Durable Objects, native in tests. No `HashMap` iteration, no
/// randomness but a seeded generator, no `sin`/`cos`/`powf` in anything that
/// reaches the state.
pub trait RoomSim: Clone + Sized {
    /// The game's id, as clients and rooms name it.
    const GAME: &'static str;
    /// Bump whenever stepping changes. A client and a room on different
    /// versions would diverge, so a room refuses a client on another one.
    const VERSION: u32;
    /// Ticks a second.
    const TICK_HZ: u32;
    /// The most players one world holds.
    const MAX_SEATS: usize;
    /// The size of one seat's input. Inputs of any other size are dropped.
    ///
    /// All zeros must mean *idle*: nothing pressed, nothing steering. The
    /// room sets a seat's input to zeros when its connection drops, so a
    /// player who lost signal mid-stride stands still through their grace
    /// period instead of walking on as a ghost.
    const INPUT_BYTES: usize;

    /// A fresh world.
    fn new(seed: u64) -> Self;
    /// Ticks stepped so far.
    fn tick(&self) -> u64;
    /// Apply `changes` in order, then advance one tick.
    fn step(&mut self, changes: &[Change]);
    /// A hash of everything that evolves: equal hashes after the same ticks
    /// is what the room checks clients against.
    fn hash(&self) -> u64;
    /// Everything that evolves, as bytes: what a joining client starts from,
    /// and what the room saves.
    fn snapshot(&self) -> Vec<u8>;
    /// The world a snapshot was taken of.
    fn restore(bytes: &[u8]) -> Result<Self, String>;

    /// Who `seat` should talk to over the peer plane: at most `k` others,
    /// nearest first if the game has a notion of near.
    ///
    /// The room intersects what this names with the seats it actually holds, so
    /// a stale or careless answer cannot name a ghost, and it publishes the
    /// result as a group. `None` means "no opinion": every other seated player,
    /// which is what a world small enough not to care wants, and what every
    /// game that has not implemented this keeps.
    ///
    /// Positions are the sim's, so this is where area-of-interest belongs:
    /// what is on screen is what is worth a data channel.
    fn interest(&self, _seat: Seat, _k: usize) -> Option<Vec<Seat>> {
        None
    }
}
