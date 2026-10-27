# Agent instructions — scenegraph

Named scene graph. The document is the frame: if a draw call can exist
outside the graph, stop. Read the dump grammar before editing a scene.

The family, in `graphics/`: this crate (pure), `sceneanim` (clips),
`sceneinput` (pointer), `scenedrv` (drivers), `sceneworld` (paged worlds),
`sceneapp` (dump folders, replay, gauntlet), and the edge crates
`scenegraph-mq` (macroquad), `scenegraph-wgpu` (wgpu, egui) and
`scene-pane` (a scene on a macroquad-gamekit stage). Games, their registry,
the `scene-gauntlet` binary and the clients live with the app that ships
them (augminted-bots: `scenegames`, `scene-mq`, `scene-play`, `scene-view`,
and the `scene-dump` authoring skill).

## Commands

```sh
cargo test -p scenegraph
cargo test -p scenegraph --features gltf   # glTF importer: models, rigs, clips (Khronos fixtures)
cargo test -p scenegraph-mq                # backend parity with scenegraph-wgpu, headless
cargo test -p scenegraph-wgpu              # GPU raster; skips without an adapter
cargo test -p sceneapp                     # fixture folders scored without a game
cargo test -p sceneworld --test profile -- --nocapture   # flatten census
cargo clippy -p scenegraph --all-targets --all-features -- -D warnings
cargo check --target wasm32-unknown-unknown -p scenegraph --features gltf -p scenegraph-mq
```

This crate is pure; the renderers are edge crates over the same
[`DrawList`]. Do not mix draw APIs. Both project with
`DrawList::view_proj` (the `hit` matrix; `scenegraph_mq::view_proj_glam`).
A macroquad client holds one `scenegraph_mq::Renderer` and calls
`draw_list_to` into a depth target (`sample_count: 0`, drawn with
`flip_y`). A change to what either backend submits must keep
`scenegraph_mq::plan(..).census()` equal to `scenegraph_wgpu::census`
([`SubmitCensus`]: triangles per texture key and sdf); `scenegraph-mq`'s
parity tests pin it. This crate, with `gltf`, must run on
`wasm32-unknown-unknown`: no wall clock (`Instant::now` panics there;
flatten timing goes through `Stopwatch`, which reads 0), no I/O, no
wasm-bindgen. The stock face is [`NOTO_SANS_BOLD`] (OFL,
`assets/NotoSans-OFL.txt`); tests use it rather than a path.

## Invariants

1. **Names are identity.** Public API is string names (pack/art ids). Never
   address by index. Duplicate insert is an error.
2. **Kinds are closed.** `Group | Quad | Camera | Light | Mesh | Label`. Mesh
   prims are closed (`Box | Plane | Geom | Slab | Sphere`). Light lamps are
   closed (`Dir | Point`). Surfaces are closed (`Lambert | Unlit`, default
   Lambert; Label defaults Unlit). Unknown dump keys/kinds/prims/lamps/shades
   are load errors. Geom verts live in [`Geometry`], not in the dump. Named
   surfaces live in [`Materials`] (`mt1`). Faces live in [`Fonts`] (TTF
   bytes + MSDF atlas); the graph stores a font *key*. Card-like fixtures are
   `Mesh Slab` (thickness + rounded corners), not zero-width `Quad`. Quoted
   strings are the one dump-grammar extension and only legal on Label.
3. **Parents exist.** Inserting with a missing parent is an error. Removing
   a node reparents its children to the removed node's parent.
4. **No cycles.** `set_parent` that would cycle is an error.
5. **Dump is the score.** The tight loop is dump → pose → hit → dump.
   That *is* the gauntlet: a battery of textual assertions, not a
   screenshot. `scene-view` raster is a human pane — looking at pixels is
   not acceptance and cannot be replayed. A change that could move a
   scene, a pose, or a pick has a before/after dump (`sg1` / `an1` /
   `mt1` / draw / a parent-local snippet). Flatten cost is
   `DrawList::census()` (`quads=` `meshes=` `mesh_verts=` `glyphs=`).
   scene-view writes a `perf` dump (fps from present-to-present wall
   time, pose/flatten n=/present, flatten `cost` matrix/prims/labels,
   GPU `submit` after cull) — that is how we tune the renderer.
   Walk/orbit only retarget `DrawList` camera (`flatten n=0`); a world
   edit rebuilds. GPU submit is the same: `geo=cached` reuses vertex
   buffers and writes the camera uniform; `geo=rebuild` on flatten.
   `dump()` omits identity
   transforms, default colours, default camera up/fov, default light
   intensity, default Lambert. Round-trip through `from_dump` must
   preserve observable state.
   A Label parented to a `Quad` or `Mesh Slab` is on the print iff its
   whole layout box, in the parent's local frame, is inside `±w/2 × ±h/2`
   and in front of the face (`z > 0` for a Quad, `z > d/2` for a Slab).
   `problems::problems` checks it (`off-print`), with `missing`,
   `off-camera`, `small`, `z-fight` and `covered`, from the document alone;
   the gauntlet leads its dump with them and fails on any. Read that
   *before* any raster.
