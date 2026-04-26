//! Wave Function Collapse solver for procedural generation.
//!
//! WFC generates patterns by ensuring local adjacency constraints are satisfied
//! everywhere in the output grid. Given a tileset with rules about which tiles
//! can appear next to each other, the solver collapses possibilities one cell
//! at a time and propagates constraints to neighbors.
//!
//! # Example
//!
//! ```
//! use wfc::{Tileset, WfcSolver, SolveResult};
//! use rand::SeedableRng;
//!
//! let mut tileset = Tileset::new();
//!
//! // Define tiles with edge labels
//! let grass = tileset.add_tile("grass");
//! let road_h = tileset.add_tile("road_h");
//!
//! // Set edge compatibility: which edges can be adjacent
//! tileset.set_edges(grass, ["g", "g", "g", "g"]); // all sides grass
//! tileset.set_edges(road_h, ["g", "r", "g", "r"]); // top/bottom grass, left/right road
//!
//! // Solve a 10x10 grid
//! let solver = WfcSolver::new(10, 10, &tileset);
//! let mut rng = rand::rngs::SmallRng::seed_from_u64(42);
//! let result = solver.solve(&mut tileset, &mut rng);
//! assert!(matches!(result, SolveResult::Solved(_)));
//! ```

mod grid;
mod solver;
mod tileset;

pub use grid::WfcGrid;
pub use solver::{SolveResult, WfcSolver};
pub use tileset::{TileId, Tileset};

/// Cardinal directions for adjacency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// +Y (row below)
    South,
    /// -Y (row above)
    North,
    /// +X (column right)
    East,
    /// -X (column left)
    West,
}

impl Direction {
    pub const ALL: [Direction; 4] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];

    /// The opposite direction.
    pub fn opposite(self) -> Direction {
        match self {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::East => Direction::West,
            Direction::West => Direction::East,
        }
    }

    /// Index for edge arrays: N=0, E=1, S=2, W=3.
    pub fn index(self) -> usize {
        match self {
            Direction::North => 0,
            Direction::East => 1,
            Direction::South => 2,
            Direction::West => 3,
        }
    }
}
