use rand::rngs::SmallRng;
use rand::SeedableRng;
use wfc::{Direction, SolveResult, Tileset, WfcSolver};

/// Simple 2-tile test: grass and water. Both self-adjacent.
#[test]
fn test_uniform_tiles() {
    let mut tileset = Tileset::new();
    let grass = tileset.add_tile("grass");
    tileset.set_edges(grass, ["g", "g", "g", "g"]);

    let mut rng = SmallRng::seed_from_u64(42);
    let solver = WfcSolver::new(8, 8, &tileset);

    match solver.solve(&mut tileset, &mut rng) {
        SolveResult::Solved(grid) => {
            assert_eq!(grid.width, 8);
            assert_eq!(grid.height, 8);
            // With only one tile, every cell must be grass
            for (_, _, tile) in grid.iter() {
                assert_eq!(tile, grass);
            }
        }
        SolveResult::Contradiction(_) => panic!("Should not contradict with single tile"),
    }
}

/// Road network test: grass, horizontal road, vertical road, crossroads.
/// Edges must match: road ends connect to road ends, grass to grass.
#[test]
fn test_road_network() {
    let mut tileset = Tileset::new();

    //  Edge labels:
    //  "g" = grass edge
    //  "r" = road edge
    //
    //  Tiles:
    //  grass:      g/g/g/g  — all grass edges
    //  road_h:     g/r/g/r  — horizontal road (N=grass, E=road, S=grass, W=road)
    //  road_v:     r/g/r/g  — vertical road (N=road, E=grass, S=road, W=grass)
    //  crossroads: r/r/r/r  — all road edges

    let grass = tileset.add_tile_weighted("grass", 5.0);
    let road_h = tileset.add_tile("road_h");
    let road_v = tileset.add_tile("road_v");
    let cross = tileset.add_tile("crossroads");

    tileset.set_edges(grass, ["g", "g", "g", "g"]);
    tileset.set_edges(road_h, ["g", "r", "g", "r"]);
    tileset.set_edges(road_v, ["r", "g", "r", "g"]);
    tileset.set_edges(cross, ["r", "r", "r", "r"]);

    let mut rng = SmallRng::seed_from_u64(123);
    let solver = WfcSolver::new(12, 12, &tileset);

    match solver.solve(&mut tileset, &mut rng) {
        SolveResult::Solved(grid) => {
            // Verify adjacency constraints
            for row in 0..grid.height {
                for col in 0..grid.width {
                    let tile = grid.get(col, row);
                    let tile_def = tileset.tile(tile);

                    // Check east neighbor
                    if col + 1 < grid.width {
                        let east_tile = grid.get(col + 1, row);
                        let east_def = tileset.tile(east_tile);
                        assert_eq!(
                            tile_def.edges[Direction::East.index()],
                            east_def.edges[Direction::West.index()],
                            "Edge mismatch at ({col},{row}) E ↔ ({},{row}) W: {} vs {}",
                            col + 1, tile_def.name, east_def.name
                        );
                    }

                    // Check south neighbor
                    if row + 1 < grid.height {
                        let south_tile = grid.get(col, row + 1);
                        let south_def = tileset.tile(south_tile);
                        assert_eq!(
                            tile_def.edges[Direction::South.index()],
                            south_def.edges[Direction::North.index()],
                            "Edge mismatch at ({col},{row}) S ↔ ({col},{}) N: {} vs {}",
                            row + 1, tile_def.name, south_def.name
                        );
                    }
                }
            }

            // Print the grid for visual inspection
            eprintln!("\nRoad network (12x12):");
            for row in 0..grid.height {
                let line: String = (0..grid.width).map(|col| {
                    match tileset.tile(grid.get(col, row)).name.as_str() {
                        "grass" => '.',
                        "road_h" => '─',
                        "road_v" => '│',
                        "crossroads" => '┼',
                        _ => '?',
                    }
                }).collect();
                eprintln!("  {line}");
            }
        }
        SolveResult::Contradiction(n) => {
            panic!("Contradicted after {n} cells");
        }
    }
}

