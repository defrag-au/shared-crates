//! A planet's crust through geological time.
//!
//! Plates drift rigidly over the sphere, new sea floor forms in the gaps
//! between plates pulling apart, sea floor sinks as it ages, and land wears
//! down. Follows the procedural model of Cortial, Peytavie, Galin and Guérin,
//! *Procedural Tectonic Planets* (Eurographics 2019): plate rules that
//! reproduce what tectonics does to a planet's surface, not mantle physics.
//!
//! This is the first phase: drift, new sea floor and ageing. Where plates
//! converge, the crust on top is decided but no uplift is raised yet;
//! subduction, collision and rifting come next.
//!
//! - [`sim`] — the simulation and its frames.
//! - [`crust`] — a sample's crust and how it ages.
//! - [`sphere`] — the sample points and the spatial index over them.

pub mod crust;
mod noise;
pub mod sim;
pub mod sphere;

pub use crust::{Crust, Kind};
pub use sim::{Frame, Motion, Params, Sim, Start, Stats};
