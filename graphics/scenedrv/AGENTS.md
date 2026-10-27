# Agent instructions — scenedrv

Closed clip **drivers**. Ops compile to [`sceneanim::Clip`] (`an1`).
`pose(scene, clip, t)` still evaluates. This crate does not sample time.

## Commands

```sh
cargo test -p scenedrv
cargo clippy -p scenedrv --all-targets
```

## Invariants

1. **`an1` is the score.** A driver dump round-trips (`drv1`). Expansion
   is a clip; tests `clip(scene)` then `pose` a posed `sg1` snippet.
   Screenshots are not acceptance.
2. **Closed ops.** `fan | orbit | to`. Unknown ops/keys are load errors.
3. **Names and tags.** `fan` selects `tag=`; `orbit` targets a Camera
   by name; `to` targets one node by name. Missing tag/camera/node is a
   compile error.
4. **Rust is the hook.** `Driver::op(Fan { … }).op(To::lift("th-shredder"))`
   is the same document as the dump. No expressions, no `fn` in `drv1`.
5. **Rest is untouched.** `clip` reads rest; it does not write the scene.
   `pose` clones. Flavor that only exists while inspecting is
   [`inspect`](src/inspect.rs) — clone rest, `insert` Labels, then pose.
   Pose cannot add nodes. Do not add `hidden=`, inspect kinds, or clip
   tracks for text. [`present`](src/inspect.rs) is insert-then-pose for a
   mixer (optional idle underlay first). Pane and gauntlet both call it.

## Dump grammar (`drv1`)

```
drv1
showcase
fan tag=layer
orbit cam=cam
```

```
drv1
inspect
to node=th-shredder open=0.7 t=0,2.32,2.2 r=0,0,0 s=1.6,1.6,1 ease=inout-cubic
```

- Line 1: `drv1`
- Line 2: `NAME [wrap=once|loop|ping]` — wrap default `once`, omitted
- Then one op per line: `fan|orbit|to [k=v …]`
- `fan`: `tag=` required. `open=` time (default 0.4). `x=` `y=` `z=`
  spread per tagged index, sorted by rest `t.z` then name (defaults
  0.12, 0.08, 0.55). `fit=cam` writes `s` so apparent size holds.
- `orbit`: `cam=` required. `from=` start of the camera move (default
  0.4). `az=` `el=` radians (default 0.85, 0.32). `dist=` optional;
  omitted uses rest eye–at length.
- `to`: `node=` required. At least one of `t=` `r=` `s=` `a=` (dest in
  the parent frame; `a=` is surface alpha 0–1). `open=` travel time
  (default 0.4). `ease=` closed. `To::lift` is dest `open=0.7 t=0,2.32,2.2
  r=0,0,0 s=1.6,1.6,1 ease=inout-cubic`. `To::fade` is `open=0.22 a=1
  ease=inout-cubic`. Inspect is a mixer chain: lift, then fade
  (`after=`). Reverse fades out before the card returns. Flavor Labels
  are a scene write, inserted only while the fade layer has `t>0`.

Goldens: `tests/golden/showcase.drv1`, `tests/golden/inspect.drv1`,
`tests/golden/inspect-fade.drv1`.
