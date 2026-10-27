# Boundary — what the graph is not

Framing for work that *touches* this crate. The dump grammar lives in
`AGENTS.md`. This file is the neighbour map: engines we will not absorb,
and the one document they may emit.

Cards, ships, and CNFT tokens are **fixtures using general kinds**. A
new look is not a new `Kind`. A new clock is not a playhead. A new GPU
program is not `sg1`.

The shared split:

| Neighbour owns | Graph owns |
|---|---|
| Generation, rarity, constraints, chain metadata | Rest pose of one appearance |
| Live sim at a fixed `dt` (cloth, optional water field) | Posed `Geometry` / interned images the sim wrote |
| Shader programs, blend stacks, encode-to-PNG | Closed `sh=`, named nodes, hit, orbit |
| Collection of N tokens (`plan.json`, HCF, CIP-25) | One token as `sg1` + `mt1` |

If a draw call can exist outside the graph, stop. If a solver, a Verlet
step, or a Gerstner fragment shader wants a home in `sg1`, it is in the
wrong crate.

---

## Ship compositor (`widgets/ship-renderer`)

`ship-renderer` is a 2D WebGL2 layer compositor: hull PNGs, integer
z-order, Normal/Multiply blend, rAF with variable `dt`, wasm-bindgen
`WebGl2RenderingContext`. Cloth is CPU Verlet in `ship-types`
(`ClothState::step`) uploaded as a grid mesh. Water/sky on screen is a
fullscreen fragment shader (`u_time`, hull-profile `R32F`, ship speed,
sun). `WaveState` in `ship-types` is a dump-first 1D field and is **not**
what the compositor draws.

**Keep the engines separate.** Do not merge `LayerCompositor` into
`gpu.rs`. Do not add `Kind::Sail` / `Kind::Water` / `Kind::Cloth`.

`ship-types` is already the right shared crate (manifest, anchors,
slack, hull profile, Verlet, optional `WaveState`). The graph may
*consume* it later the same way it consumes `Images` / `Geometry`.

When a ship must live in the hit/pose/dump loop it is fixtures:

```
sg1
ship Group
cam Camera eye=0,0,8 at=0,0,0
sky Quad 16x10 sh=Unlit p=ship t=0,0,-1
hull Quad 6x8 sh=Unlit m=hull-base p=ship
foresail Mesh Geom g=sail-fore p=ship t=0,1.2,0.2
water Quad 16x6 sh=Unlit p=ship t=0,-2.2,0.4
```

Z is camera depth, not a painter index. Cloth motion is a sibling
stepper shaped like `sceneanim`: `step_cloth(scene, wind, t, dt=1/60) →
Geometry` (or a posed `Scene`). Fixed substeps. Wind is data. The
gauntlet is a particle census, not a screenshot of a billow.

Water is two honest options, not both in `sg1`:

- **Pane-only.** Closed `sh=Water` in an *app* backend. Waterline,
  speed, sun, interned profile are document/clip data. `t` is the
  argument to `pose`, never a node. Foam is not in the dump.
- **Dump-scored.** `WaveState` at the same fixed `dt` writes a
  heightfield; dump a census; the shader only shades that field.

A Gerstner fragment shader will not round-trip across GPUs. Fine for a
portrait pane; fatal if replay is the score.

A game pane that needs both systems composites: ship pass to a texture
on the **same** device, then a `Quad`. That is `present_list`, not
GPU→CPU→GPU.

### WASM is not WebGL

Throwing away the WebGL compositor does not throw away the browser.
WASM in this repo already has two GPU paths:

- **wgpu** (this crate’s `gpu` backend, eframe). On wasm that is WebGPU,
  with wgpu able to fall back to WebGL2 *inside wgpu*. No hand-written
  `web-sys` GL.
- **miniquad / macroquad** (arcade clients). Also a canvas, own `gl.js`.
  wasm-bindgen GL must never run here.

`DrawList` is pure. Native vs browser is a backend knob. A ship in an
arcade activity uses the macroquad backend (or a shared-device texture).
A ship-builder pane can be the same wgpu path as `scene-view`, compiled
to `wasm32-unknown-unknown`.

Revisit a merge only if the WebGL compositor is gone, cloth is on a
fixed-`dt` stepper, and water is *either* a dump-scored field *or* a
pane-only shade.

---

## Hodlcroft compositor (generative CNFT)

Tree: `~/code/hodlcroft/compositor`. Pipeline:

```
project.toml + assets  →  gen (CSP)  →  plan.json  →  render  →  PNG/HCF
     collection                 choices         appearance
```

