# Agent instructions — sceneinput

Pure pointer session. `step(scene, list, session, event)` writes a **new**
[`scenegraph::Scene`] only when the rest pose changes. Selection lives on
the session, not as a node tag. Dumps are the tests.

## Commands

```sh
cargo test -p sceneinput
cargo clippy -p sceneinput --all-targets
```

## Invariants

1. **Rest scene is the document.** Pick does not mutate it. Orbit writes
   the active camera; drag writes `xf.t` of the hit node; walk writes
   `cam` eye/at (look around the eye, stride in XZ). NDC x+ looks
   right; stride+ is right. Collision is
   `DrawList::hit` with a pad — not a node tag.
2. **No widget kinds.** Tools are `pick|orbit|drag|walk`. Events are
   `Down|Move|Up|Wheel` in NDC (y up). Unknown tokens are load errors.
   WASD in a pane calls [`stride`], not a new event. A pick made while
   another tool owns the pointer (a walk's crosshair, at `[0, 0]`) is
   [`pick_ndc`]: the pick tool's rule, no session.
3. **Selection is session state.** `sel=name` is not `tag=` on the graph.
   Authoring tags stay on the node (`tag=hand`). A Label pick writes the
   parent (the card, button, or pile) — chrome is not a separate control.
4. **`list` is what the pointer sees.** Pass a posed `DrawList` if a clip
   is playing; still write the rest scene. A clip that keys `t` on a node
   will hide rest drags in the posed picture — the viewer shows rest while
   the drag tool is selected. `step` is geometry-only (missing images are
   opaque). `step_with(…, images)` uses `DrawList::hit_ndc_with` so a
   transparent CNFT slot does not steal the pick from the layer behind.
5. **Dump is the score.** `in1` round-trips. A pick/orbit/drag that could
   change observable state has a dump snippet in the test — not a screenshot.

## Dump grammar (`in1`)

```
in1
pick
ndc=-0.3,0.1
sel=a face=front uv=0.5,0.5 at=-1.3,1.55,-0.15
```

- Line 1: `in1`
- Line 2: `pick|orbit|drag|walk`
- Then optional `ndc=x,y`, `aspect=` (omitted if 1), `sel=NAME face=front|back|edge uv=u,v at=x,y,z`
- Default tool in a new session is `pick`. Default ndc `0,0` omitted.

Goldens: `tests/golden/pick-a.in1`.
