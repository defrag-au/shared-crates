//! `arcade-room` — lockstep rooms for deterministic games.
//!
//! A room hosts one world. Every client, and the room itself, steps its own
//! copy of the game's sim on the same confirmed stream of changes, so every
//! copy agrees without the world ever being sent. The design, and why it is
//! lockstep paced by the room's clock, is in augminted-bots'
//! `docs/LOCKSTEP_ROOM_DESIGN.md`.
//!
//! - [`RoomSim`]: what a game supplies.
//! - [`Room`]: one world's seats, clock and confirmed stream. The server half.
//! - [`Client`]: one player's view, with prediction. The client half.
//! - [`wire`]: the binary messages between them.
//! - [`testkit`] (feature `testkit`): a room and many clients over a
//!   simulated network, for load tests.
//!
//! Pure: no I/O, no clock, no dependencies. The same crate runs in a Durable
//! Object, a macroquad client and a native test; the caller passes the time.

pub mod client;
pub mod room;
pub mod sim;
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
pub mod wire;

pub use client::Client;
pub use room::{ConnId, Identify, Inbound, Out, Room, RoomConfig, RoomStats, To};

#[cfg(test)]
mod tests;
pub use sim::{Change, INTEREST_MAX, RoomSim, Schema, Seat};
