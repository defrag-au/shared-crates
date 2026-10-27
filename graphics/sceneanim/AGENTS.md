# Agent instructions — sceneanim

Pure clip evaluator. `pose(scene, clip, t)` writes a **new** [`scenegraph::Scene`].
The graph has no playhead. Dumps are the tests.

## Commands

```sh
cargo test -p sceneanim
cargo clippy -p sceneanim --all-targets
```

## Invariants

1. **Scene is rest pose.** Animation never mutates the input scene.
2. **Names are identity.** Channels target node names (`crate t`, `cam eye`).
3. **Closed tracks / interp / wrap / ease.** `t|r|s|eye|at|a|pose`, `Step|Linear|Cubic`,
   `once|loop|ping`, `ease=linear|in-quad|out-quad|inout-quad|in-cubic|out-cubic|inout-cubic|in-sine|out-sine|inout-sine`.
   Unknown tokens are load errors. Default ease `linear` is omitted.
   Ease warps `u` between keys; it is not a new interp.
4. **Radians.** Rotation keys are Euler XYZ, same as `Transform.r`. Linear
   rotation is slerp; Cubic rotation is a smoothstep slerp (not euler Catmull).
5. **Dump is the score.** `an1` round-trips. A pose that could move a node has
   a posed `sg1` snippet in the test (same discipline as scenegraph goldens).
   Screenshots are not acceptance.
6. **Several clips at once.** `pose_layers(scene, &[(clip, t), …])` overlays
   in order (later writes win the same node+track). [`Mixer`] is playheads
   (`mx1`): `go` / `then(after=)` / `retreat` / `tick`. Reverse is `rate=-1`
   on the same clip, not a second document. Follow-ons wait until `after`
   parks; on retreat they play backward first, then the predecessor.
   Track `a` writes inline surface alpha (`c=` rgba). The graph still has
   no playhead.

## Dump grammar (`an1`)

```
an1
bob wrap=loop
crate t 0:0,0.4,0 0.5:0,0.7,0 1:0,0.4,0
```

- Line 1: `an1`
- Line 2: `NAME [wrap=once|loop|ping] [len=seconds]`
- Then `NODE TRACK [Step|Linear|Cubic] [ease=inout-cubic] time:x,y,z ...`
  Track `a` is alpha; `x` is 0–1, `y,z` unused.
  Track `pose` is a posed model's seconds into its own clip, in `x`. The
  node names the clip (`pose=Walk@0` in `sg1`); the track only writes the
  time, and the model wraps it by the clip's length. A node with no
  `pose=` is a `BadTrack`. Which clips a model has is the scenegraph's
  business (its census), not this crate's.
- Default wrap `once`, default interp `Linear`, default ease `linear`, omitted.
- `len=` only if duration is longer than the last key.

Goldens: `tests/golden/bob.an1`, `tests/golden/hover.an1`,
`tests/golden/table-hover.an1`.

Patterns over a tag or a formula (`fan`, `orbit`, `to`) live in the sibling
crate `scenedrv` (`drv1` compiles to this clip). Do not put ops or
functions in `an1`. Inspect flavor is a scene write (`inspect`), not a
track.
