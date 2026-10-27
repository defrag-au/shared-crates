# Agent instructions — sceneworld

World document + a **window**. Hand dumps use a Chebyshev square. A
Hilbert **palace** (`order=`) emits a *stretch along the curve* (the
regl winding corridor), not a square of rooms. [`World::emit`] writes
that stretch as [`scenegraph::Scene`] (`sg1`).

## Commands

```sh
cargo test -p sceneworld
cargo clippy -p sceneworld --all-targets
```

## Invariants

1. **`sw1` is the score.** A world dump round-trips. `emit()` is valid
   `sg1` (round-trips through `Scene::from_dump`). Screenshots are not
   acceptance. `span=0 at=0,0` of `tests/golden/gallery.sw1` dumps equal
   to `scenegraph` `gallery.sg1`.
2. **Closed statements.** `hang` is the only body line. Walls are
   `n|e|s|w`. Unknown keys/walls are load errors. `m=` is an opaque
   image key — no `AssetId`, no IIIF in this crate.
3. **Window.** Hand dumps: Chebyshev `max(|dx|,|dz|) ≤ span`. Palace
   (`order=`): Hilbert index `|i − i_at| ≤ span` — a winding stretch
   (`span=24` matches regl `renderDist≈20`). A Chebyshev square fills
   in every nearby cell and looks like a grid; do not emit that for a
   palace. Floor/ceil cover the whole wing and do not resize on page.
   Stable `c{x}_{z}` names.
4. **No `Kind::Gallery`.** A cell is `Group` + `Quad` walls + `Mesh Slab`
   hangs (`rad=0` 1:1 Unlit) + Labels on the print. Same kinds as the
   rest of the graph.
5. **Rust is the hook.** `World::new("gallery").push(Hang::new(…))` is
   the same document as the dump. No expressions in `sw1`.
6. **Camera is cursor-local.** `emit` writes `cam` in the cursor cell's
   frame. A pane that re-emits after a walk uses [`transplant_cam`] so
   the live eye/at survive the slice swap.
7. **Walls, not windows.** Hand dumps may still cut jambs. **Palace
   (`order=`):** an opening is a *missing* wall (the regl palace), not a
   cut-out. Solid shared faces are drawn once (west/north owner) so the
   GPU does not z-fight two coplanar quads. Corridor / shortcut still
   choose which faces open. One floor and one ceiling cover the whole
   wing (they do not resize on page). No per-cell plinths or extra lamps.
8. **Hilbert generates map and prints.** `order=` + `print m= title=`
   is the document; hangs are `p-{hilbertIndex*4+wall}` cycling the
   catalog. Consecutive cells are cardinal neighbours. `fill_hilbert`
   still expands explicit hangs for tests. `should_page` waits until
   the Hilbert index has moved by `span/2`. Pane must not restart
   IIIF on page — overlapping `p-{i}` stay interned.
9. **Raster culls.** `DrawList` stays complete (hit needs walls behind
   you). The GPU frustum-culls quads/meshes/labels. Flatten cost is
   [`DrawList::census`] and scene-view `perf` (flatten n=, present,
   submit after cull). `cargo test -p sceneworld --test profile -- --nocapture`.

## Dump grammar (`sw1`)

```
sw1
gallery
hang c=0,0 wall=n name=p-toolhead m=policy:hex title=Toolhead0220
hang c=1,0 wall=n name=p-next m=demo:next title=Next
```

- Line 1: `sw1`
- Line 2: `NAME [cell=8] [span=0] [at=0,0] [order=] [seed=0]` — defaults omitted
- Then `print m= [title=]` (catalog, palace) and/or
  `hang c=x,z wall=n|e|s|w name= m= [title=]`
- `c=` is integer cell coords (+X east, +Z south). Origin cell is
  centred on world origin; cell size `cell=` (default 8).
- `title=` is dump-safe (no spaces). Omitted: no Label.
- Duplicate `name=` or duplicate `(c, wall)` is a load error.

Goldens: `tests/golden/gallery.sw1` (hand one-cell + neighbour);
`tests/golden/hilbert-4.sw1` (explicit fill, four walls of the origin);
`tests/golden/palace.sw1` (order-6 generator, three catalog prints).
