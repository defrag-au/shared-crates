use rand::prelude::*;

use crate::grid::WfcGrid;
use crate::tileset::{TileId, Tileset};
use crate::Direction;

/// Result of a WFC solve attempt.
#[derive(Debug)]
pub enum SolveResult {
    /// Successfully solved — all cells collapsed to a single tile.
    Solved(WfcGrid),
    /// Contradiction — could not satisfy all constraints.
    /// Contains the number of cells that were successfully collapsed before failure.
    Contradiction(usize),
}

/// Internal cell state during solving.
#[derive(Debug, Clone)]
struct Cell {
    /// Bitset of candidate tile IDs still possible for this cell.
    /// candidates[i] == true means TileId(i) is still a possibility.
    candidates: Vec<bool>,
    /// Cached count of remaining candidates.
    count: usize,
    /// Whether this cell has been collapsed to a single tile.
    collapsed: bool,
}

/// Snapshot for backtracking.
#[derive(Clone)]
struct Snapshot {
    cells: Vec<Cell>,
    /// The cell that was collapsed in this step.
    collapsed_pos: usize,
    /// The tile it was collapsed to.
    collapsed_tile: TileId,
}

/// Wave Function Collapse solver.
///
/// Operates on a 2D grid where each cell starts with all tiles as candidates.
/// The solver iteratively:
/// 1. Finds the cell with lowest entropy (fewest candidates)
/// 2. Collapses it to a single tile (weighted random)
/// 3. Propagates constraints to neighbors
/// 4. Repeats until solved or contradicted
pub struct WfcSolver {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
    num_tiles: usize,
    /// Stack of snapshots for backtracking.
    history: Vec<Snapshot>,
    /// Maximum backtrack attempts before giving up.
    max_backtracks: usize,
}

impl WfcSolver {
    /// Create a new solver for a grid of the given dimensions.
    pub fn new(width: usize, height: usize, tileset: &Tileset) -> Self {
        let num_tiles = tileset.len();
        let cells = vec![
            Cell {
                candidates: vec![true; num_tiles],
                count: num_tiles,
                collapsed: false,
            };
            width * height
        ];

        Self {
            width,
            height,
            cells,
            num_tiles,
            history: Vec::new(),
            max_backtracks: width * height * 4,
        }
    }

    /// Constrain a specific cell to only allow certain tiles.
    ///
    /// Useful for pinning border tiles, placing fixed features, etc.
    /// Must be called before `solve()`.
    pub fn constrain(&mut self, col: usize, row: usize, allowed: &[TileId]) {
        let idx = row * self.width + col;
        let cell = &mut self.cells[idx];

        for i in 0..self.num_tiles {
            cell.candidates[i] = false;
        }
        for &tile in allowed {
            cell.candidates[tile.0 as usize] = true;
        }
        cell.count = allowed.len();
    }

    /// Pre-collapse a cell to a specific tile.
    ///
    /// The tile is fixed and constraints are propagated immediately.
    pub fn pin(&mut self, col: usize, row: usize, tile: TileId, tileset: &mut Tileset) {
        let idx = row * self.width + col;
        self.collapse_cell(idx, tile);
        self.propagate(idx, tileset);
    }

    /// Run the solver to completion.
    pub fn solve(mut self, tileset: &mut Tileset, rng: &mut impl Rng) -> SolveResult {
        let total_cells = self.width * self.height;
        let mut collapsed_count = self.cells.iter().filter(|c| c.collapsed).count();

        let mut backtracks = 0;

        while collapsed_count < total_cells {
            // Find cell with minimum entropy (fewest candidates, >1)
            let next = self.find_min_entropy(rng);

            let next = match next {
                Some(idx) => idx,
                None => {
                    // All remaining cells have 0 candidates — contradiction
                    if !self.backtrack(tileset) || backtracks >= self.max_backtracks {
                        return SolveResult::Contradiction(collapsed_count);
                    }
                    backtracks += 1;
                    collapsed_count = self.cells.iter().filter(|c| c.collapsed).count();
                    continue;
                }
            };

            if self.cells[next].count == 0 {
                // Contradiction at this cell
                if !self.backtrack(tileset) || backtracks >= self.max_backtracks {
                    return SolveResult::Contradiction(collapsed_count);
                }
                backtracks += 1;
                collapsed_count = self.cells.iter().filter(|c| c.collapsed).count();
                continue;
            }

            // Save snapshot for backtracking
            let tile = self.choose_tile(next, tileset, rng);

            self.history.push(Snapshot {
                cells: self.cells.clone(),
                collapsed_pos: next,
                collapsed_tile: tile,
            });

            // Collapse and propagate
            self.collapse_cell(next, tile);
            let ok = self.propagate(next, tileset);

            if !ok {
                // Propagation caused a contradiction — backtrack
                if !self.backtrack(tileset) || backtracks >= self.max_backtracks {
                    return SolveResult::Contradiction(collapsed_count);
                }
                backtracks += 1;
                collapsed_count = self.cells.iter().filter(|c| c.collapsed).count();
                continue;
            }

            collapsed_count += 1;
        }

        // Build output grid
        let resolved: Vec<TileId> = self
            .cells
            .iter()
            .map(|cell| {
                for i in 0..self.num_tiles {
                    if cell.candidates[i] {
                        return TileId(i as u16);
                    }
                }
                TileId(0) // shouldn't happen
            })
            .collect();

        SolveResult::Solved(WfcGrid::new(self.width, self.height, resolved))
    }

