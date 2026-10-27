# sceneworld

World document for [`scenegraph`](../scenegraph). A Hilbert palace
(`order=` + print catalog) generates the maze and the art;
[`World::emit`] writes the Chebyshev window as `sg1`. Doors follow the
curve (plus seeded shortcuts) so line of sight dies at the next turn —
the window rim is not a visible cutoff. The graph never holds the wing.

```sh
cargo test -p sceneworld
```

```rust
let mut world = World::from_dump(include_str!("tests/golden/palace.sw1"))?;
let scene = world.emit()?;            // 9×9 of a 64×64 maze
if world.should_page(player_cell) {
    world.at = player_cell;
    let mut next = world.emit()?;
    transplant_cam(&scene, &mut next); // keep cam; keep interned prints
}
```

Dump grammar: `AGENTS.md`.
