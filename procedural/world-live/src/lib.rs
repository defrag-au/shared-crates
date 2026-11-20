//! The live world: everything global is a pure function of (the baked world,
//! the world clock), so two clients that agree on the clock agree on the
//! world without exchanging state. See the compositor's
//! `docs/perps-world-map/baked-vs-live.md`.
//!
//! - [`clock`]: real time to world time, and world time to a [`Moment`]: the
//!   year, the day, the season and the time of day.
//! - [`sun`]: where the sun is at a moment, and how much light a latitude
//!   takes over a day.
//! - [`seasons`]: the shape of the year at a place (baked), and the
//!   temperature and rain at a moment from it.
//! - [`growth`]: snow cover and leaf at a moment, from those.
//!
//! Directions follow `world_bake`'s convention: unit vectors, the north pole
//! on +y, latitude `asin(y)`, longitude from +x towards +z.

pub mod clock;
pub mod growth;
pub mod seasons;
pub mod sun;

pub use clock::{Clock, Moment, Season};
pub use growth::GrowthModel;
pub use seasons::{SeasonModel, Seasonal};
pub use sun::Orbit;
