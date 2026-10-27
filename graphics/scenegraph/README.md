# scenegraph

Named scene graph. The document is the frame — `dump()` / `from_dump()`
are the agent loop (dump → pose → hit → dump). That loop is the gauntlet:
textual, replayable. GPU backends only draw a [`DrawList`]. Raster is for
humans; dumps are the score. Interactives are hits on named nodes, not
widget kinds.

```sh
cargo test -p scenegraph
cargo run -p scene-view          # dump + raster of the goldens
cargo run -p scene-view -- --plan PLAN.json --assets DIR
                                 # compositor job → sg1 (orbit, pick through alpha)
cargo run -p scene-view -- --plan PLAN.json --assets DIR --showcase
                                 # pose showcase clip: explode, then 3/4 camera
cargo run -p scene-view -- --gltf FILE.glb
                                 # glTF node tree as parented sg1 (orbit, pick)
```

Features: `macroquad` (miniquad draw: pure `mq::plan`, cached
`mq::Renderer`, CPU per-vertex Lambert, GLSL ES 100 alpha-discard and MSDF
programs), `wgpu` (forward raster; CPU bytes for tests only). Default is
pure. The viewer shares eframe's wgpu device and presents a native texture
— never GPU→CPU→GPU. Labels pack an MSDF atlas (`Fonts`); both backends
take the median of RGB. Both draw with `DrawList::view_proj`, the pick
matrix. `cargo test -p scenegraph --features macroquad,wgpu` pins parity
headless: same triangles per texture key (`SubmitCensus`), same matrix, a
pick projects back to its ndc.

See `AGENTS.md` for the dump grammar. Angles are radians. Kinds stay
general (`Group Quad Camera Light Mesh Label`); lamps are `Dir|Point`;
mesh prims are `Box|Plane|Geom|Slab|Sphere`. Surfaces are `Lambert|Unlit`
(Lambert omitted). The combat fixture is a frontal camera on an XY
layout (2.5D): portraits and a fanned hand of `Mesh Slab`, not a
deckbuilder kind. Zero-width quads are planes, not cards. Image keys (`m=` / `mb=`) resolve
through [`Images`]; geom keys (`g=`) through [`Geometry`]; material keys
(`mat=`) through [`Materials`] (`mt1`). Triangle blobs stay off-dump
(`Geometry::dump` is a census). Shader *programs* stay in backends — the
document never contains WGSL. `DrawList::hit` picks a named quad or mesh.
`--features gltf` uses the `gltf` crate `import` for buffers and
PNG/JPEG, maps PBR base-color onto [`Materials`] / [`Images`], and
walks the node tree into a parented [`Scene`] (`scene_from_gltf`).
glTF is not the runtime document.

Pinned scenes: `tests/golden/room.sg1`, `tests/golden/cards.sg1`,
`tests/golden/surfaces.mt1`, `tests/golden/table.sg1` / `table.mt1`,
`tests/golden/gallery.sg1` / `gallery.mt1` (span=0 emit of
[`sceneworld`](../sceneworld) `gallery.sw1`; the viewer fills prints
from IIIF 400/1646 without changing the dump keys).
Clips live in [`sceneanim`](../sceneanim) — `pose(scene, clip, t)` writes a
posed copy; the graph itself has no playhead. Pointer session lives in
[`sceneinput`](../sceneinput) — `step` writes selection on the session and
orbit/drag/walk on a new rest scene.

A fulfilled CNFT job is a [`Stack`] of Unlit `Quad`s (tiny Z, alpha
pick). Motion patterns live in [`scenedrv`](../scenedrv) (`drv1` →
`an1`). A palace is [`sceneworld`](../sceneworld) (`sw1` → window `sg1`).
Neighbouring engines (ship compositor, hodlcroft gen, Hilbert) stay
outside the graph. See [`BOUNDARY.md`](BOUNDARY.md).
