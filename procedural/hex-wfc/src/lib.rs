//! Socket-based Wave Function Collapse for hex grids.
//!
//! Lifted from augminted-bots' `microversus-wfc`. Game-agnostic: tiles are
//! abstract ids with socket constraints, and each tile's terrain is the
//! caller's own type `T`, parsed from the tile set's `terrain` strings via
//! `FromStr`.
//!
//! # Architecture
//!
//! - [`TileSet`]: loaded from TOML, defines tiles with 6 sockets each plus
//!   valid socket pairs. Rotational variants are expanded automatically.
//! - [`Solver`]: runs WFC on an even-r hex grid of a given size, with optional
//!   pre-collapsed cells and edge constraints from adjacent chunks.

mod solver;
mod tileset;

pub use solver::{EdgeConstraint, GridEdge, PreCollapsed, SolveError, SolveResult, Solver};
pub use tileset::{HexDirection, SocketId, Tile, TileId, TileSet, TileSetError, TileSetSummary};
