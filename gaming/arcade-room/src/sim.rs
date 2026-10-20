//! `RoomSim` — what a game supplies to run in a room: a deterministic world
//! stepped one tick at a time on everyone's changes.

/// A player's stable id in a world. Not an index: a departure never
/// renumbers anyone.
pub type Seat = u16;

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
}