6. **Queries, not walks.** `get`, `tagged`, `kind`, `DrawList::hit`. Don't
   hand-roll tree walks in callers.
7. **No playhead in the graph.** Animation is the sibling crate `sceneanim`
   (`pose(scene, clip, t) → Scene`). The scene is rest pose; a clip writes
   a new scene. Do not add time, tracks, or mixers to `scenegraph`.
   Live camera is the pane (`put_camera`): rest `cam` is the starting pose.
   Motion patterns (`fan`, `orbit`, `to`) live in `scenedrv`. Inspect
   flavor is `inspect(scene, name, text)` — clone + insert, then pose.
   Pane and gauntlet share `present(rest, mixer, flavor, underlay)`.
   Several clips overlay with `pose_layers`; inspect is a mixer chain
   (lift, then fade). Reverse fades text out before the card returns.
8. **No shader source in the graph.** `sh=` / [`Shade`] selects a closed
   model. GPU programs live in backends (`scenegraph-wgpu` WGSL, `scenegraph-mq` GLSL ES
   100: alpha discard at 0.031, MSDF median with a fixed band — no
   `fwidth` on WebGL1). Lambert is both backends: per-fragment on wgpu,
   CPU per-vertex in `mq::plan`. Custom looks belong to an app backend,
   not `sg1`.
9. **Raster is a preview.** Resolution, MSAA, and LOD are backend knobs.
   They must not change `sg1`, `DrawList::dump()`, or `hit`. Gallery
   prints intern 400px IIIF at distance and 1646px when close — same
   `m=` key, GPU re-uploads; the dump does not name the derivative. A cheap
   orbit preview and a sharp rest frame are the same document.
   Determinism lives in the dump; performance lives in the pane. Do not
   put a pixel budget in `sg1` to “go faster.”
   **Antipattern: GPU → CPU readback → GPU to show a pane.** The graph
   is already a forward projection. Interactive viewers share the window
   device (`Gpu::from_device`) and `present_list` into a native egui
   texture. `rasterize` (bytes) is for tests. A second wgpu adapter plus
   `copy_texture_to_buffer` is how a 3-card hand stalls a 4K window.
10. **Interactives are not kinds.** No Button, Slider, Widget, Gizmo node.
   A control is a named, tagged drawable. `DrawList::hit` (name, face, uv,
   point) is the input primitive. The sibling crate `sceneinput`
   (`step(scene, list, session, event)`) maps pointer events to dumps:
   pick writes the session (`in1`); orbit/drag write rest pose. Test with
   ndc + expected dump, not a screenshot. 2D chrome stays outside the graph.

## Dump grammar (`sg1`)

```
sg1
room Group
floor Quad 4x4 p=room r=-1.5708,0,0
crate Mesh Box p=room t=0,0.4,0 s=0.6,0.8,0.6
orb Mesh Sphere p=room t=1.15,0.5,-0.55
a Mesh Slab 2.2x3.1 mat=print-a
title Label font=ibm size=0.22 "Shredder"
cam Camera eye=3.2,2.4,4.2 at=0,0.6,0
sun Light Dir dir=-0.4,-1,-0.5
```

- Line 1: `sg1`
- Then `NAME KIND [WxH | Box|Plane|Geom|Slab|Sphere | Dir|Point] [k=v ...] ["text"]`
- Keys: `p` parent, `t` `r` `s` (x,y,z), `m` image key, `g` geom key,
  `c` rgb/rgba hex, `tag` (repeatable), `mb` slab back image, `sh` shade,
  `mat` material key, `font` `size` (Label; `size` is em height in world
  units), `eye` `at` `up` `fov` `active` (Camera), `dir` `i`
  `range` (Light), `d` `rad` (Slab depth / corner radius). Default slab
  depth 0.05, radius 0.12, omitted.
