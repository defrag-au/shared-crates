//! Cameras for exploring a scene.
//!
//! A camera here takes **intents**, not devices: grab the ground under a
//! point of the screen and drag it, zoom toward a point, orbit, hold keys to
//! travel, fly somewhere. A viewer maps its own mouse, touch and keys to them
//! (`macroquad-gamekit`'s `steer` does it for macroquad), so the same camera
//! answers a trackpad, a phone and a keyboard, and a test can drive it with
//! no input at all. `update(dt, aspect)` moves whatever is under way by a
//! frame.
//!
//! - [`ground`] — an orthographic camera over flat ground, z up: an
//!   isometric diorama, a map, a board.
//! - [`motion`] — what any camera shares: held keys that ease in and out
//!   ([`motion::Drive`]), and the easing itself.
//!
//! The feel (ease times, a step of zoom being a fifth of the view, coasting
//! after a drag) is `unify-space`'s globe camera's, so a flat scene and the
//! globe handle alike.

pub mod ground;
pub mod motion;