    /// Find the uncollapsed cell with the fewest remaining candidates.
    /// Ties are broken randomly for variety.
    fn find_min_entropy(&self, rng: &mut impl Rng) -> Option<usize> {
        let mut min_count = usize::MAX;
        let mut candidates: Vec<usize> = Vec::new();

        for (i, cell) in self.cells.iter().enumerate() {
            if cell.collapsed {
                continue;
            }

            if cell.count == 0 {
                return Some(i); // contradiction cell — caller will handle
            }

            if cell.count < min_count {
                min_count = cell.count;
                candidates.clear();
                candidates.push(i);
            } else if cell.count == min_count {
                candidates.push(i);
            }
        }

        if candidates.is_empty() {
            None
        } else {
            let idx = rng.random_range(0..candidates.len());
            Some(candidates[idx])
        }
    }

    /// Choose a tile for a cell, weighted by tile weights.
    fn choose_tile(&self, cell_idx: usize, tileset: &Tileset, rng: &mut impl Rng) -> TileId {
        let cell = &self.cells[cell_idx];

        // Collect candidates with weights
        let mut total_weight = 0.0f32;
        let mut options: Vec<(TileId, f32)> = Vec::new();

        for i in 0..self.num_tiles {
            if cell.candidates[i] {
                let w = tileset.weight(TileId(i as u16));
                total_weight += w;
                options.push((TileId(i as u16), w));
            }
        }

        if options.is_empty() {
            return TileId(0);
        }

        // Weighted random selection
        let mut roll = rng.random::<f32>() * total_weight;
        for (tile, w) in &options {
            roll -= w;
            if roll <= 0.0 {
                return *tile;
            }
        }

        options.last().unwrap().0
    }

    /// Collapse a cell to a single tile.
    fn collapse_cell(&mut self, idx: usize, tile: TileId) {
        let cell = &mut self.cells[idx];
        for i in 0..self.num_tiles {
            cell.candidates[i] = i == tile.0 as usize;
        }
        cell.count = 1;
        cell.collapsed = true;
    }

    /// Propagate constraints from a collapsed cell to its neighbors.
    ///
    /// Uses a worklist algorithm: when a cell's candidates change,
    /// all its neighbors are re-checked.
    ///
    /// Returns false if a contradiction is detected (any cell reaches 0 candidates).
    fn propagate(&mut self, start: usize, tileset: &mut Tileset) -> bool {
        let mut worklist = vec![start];

        while let Some(current) = worklist.pop() {
            let col = current % self.width;
            let row = current / self.width;

            for dir in Direction::ALL {
                let (ncol, nrow) = match dir {
                    Direction::North => {
                        if row == 0 {
                            continue;
                        }
                        (col, row - 1)
                    }
                    Direction::South => {
                        if row + 1 >= self.height {
                            continue;
                        }
                        (col, row + 1)
                    }
                    Direction::East => {
                        if col + 1 >= self.width {
                            continue;
                        }
                        (col + 1, row)
                    }
                    Direction::West => {
                        if col == 0 {
                            continue;
                        }
                        (col - 1, row)
                    }
                };

                let neighbor_idx = nrow * self.width + ncol;
                if self.cells[neighbor_idx].collapsed {
                    continue;
                }

                // Build union of compatible tiles for this neighbor
                // A candidate in the neighbor is valid only if at least one
                // candidate in the current cell allows it via adjacency
                let mut changed = false;

                for n_tile in 0..self.num_tiles {
                    if !self.cells[neighbor_idx].candidates[n_tile] {
                        continue;
                    }

                    // Is n_tile compatible with ANY candidate in current cell?
                    let compatible = (0..self.num_tiles).any(|c_tile| {
                        if !self.cells[current].candidates[c_tile] {
                            return false;
                        }
                        let neighbors = tileset.compatible(TileId(c_tile as u16), dir);
                        neighbors.contains(&TileId(n_tile as u16))
                    });

                    if !compatible {
                        self.cells[neighbor_idx].candidates[n_tile] = false;
                        self.cells[neighbor_idx].count -= 1;
                        changed = true;
                    }
                }

                if self.cells[neighbor_idx].count == 0 {
                    return false; // contradiction
                }

                if changed {
                    worklist.push(neighbor_idx);
                }
            }
        }

        true
    }

    /// Backtrack to the previous state and try a different tile.
    fn backtrack(&mut self, tileset: &mut Tileset) -> bool {
        while let Some(snapshot) = self.history.pop() {
            // Restore state from before this collapse
            self.cells = snapshot.cells;

            let cell_idx = snapshot.collapsed_pos;
            let failed_tile = snapshot.collapsed_tile;

            // Remove the failed tile from candidates
            self.cells[cell_idx].candidates[failed_tile.0 as usize] = false;
            self.cells[cell_idx].count -= 1;

            if self.cells[cell_idx].count > 0 {
                // Re-propagate with the removed candidate
                let ok = self.propagate(cell_idx, tileset);
                if ok {
                    return true;
                }
                // If propagation fails, continue backtracking
            }
        }

        false // exhausted all options
    }

    /// Get the current grid dimensions.
    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }
}
