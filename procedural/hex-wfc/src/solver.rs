//! WFC solver for hex grids with backtracking.
//!
//! Uses AC-3 constraint propagation with u64 bitset domains and pre-computed
//! constraint masks. On contradiction, backtracks via an undo log of domain
//! snapshots. Falls back to full restart after exceeding backtrack depth.
//!
//! The grid is even-r offset: `(q, r)` with odd rows shifted right.

use std::collections::VecDeque;

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::tileset::{HexDirection, SocketId, TileId, TileSet};

/// Maximum number of tiles supported (u64 bitset width).
const MAX_TILES: usize = 64;

/// A pre-collapsed cell — used for edge constraints from adjacent chunks.
#[derive(Debug, Clone)]
pub struct PreCollapsed {
    pub q: i32,
    pub r: i32,
    pub tile_id: TileId,
}

/// A constrained cell — restrict which tiles are allowed without collapsing to one.
#[derive(Debug, Clone)]
pub struct EdgeConstraint {
    pub q: i32,
    pub r: i32,
    pub direction: HexDirection,
    pub neighbour_socket: SocketId,
}

/// Result of a successful solve.
#[derive(Debug, Clone)]
pub struct SolveResult {
    pub width: u32,
    pub height: u32,
    /// Tile ID at each cell. Indexed as `grid[r * width + q]`.
    pub grid: Vec<TileId>,
    pub iterations: u32,
    pub backtracks: u32,
}

impl SolveResult {
    pub fn get(&self, q: i32, r: i32) -> Option<TileId> {
        if q < 0 || r < 0 || q >= self.width as i32 || r >= self.height as i32 {
            return None;
        }
        Some(self.grid[r as usize * self.width as usize + q as usize])
    }