A **collection is not a scene.** CSP, rarity quotas, variant flow,
exclusive groups, CIP-25/Metaplex, `audit`, filename conventions — none
of that is a camera, a parent, or a hit. `plan.json` stays the
collection document (`RenderJob`: slot, asset id, z_order, effects,
optional warp).

A **token’s appearance** can be a scene. That is the only part of
hodlcroft that belongs in scenegraph language. Gen still writes the
plan. [`Stack`] turns fulfilled layers into `sg1` + `mt1` as `Quad`s
(zero-width prints, not `Mesh Slab` — a slab back is X-flipped and
ghosts through alpha). Rest Z is a hair (`GAP=0.001`) so the frontal
reads as one image. `scene-view --plan PLAN.json --assets DIR [--job N]`
loads **one** render job with no playhead. Showcase is the sibling
crate `scenedrv` (`drv1`): `fan` + `orbit` compile to `an1`. Score
`clip(scene)` then `pose` — not a screenshot. `--showcase` plays that
clip; it does not write rest. `hit_with` samples PNG alpha so a click
on empty hat canvas selects `skin`. The GPU discards the same cutoff.
Effects (colorize, grain) stay compositor-side: bake, then intern.

| Expressing | In the graph? |
|---|---|
| 10k-token gen, rarity, uniqueness, chain metadata | No. `plan.json` + CSP. |
| Flat z-order PNG stack (classic PFP) | Appearance may be `sg1`; `layering` remains the batch compositor. |
| One token as 2.5D (mug warp, ceramic, occlusion, studio pick) | Yes. Named nodes + hit. |
| Holo / grain / hue-key / foil | Closed `sh=` in a compositor backend. Bake to interned images before `m=`. |

Warp is the exception: flat design + surface mesh stays `Mesh Geom`, not
another PNG, so the print is still on the mug when you orbit. Colorize
and grain bake; they are not shader source in `sg1`.

### Rest vs explode

Painter’s `z_order` is not world Z, but rest Z is only enough to order
hits and avoid z-fight (`t=0,0,0.001` per slot). The mint is that
frontal. A `Quad` is the print (no rim, no back). Node names are slots
(`skin`, `hat`). Pick is alpha-aware.

```
sg1
token Group
cam Camera eye=0,0,6 at=0,0,0
skin Quad 2x2 mat=skin p=token
clothes Quad 2x2 mat=clothes p=token t=0,0,0.003
hat Quad 2x2 mat=hat p=token t=0,0,0.008
```

Showcase: `Driver::from_dump(drv1).clip(&scene)` then `pose`. At
`open=` the layers have travelled; at t=1 `cam eye.x` is off axis.
`--showcase` plays it. Do not treat that pose as rest.

A generative drop is **N scenes sharing interned atlases**, not one
scene with 10k characters. Do not encode the drop itself as a graph.

Mugz-style tokens still want `Mesh Geom` for a UV wrap. A classic PFP is
this quad stack. Neither specialises the kind inventory.

---

## Virtual art gallery

The REGL palace (`virtual-art-gallery`) is Hilbert generation, pointer-lock
FPS, painting glow shaders, and IIIF/ARTIC fetch. Do not add
`Kind::Gallery` or marble GLSL to `sg1`.

| They own | Graph owns |
|---|---|
| Hilbert maze, pagination, IIIF fetch | Window as named nodes (`gallery.sg1` = span=0 emit) |
| Pointer-lock, footsteps, collision loop | Camera node; `hit` for gaze and walls |
| Painting wrapping / floor-mirror shaders | `Mesh Slab` + interned print + Label |

A hang is `tag=hang` on a `Mesh Slab` parented to `tag=wall`. Walk is a
`sceneinput` tool (`walk` / `stride`), not a playhead. The palace lives
in [`sceneworld`](../sceneworld) (`sw1`): sparse hangs + cursor/span.
`World::emit` writes the Chebyshev window as `sg1`. A palace (`order=`)
is a Hilbert corridor maze plus seeded shortcuts — the same layout as
the REGL gallery — so line of sight never reaches the window rim.
Prints stream from a catalog along the curve; architecture names are
stable across pages.
A later sculpture is `Mesh Geom` (from glTF) on the plinth — same dump.

IIIF is the pane: [`cardano_assets::AssetId`] parses `policy:hex`,
`image-core` builds the warm URLs (**400** then **1646** — nothing else).
An asset print is still a `Mesh Slab` whose `m=` *is* that `policy:hex`.
Rest is 1:1 with `rad=0`. LOD intern 400 then 1646 under the same key.
Non-square pixels may write `w`×`h` (longest side held).
The cell dump names three sample assets (Toolhead, Pirate, Perp);
the pane IIIF-loads those keys.