/// Test with pinned cells — fix some tiles and let WFC fill the rest.
#[test]
fn test_pinned_cells() {
    let mut tileset = Tileset::new();

    let grass = tileset.add_tile_weighted("grass", 5.0);
    let road_h = tileset.add_tile("road_h");
    let road_v = tileset.add_tile("road_v");
    let cross = tileset.add_tile("crossroads");

    tileset.set_edges(grass, ["g", "g", "g", "g"]);
    tileset.set_edges(road_h, ["g", "r", "g", "r"]);
    tileset.set_edges(road_v, ["r", "g", "r", "g"]);
    tileset.set_edges(cross, ["r", "r", "r", "r"]);

    let mut solver = WfcSolver::new(8, 8, &tileset);

    // Pin a crossroads in the center
    solver.pin(4, 4, cross, &mut tileset);

    let mut rng = SmallRng::seed_from_u64(42);
    match solver.solve(&mut tileset, &mut rng) {
        SolveResult::Solved(grid) => {
            // The pinned cell must be crossroads
            assert_eq!(grid.get(4, 4), cross);

            // Its neighbors must have matching road edges
            // North of crossroads (4,3) must have road on South edge
            let north = grid.get(4, 3);
            let north_def = tileset.tile(north);
            assert_eq!(
                north_def.edges[Direction::South.index()],
                tileset.tile(cross).edges[Direction::North.index()],
                "North of pinned crossroads must have road on south edge"
            );

            eprintln!("\nPinned crossroads (8x8):");
            for row in 0..grid.height {
                let line: String = (0..grid.width).map(|col| {
                    match tileset.tile(grid.get(col, row)).name.as_str() {
                        "grass" => '.',
                        "road_h" => '─',
                        "road_v" => '│',
                        "crossroads" => '┼',
                        _ => '?',
                    }
                }).collect();
                eprintln!("  {line}");
            }
        }
        SolveResult::Contradiction(n) => {
            panic!("Contradicted after {n} cells");
        }
    }
}

/// Test determinism: same seed produces same output.
#[test]
fn test_deterministic() {
    let mut tileset = Tileset::new();
    let grass = tileset.add_tile_weighted("grass", 5.0);
    let road_h = tileset.add_tile("road_h");
    let road_v = tileset.add_tile("road_v");

    tileset.set_edges(grass, ["g", "g", "g", "g"]);
    tileset.set_edges(road_h, ["g", "r", "g", "r"]);
    tileset.set_edges(road_v, ["r", "g", "r", "g"]);

    let solve_with_seed = |seed: u64| -> Vec<u16> {
        let mut ts = tileset.clone();
        let mut rng = SmallRng::seed_from_u64(seed);
        let solver = WfcSolver::new(6, 6, &ts);
        match solver.solve(&mut ts, &mut rng) {
            SolveResult::Solved(grid) => grid.as_slice().iter().map(|t| t.0).collect(),
            SolveResult::Contradiction(_) => panic!("Should not contradict"),
        }
    };

    let result_a = solve_with_seed(42);
    let result_b = solve_with_seed(42);
    let result_c = solve_with_seed(99);

    assert_eq!(result_a, result_b, "Same seed must produce same output");
    // Different seeds should (almost certainly) produce different output
    assert_ne!(result_a, result_c, "Different seeds should produce different output");
}

/// Larger grid stress test.
#[test]
fn test_large_grid() {
    let mut tileset = Tileset::new();
    let grass = tileset.add_tile_weighted("grass", 8.0);
    let road_h = tileset.add_tile("road_h");
    let road_v = tileset.add_tile("road_v");
    let cross = tileset.add_tile("crossroads");

    tileset.set_edges(grass, ["g", "g", "g", "g"]);
    tileset.set_edges(road_h, ["g", "r", "g", "r"]);
    tileset.set_edges(road_v, ["r", "g", "r", "g"]);
    tileset.set_edges(cross, ["r", "r", "r", "r"]);

    let mut rng = SmallRng::seed_from_u64(42);
    let solver = WfcSolver::new(40, 40, &tileset);

    match solver.solve(&mut tileset, &mut rng) {
        SolveResult::Solved(grid) => {
            let total = grid.width * grid.height;
            let road_count = grid.as_slice().iter().filter(|t| t.0 != grass.0).count();
            eprintln!("40x40 grid: {road_count}/{total} road cells ({:.1}%)", road_count as f64 / total as f64 * 100.0);
            assert!(total == 1600);
        }
        SolveResult::Contradiction(n) => {
            panic!("Contradicted after {n}/1600 cells on 40x40 grid");
        }
    }
}