    pub fn edge_manifest<T>(&self, edge: GridEdge, tileset: &TileSet<T>) -> Vec<SocketId> {
        match edge {
            GridEdge::North => (0..self.width as i32)
                .map(|q| {
                    let tid = self.get(q, 0).unwrap();
                    tileset.tiles[tid as usize].sockets[HexDirection::NW as usize]
                })
                .collect(),
            GridEdge::South => (0..self.width as i32)
                .map(|q| {
                    let tid = self.get(q, self.height as i32 - 1).unwrap();
                    tileset.tiles[tid as usize].sockets[HexDirection::SE as usize]
                })
                .collect(),
            GridEdge::East => (0..self.height as i32)
                .map(|r| {
                    let tid = self.get(self.width as i32 - 1, r).unwrap();
                    tileset.tiles[tid as usize].sockets[HexDirection::E as usize]
                })
                .collect(),
            GridEdge::West => (0..self.height as i32)
                .map(|r| {
                    let tid = self.get(0, r).unwrap();
                    tileset.tiles[tid as usize].sockets[HexDirection::W as usize]
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum GridEdge {
    North,
    South,
    East,
    West,
}

#[derive(Debug, thiserror::Error)]
pub enum SolveError {
    #[error(
        "contradiction at ({q}, {r}) — no valid tiles remain after {attempts} restart attempts"
    )]
    Contradiction { q: i32, r: i32, attempts: u32 },
    #[error("tile set has {0} tiles, maximum supported is {MAX_TILES}")]
    TooManyTiles(usize),
}

// --- Pre-computed constraint tables ---

/// For each (tile_id, direction), a bitmask of compatible neighbour tiles.
/// `masks[tile_id * 6 + dir]` = bitset of tiles that can be in direction `dir`.
struct ConstraintMasks {
    masks: Vec<u64>,
}

impl ConstraintMasks {
    fn build<T>(tileset: &TileSet<T>) -> Self {
        let n = tileset.tiles.len();
        // masks[tile * 6 + dir] = compatible neighbour bitset
        let mut masks = vec![0u64; n * 6];

        for (tid, tile) in tileset.tiles.iter().enumerate() {
            for dir in HexDirection::ALL {
                let our_socket = tile.sockets[dir as usize];
                let opp = dir.opposite();
                let opp_edge = opp as usize;
                let mut compatible = 0u64;
                for (ntid, ntile) in tileset.tiles.iter().enumerate() {
                    let their_socket = ntile.sockets[opp_edge];
                    if tileset.valid_pairs.contains(&(our_socket, their_socket)) {
                        compatible |= 1u64 << ntid;
                    }
                }
                masks[tid * 6 + dir as usize] = compatible;
            }
        }

        Self { masks }
    }

    /// Bitset of tiles compatible with ANY tile in `domain` in the given direction.
    #[inline]
    fn compatible_in_direction(&self, domain: u64, dir: HexDirection) -> u64 {
        let dir_idx = dir as usize;
        let mut result = 0u64;
        let mut remaining = domain;
        while remaining != 0 {
            let tid = remaining.trailing_zeros() as usize;
            result |= self.masks[tid * 6 + dir_idx];
            remaining &= remaining - 1; // clear lowest set bit
        }
        result
    }
}

// --- Undo log ---

/// Snapshot of a cell's domain before it was modified.
#[derive(Debug, Clone, Copy)]
struct UndoEntry {
    cell_idx: usize,
    old_domain: u64,
}

struct Checkpoint {
    undo_log_len: usize,
    cell_idx: usize,
    guessed_tile: TileId,
}

// --- Solver ---

pub struct Solver<'a, T> {
    tileset: &'a TileSet<T>,
    width: u32,
    height: u32,
    max_restarts: u32,
    max_backtrack_depth: u32,
}

impl<'a, T> Solver<'a, T> {
    pub fn new(tileset: &'a TileSet<T>, width: u32, height: u32) -> Self {
        Self {
            tileset,
            width,
            height,
            max_restarts: 5,
            max_backtrack_depth: 100,
        }
    }

    pub fn max_restarts(mut self, n: u32) -> Self {
        self.max_restarts = n;
        self
    }

    pub fn max_backtrack_depth(mut self, n: u32) -> Self {
        self.max_backtrack_depth = n;
        self
    }

    pub fn solve(
        &self,
        seed: u64,
        pre_collapsed: &[PreCollapsed],
    ) -> Result<SolveResult, SolveError> {
        self.solve_full(seed, pre_collapsed, &[], &[])
    }

    pub fn solve_constrained(
        &self,
        seed: u64,
        edge_constraints: &[EdgeConstraint],
    ) -> Result<SolveResult, SolveError> {
        self.solve_full(seed, &[], edge_constraints, &[])
    }

    pub fn solve_full(
        &self,
        seed: u64,
        pre_collapsed: &[PreCollapsed],
        edge_constraints: &[EdgeConstraint],
        weight_overrides: &[(TileId, f32)],
    ) -> Result<SolveResult, SolveError> {
        let num_tiles = self.tileset.tiles.len();
        if num_tiles > MAX_TILES {
            return Err(SolveError::TooManyTiles(num_tiles));
        }

        let grid_size = (self.width * self.height) as usize;
        let all_bits = if num_tiles == 64 {
            u64::MAX
        } else {
            (1u64 << num_tiles) - 1
        };

        // Pre-compute constraint masks
        let constraints = ConstraintMasks::build(self.tileset);

        // Build effective weights
        let mut weights: Vec<f32> = self.tileset.tiles.iter().map(|t| t.weight).collect();
        for &(tid, w) in weight_overrides {
            if (tid as usize) < weights.len() {
                weights[tid as usize] = w;
            }
        }

        // Pre-compute weight sums and log-weight sums for entropy calculation
        let weight_log: Vec<f64> = weights
            .iter()
            .map(|&w| {
                let w = w as f64;
                if w > 0.0 { w * w.ln() } else { 0.0 }
            })
            .collect();

        for attempt in 0..self.max_restarts {
            let attempt_seed = seed.wrapping_add(attempt as u64 * 1_000_003);
            let mut rng = SmallRng::seed_from_u64(attempt_seed);

            // Initialize all cells with all tiles possible
            let mut domains = vec![all_bits; grid_size];

            // Per-cell cached entropy components for incremental updates
            let mut sum_weights = vec![0.0f64; grid_size];
            let mut sum_weight_log = vec![0.0f64; grid_size];
            let initial_sw: f64 = weights.iter().map(|&w| w as f64).sum();
            let initial_swl: f64 = weight_log.iter().sum();
            for sw in sum_weights.iter_mut() {
                *sw = initial_sw;
            }
            for swl in sum_weight_log.iter_mut() {
                *swl = initial_swl;
            }

            let mut undo_log: Vec<UndoEntry> = Vec::with_capacity(grid_size * 4);
            let mut checkpoints: Vec<Checkpoint> = Vec::new();
            let mut total_backtracks = 0u32;
            let mut propagation_queue: VecDeque<usize> = VecDeque::with_capacity(256);

            // Apply pre-collapsed cells
            for pc in pre_collapsed {
                if let Some(idx) = self.cell_index(pc.q, pc.r) {
                    let new_domain = 1u64 << pc.tile_id;
                    if domains[idx] != new_domain {
                        domains[idx] = new_domain;
                        self.recompute_entropy_cache(
                            idx,
                            domains[idx],
                            &weights,
                            &weight_log,
                            &mut sum_weights,
                            &mut sum_weight_log,
                        );
                        propagation_queue.push_back(idx);
                    }
                }
            }

            // Apply edge constraints
            for ec in edge_constraints {
                let idx = match self.cell_index(ec.q, ec.r) {
                    Some(idx) => idx,
                    None => continue,
                };
                let our_edge = ec.direction as usize;
                // Keep only tiles whose socket on our_edge is compatible with neighbour_socket
                let mut allowed = 0u64;
                for tid in 0..num_tiles {
                    let our_socket = self.tileset.tiles[tid].sockets[our_edge];
                    if self
                        .tileset
                        .valid_pairs
                        .contains(&(our_socket, ec.neighbour_socket))
                    {
                        allowed |= 1u64 << tid;
                    }
                }
                let new_domain = domains[idx] & allowed;
                if new_domain != domains[idx] {
                    domains[idx] = new_domain;
                    self.recompute_entropy_cache(
                        idx,
                        domains[idx],
                        &weights,
                        &weight_log,
                        &mut sum_weights,
                        &mut sum_weight_log,
                    );
                    propagation_queue.push_back(idx);
                }
            }

            if !self.propagate(
                &mut domains,
                &mut propagation_queue,
                &mut undo_log,
                &constraints,
                &weights,
                &weight_log,
                &mut sum_weights,
                &mut sum_weight_log,
            ) {
                continue;
            }

            // Main loop
            let mut iterations = 0u32;
            let mut last_contradiction = None;

            let success = loop {
                let min_cell =
                    self.find_min_entropy(&domains, &sum_weights, &sum_weight_log, &mut rng);

                let Some((cell_idx, _q, _r)) = min_cell else {
                    break true;
                };

                let tile_id = match self.weighted_pick(domains[cell_idx], &weights, &mut rng) {
                    Some(tid) => tid,
                    None => {
                        last_contradiction = Some(self.index_to_qr(cell_idx));
                        break false;
                    }
                };

                // Checkpoint
                checkpoints.push(Checkpoint {
                    undo_log_len: undo_log.len(),
                    cell_idx,
                    guessed_tile: tile_id,
                });

                // Collapse
                let old_domain = domains[cell_idx];
                let new_domain = 1u64 << tile_id;
                if old_domain != new_domain {
                    undo_log.push(UndoEntry {
                        cell_idx,
                        old_domain,
                    });
                    domains[cell_idx] = new_domain;
                    self.recompute_entropy_cache(
                        cell_idx,
                        new_domain,
                        &weights,
                        &weight_log,
                        &mut sum_weights,
                        &mut sum_weight_log,
                    );
                }

                propagation_queue.push_back(cell_idx);
                let ok = self.propagate(
                    &mut domains,
                    &mut propagation_queue,
                    &mut undo_log,
                    &constraints,
                    &weights,
                    &weight_log,
                    &mut sum_weights,
                    &mut sum_weight_log,
                );

                if ok {
                    iterations += 1;
                    continue;
                }

                // Contradiction — backtrack
                let recovered = self.backtrack(
                    &mut domains,
                    &mut undo_log,
                    &mut checkpoints,
                    &mut propagation_queue,
                    &mut total_backtracks,
                    &constraints,
                    &weights,
                    &weight_log,
                    &mut sum_weights,
                    &mut sum_weight_log,
                );

                if !recovered {
                    last_contradiction = Some(self.index_to_qr(cell_idx));
                    break false;
                }

                iterations += 1;
            };

            if success {
                let grid: Vec<TileId> = domains
                    .iter()
                    .map(|&d| d.trailing_zeros() as TileId)
                    .collect();

                return Ok(SolveResult {
                    width: self.width,
                    height: self.height,
                    grid,
                    iterations,
                    backtracks: total_backtracks,
                });
            }

            if attempt + 1 >= self.max_restarts {
                let (q, r) = last_contradiction.unwrap_or((0, 0));
                return Err(SolveError::Contradiction {
                    q,
                    r,
                    attempts: attempt + 1,
                });
            }
        }

        Err(SolveError::Contradiction {
            q: 0,
            r: 0,
            attempts: self.max_restarts,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn backtrack(
        &self,
        domains: &mut [u64],
        undo_log: &mut Vec<UndoEntry>,
        checkpoints: &mut Vec<Checkpoint>,
        propagation_queue: &mut VecDeque<usize>,
        total_backtracks: &mut u32,
        constraints: &ConstraintMasks,
        weights: &[f32],
        weight_log: &[f64],
        sum_weights: &mut [f64],
        sum_weight_log: &mut [f64],
    ) -> bool {
        loop {
            *total_backtracks += 1;
            if *total_backtracks > self.max_backtrack_depth {
                return false;
            }

            let checkpoint = match checkpoints.pop() {
                Some(cp) => cp,
                None => return false,
            };

            // Restore domains from undo log
            while undo_log.len() > checkpoint.undo_log_len {
                let entry = undo_log.pop().unwrap();
                domains[entry.cell_idx] = entry.old_domain;
                self.recompute_entropy_cache(
                    entry.cell_idx,
                    entry.old_domain,
                    weights,
                    weight_log,
                    sum_weights,
                    sum_weight_log,
                );
            }

            // Ban the guessed tile
            let old_domain = domains[checkpoint.cell_idx];
            let new_domain = old_domain & !(1u64 << checkpoint.guessed_tile);
            if new_domain == 0 {
                continue; // No options left, backtrack further
            }
            undo_log.push(UndoEntry {
                cell_idx: checkpoint.cell_idx,
                old_domain,
            });
            domains[checkpoint.cell_idx] = new_domain;
            self.recompute_entropy_cache(
                checkpoint.cell_idx,
                new_domain,
                weights,
                weight_log,
                sum_weights,
                sum_weight_log,
            );

            propagation_queue.clear();
            propagation_queue.push_back(checkpoint.cell_idx);
            if self.propagate(
                domains,
                propagation_queue,
                undo_log,
                constraints,
                weights,
                weight_log,
                sum_weights,
                sum_weight_log,
            ) {
                return true;
            }
        }
    }

    #[inline]
    fn recompute_entropy_cache(
        &self,
        idx: usize,
        domain: u64,
        weights: &[f32],
        weight_log: &[f64],
        sum_weights: &mut [f64],
        sum_weight_log: &mut [f64],
    ) {
        let mut sw = 0.0f64;
        let mut swl = 0.0f64;
        let mut remaining = domain;
        while remaining != 0 {
            let tid = remaining.trailing_zeros() as usize;
            sw += weights[tid] as f64;
            swl += weight_log[tid];
            remaining &= remaining - 1;
        }
        sum_weights[idx] = sw;
        sum_weight_log[idx] = swl;
    }

    #[inline]
    fn cell_index(&self, q: i32, r: i32) -> Option<usize> {
        if q < 0 || r < 0 || q >= self.width as i32 || r >= self.height as i32 {
            return None;
        }
        Some(r as usize * self.width as usize + q as usize)
    }

    #[inline]
    fn index_to_qr(&self, idx: usize) -> (i32, i32) {
        let q = (idx % self.width as usize) as i32;
        let r = (idx / self.width as usize) as i32;
        (q, r)
    }

    /// Get neighbours as a stack-allocated array. Returns (array, count).
    #[inline]
    fn neighbours(&self, q: i32, r: i32) -> ([Neighbour; 6], usize) {
        let is_even_row = r % 2 == 0;
        let offsets: [(i32, i32); 6] = if is_even_row {
            [(0, -1), (1, 0), (0, 1), (-1, 1), (-1, 0), (-1, -1)]
        } else {
            [(1, -1), (1, 0), (1, 1), (0, 1), (-1, 0), (0, -1)]
        };

        let mut result = [Neighbour {
            idx: 0,
            dir: HexDirection::NE,
        }; 6];
        let mut count = 0;

        for (i, &dir) in HexDirection::ALL.iter().enumerate() {
            let nq = q + offsets[i].0;
            let nr = r + offsets[i].1;
            if nq >= 0 && nr >= 0 && nq < self.width as i32 && nr < self.height as i32 {
                result[count] = Neighbour {
                    idx: nr as usize * self.width as usize + nq as usize,
                    dir,
                };
                count += 1;
            }
        }

        (result, count)
    }

    /// AC-3 propagation with bitset domains and pre-computed constraint masks.
    #[allow(clippy::too_many_arguments)]
    fn propagate(
        &self,
        domains: &mut [u64],
        queue: &mut VecDeque<usize>,
        undo_log: &mut Vec<UndoEntry>,
        constraints: &ConstraintMasks,
        weights: &[f32],
        weight_log: &[f64],
        sum_weights: &mut [f64],
        sum_weight_log: &mut [f64],
    ) -> bool {
        while let Some(cell_idx) = queue.pop_front() {
            let (q, r) = self.index_to_qr(cell_idx);
            let (neighbours, ncount) = self.neighbours(q, r);

            for nb in &neighbours[..ncount] {
                let Neighbour {
                    idx: neighbour_idx,
                    dir,
                } = *nb;

                // Bitset of tiles compatible with our domain in this direction
                let allowed = constraints.compatible_in_direction(domains[cell_idx], dir);

                let old_domain = domains[neighbour_idx];
                let new_domain = old_domain & allowed;

                if new_domain == old_domain {
                    continue; // No change
                }

                if new_domain == 0 {
                    return false; // Contradiction
                }

                undo_log.push(UndoEntry {
                    cell_idx: neighbour_idx,
                    old_domain,
                });
                domains[neighbour_idx] = new_domain;
                self.recompute_entropy_cache(
                    neighbour_idx,
                    new_domain,
                    weights,
                    weight_log,
                    sum_weights,
                    sum_weight_log,
                );
                queue.push_back(neighbour_idx);
            }
        }

        true
    }

    /// Find uncollapsed cell with minimum entropy.
    /// Uses cached sum_weights/sum_weight_log for O(1) entropy per cell.
    fn find_min_entropy(
        &self,
        domains: &[u64],
        sum_weights: &[f64],
        sum_weight_log: &[f64],
        rng: &mut SmallRng,
    ) -> Option<(usize, i32, i32)> {
        let mut best_idx: Option<usize> = None;
        let mut best_entropy = f64::MAX;

        for (idx, &domain) in domains.iter().enumerate() {
            let count = domain.count_ones();
            if count <= 1 {
                continue;
            }

            let sw = sum_weights[idx];
            if sw <= 0.0 {
                continue;
            }

            // Shannon entropy from cached components:
            // H = log(sum_w) - sum(w*log(w)) / sum_w
            let entropy = sw.ln() - sum_weight_log[idx] / sw;

            let noise = rng.random::<f64>() * 0.001;
            let effective = entropy + noise;

            if effective < best_entropy {
                best_entropy = effective;
                best_idx = Some(idx);
            }
        }

        best_idx.map(|idx| {
            let (q, r) = self.index_to_qr(idx);
            (idx, q, r)
        })
    }

    fn weighted_pick(&self, domain: u64, weights: &[f32], rng: &mut SmallRng) -> Option<TileId> {
        if domain == 0 {
            return None;
        }

        let mut total = 0.0f64;
        let mut remaining = domain;
        while remaining != 0 {
            let tid = remaining.trailing_zeros() as usize;
            total += weights[tid] as f64;
            remaining &= remaining - 1;
        }

        if total <= 0.0 {
            return None;
        }

        let mut roll = rng.random::<f64>() * total;
        let mut remaining = domain;
        while remaining != 0 {
            let tid = remaining.trailing_zeros() as usize;
            roll -= weights[tid] as f64;
            if roll <= 0.0 {
                return Some(tid as TileId);
            }
            remaining &= remaining - 1;
        }

        // Fallback: highest set bit
        Some((63 - domain.leading_zeros()) as TileId)
    }
}

#[derive(Clone, Copy)]
struct Neighbour {
    idx: usize,
    dir: HexDirection,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_tileset() -> TileSet<String> {
        let toml_str =
            std::fs::read_to_string("tests/fixtures/ocean_islands.toml").expect("tile set file");
        TileSet::from_toml(&toml_str).expect("valid tile set")
    }

    #[test]
    fn solve_small_grid() {
        let ts = load_tileset();
        let solver = Solver::new(&ts, 8, 8);
        let result = solver.solve(42, &[]).expect("should solve 8x8");
        assert_eq!(result.grid.len(), 64);
        assert!(result.iterations > 0);
    }

    #[test]
    fn solve_with_pre_collapsed_edges() {
        let ts = load_tileset();

        let deep_ocean_id = ts
            .tiles
            .iter()
            .find(|t| t.source_id == "deep_ocean")
            .unwrap()
            .id;

        let pre: Vec<PreCollapsed> = (0..16)
            .map(|q| PreCollapsed {
                q,
                r: 0,
                tile_id: deep_ocean_id,
            })
            .collect();

        let solver = Solver::new(&ts, 16, 16);
        let result = solver
            .solve(123, &pre)
            .expect("should solve with edge constraints");

        for q in 0..16 {
            assert_eq!(result.get(q, 0), Some(deep_ocean_id));
        }
    }

    #[test]
    fn solve_full_chunk() {
        let ts = load_tileset();
        let solver = Solver::new(&ts, 96, 96).max_backtrack_depth(500);
        let result = solver
            .solve(999, &[])
            .expect("should solve 96x96 with backtracking");
        assert_eq!(result.grid.len(), 96 * 96);

        for &tid in &result.grid {
            assert!(
                (tid as usize) < ts.tiles.len(),
                "tile ID {tid} out of range"
            );
        }

        eprintln!(
            "96x96 solved: {} iterations, {} backtracks",
            result.iterations, result.backtracks
        );
    }

    #[test]
    fn edge_manifests_are_correct_length() {
        let ts = load_tileset();
        let solver = Solver::new(&ts, 16, 16);
        let result = solver.solve(42, &[]).unwrap();

        assert_eq!(result.edge_manifest(GridEdge::North, &ts).len(), 16);
        assert_eq!(result.edge_manifest(GridEdge::South, &ts).len(), 16);
        assert_eq!(result.edge_manifest(GridEdge::East, &ts).len(), 16);
        assert_eq!(result.edge_manifest(GridEdge::West, &ts).len(), 16);
    }

    #[test]
    fn adjacent_chunks_share_compatible_edges() {
        let ts = load_tileset();
        let solver = Solver::new(&ts, 16, 16);

        let chunk_a = solver.solve(100, &[]).unwrap();
        let east_manifest = chunk_a.edge_manifest(GridEdge::East, &ts);

        let edge_constraints: Vec<EdgeConstraint> = east_manifest
            .iter()
            .enumerate()
            .map(|(r, &socket)| EdgeConstraint {
                q: 0,
                r: r as i32,
                direction: HexDirection::W,
                neighbour_socket: socket,
            })
            .collect();

        let chunk_b = solver.solve_constrained(200, &edge_constraints).unwrap();

        let west_manifest = chunk_b.edge_manifest(GridEdge::West, &ts);
        for (i, (&a_socket, &b_socket)) in
            east_manifest.iter().zip(west_manifest.iter()).enumerate()
        {
            assert!(
                ts.valid_pairs.contains(&(a_socket, b_socket)),
                "edge mismatch at row {i}: socket {a_socket} vs {b_socket}"
            );
        }
    }
}