- Label: `font=key size=em "text"`. Quotes are required; `\"` `\\` `\n`
  inside. Default Unlit (omitted). Glyphs pack into the named face's MSDF
  atlas at flatten (median of RGB at sample; layout dump is unchanged).
  `Fonts::layout` dump (`glyphs=` `bbox=` `missing=`) is the score — not a
  screenshot. Missing codepoints draw `.notdef` and dump `missing=U+XXXX`
  (not silent tofu, not a load error). Fallback faces are listed on insert
  (`fb=` in `Fonts::dump()`). v1 face is NotoSans-Bold (Latin / Greek /
  Cyrillic). CJK and colour emoji are out of v1.
- Surface: `sh=Unlit|Lambert` (Lambert omitted). `mat=name` resolves through
  [`Materials`] at flatten; mix with `c`/`m`/`mb`/`sh` is a load error.
  Inline `c`/`m`/`mb`/`sh` is the one-off path. Missing image keys draw
  white. Unknown `mat=` is a flatten error.
- `m=` is the front atlas key; `mb=` is the slab back print. A key that
  parses as a Cardano `AssetId` (`policy:hex`) is an **asset print**:
  still a `Mesh Slab` (or Quad), not a new kind. Rest is **1:1** with
  `rad=0` (sharp corners). The pane IIIF-loads 400 then 1646 under that
  same key; LOD does not change the dump. Pixel aspect may write `w`×`h`
  (longest side held) so a non-square print is not letterboxed.
  `g=` is
  resolved by [`Geometry`] at flatten; missing keys are load errors.
  `Geometry::dump()` is a census (`tet 4f 12v bbox=…`), not vertex soup.
  `Geometry::insert_gltf` (`--features gltf`) fills that store.
  The `gltf` crate `import` loads buffers and PNG/JPEG; we walk the
  **node tree** into `Group` / `Mesh Geom` with rest `t/r/s` (radians)
  and `mat=` (PBR base-color → `mt1` / `Images`). A perspective camera
  bakes to world `eye`/`at`. glTF is not `sg1` — verts stay interned;
  dump names and transforms.
  **Models** are the other way in, and the one dump folders use:
  `model_from_gltf(key, bytes)` returns one [`ModelPart`] per surface
  (node transforms baked, normals through the inverse transpose, quaternions
  via a matrix decomposition that survives gimbal lock), split on whole
  triangles under 65,535 vertices, and images keyed `KEY/NAME`.
  `Geometry::insert_model(key, parts)` replaces a blob of that key; `g=KEY`
  then draws every part under the node's one name, so it picks, tags and
  animates as one. A part keeps its own surface unless the node binds one
  (`c=`, `m=`), which repaints them all. glTF `REPEAT` UVs are moved into
  `[0,1]²` per triangle at import (both backends clamp); a triangle that
  spans tiles cannot be, and counts as `tiled=N` in the census line.
  **Rigs.** A file with skins or animations imports rigged
  (`ImportedModel::rig`, `Geometry::insert_imported`): every node of its
  scene is a [`rig::RigNode`]; a skinned vertex is bound to its skin's
  joints, any other wholly to its own node, so rigid parts an animation
  moves (the truck's wheels) pose the same way. The file's animations are
  [`rig::RigClip`]s by name (T/R/S; morph weights skipped; cubic splines
  played linearly through their values). `pose=CLIP@T` on the node
  ([`Pose`]) is skinned on the CPU at flatten, so picks, checks and both
  backends see the posed mesh; without it, the parts are the rig at rest.
  An unknown clip is `Error::UnknownPose`. The census line adds
  `rig=NODES clips=NAME:LEN,…`. `rig` is pure: no glTF, no time, no GPU.
  `front=+x|-x|+z|-z` on a `Mesh Geom` ([`Front`]) declares which local
  axis it faces; `problems::headings(before, after)` judges travel by it.
  [`sketch`] casts a grid of the same pick rays a tap uses and dumps the
  frame as text: the gauntlet's answer to "what would a screenshot show". `DrawList::aim_point(scene, name)` is
  the centre of a mesh node's bounds (else its origin), which `pick node=`,
  the panes and the in-focus judge aim at. The wgpu index buffer is `u32`
  (one buffer for the scene); macroquad splits each mesh into draws under
  its per-call cap, so a part's own `u16` indices are the only limit.
  GPU uses the first 4 Dir and first 4 Point lights.
- `Mesh Sphere` is a unit UV sphere, origin centre, radius 0.5 (same
  extents as Box). Tessellation is closed. Hit face is `Front`.
- `DrawList::hit` / `hit_ndc` return the closest named **geometry**
  (quad, mesh, or label bbox), with `point`, surface `uv`, and
  `HitFace` (`Front` / `Back` / `Edge`). Slab faces are local ±Z and the
  rim, not triangle winding. Picking does not mutate the scene dump.
  `hit_with` / `hit_ndc_with` sample interned [`Images`]: Front uses
  `m=`, Back uses `mb=` only (omitted `mb=` is unprinted — never a
  flipped `m=`), Edge is solid, labels stay bbox.
  Texels at or below [`ALPHA_CUTOFF`] (8) are skipped and the ray
  continues — a transparent hat rim selects `skin`. Missing keys are
  opaque, so a dump without pixels still picks. The GPU discards the
  same cutoff so raster holes match pick.
- `active=1` on a Camera selects it for flatten. Omitted: first Camera in
  visit order.
- Point light position is the node world transform; `range` defaults to 6.
- Golden scenes: `tests/golden/room.sg1` (classic 3D + sphere),
  `tests/golden/cards.sg1` (felt + stock slabs — same kinds, not a graph
  specialisation), `tests/golden/surfaces.mt1` (stripe prints, Unlit),
  `tests/golden/table.sg1` + `table.mt1` (2.5D combat fixture: frontal
  camera, fan of `Mesh Slab` + Labels, portraits, piles, end-turn — same
  kinds, not a graph specialisation), `tests/golden/gallery.sg1` +
  `gallery.mt1` (span=0 emit of `sceneworld` `gallery.sw1`: floor/walls,
  three `tag=hang` asset slabs with `m=policy:hex`, labels on the print,
  a plinth — same kinds, not a gallery kind). A larger palace is `sw1` +
  window, not one dump. `sceneworld::fill_hilbert` fills the hang table;
  it does not live here. Pixels are not in the dump; the
  viewer loads `art/toolheads/*.jpg` into [`Images`] and `NotoSans-Bold`
  into [`Fonts`]. A Label on a hand/portrait slab must dump `print` in
  the parent-local gauntlet. Chrome HP/intent sit on Groups, not on the
  print.
- Quad local plane is XY, origin centre, facing +Z. Slab is the same pose
  with thickness along ±Z and rounded XY corners; `m=` maps to the +Z face
  only. The rim is stock (not the print); `mb=` maps to the −Z face.
  Omitted `mb=` is unprinted — the front image is **not** stamped on the
  back (that UV is X-flipped and ghosts through alpha).
  Euler `r` and `fov` are **radians**. Default fov is π/3.
- Identity `t`/`r`, unit `s`, white `c`, `up=0,1,0`, default fov, `i=1`,
  default range, Lambert omitted.

## Dump grammar (`mt1`)

```
mt1
print-a Unlit m=front-a mb=back-a
print-b Unlit m=front-b mb=back-b
```

- Line 1: `mt1`
- Then `NAME [Unlit|Lambert] [c=hex] [m=key] [mb=key]`
- Default Lambert and white omitted. Round-trips.

## Definition of done

- `cargo test -p scenegraph --features gltf` and `cargo test -p
  scenegraph-mq` green (the second is the backend parity); clippy silent
  with `--all-features`; the wasm32 check above passes.
- A scene, pose, or pick change that could move a dump has a before/after
  dump in the summary (same discipline as `deck_sim` curves). Raster is
  for humans; dumps are for agents. Do not take a screenshot to decide
  whether a Label is on a card — the gauntlet's `problems` section says.
  A new check a glance would need belongs in `problems.rs`, with a test
  that builds the bad scene.

## Neighbours

Ships, CNFT layer stacks, and other generators do not add kinds. They
either stay separate engines or emit a token appearance as `sg1`. Clip
*patterns* (`fan`, `orbit`) live in `scenedrv` (`drv1` → `an1`). A
palace is `sceneworld` (`sw1` → window `sg1`). Read
[`BOUNDARY.md`](BOUNDARY.md) before absorbing a compositor, a cloth
stepper, or a collection plan into this crate.
