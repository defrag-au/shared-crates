# Agent instructions — sceneapp

A scene **app** is a folder of dumps. This crate loads it and scores it. It
holds no game rules and depends on no game: a game crate decides what a pick
or a key does, behind [`Game`], and the consumer passes one in as
`NewGame`. The registry that knows a repository's games, the
`scene-gauntlet` binary and the clients belong to that repository (in
augminted-bots: `scenegames`, `scene-play` on wgpu, `scene-mq` on
macroquad, and the `scene-dump` authoring skill).

## Commands

```sh
cargo test -p sceneapp       # tests/fixtures/apps/{room,table,gallery,ttt}, scored without a game
cargo clippy -p sceneapp --all-targets -- -D warnings
```

## Folder

```
scene.sg1          # rest, unless world.sw1
world.sw1          # optional; emit is rest. Both files ⇒ emit dump equals scene.sg1
materials.mt1
clip.an1
drive.drv1
session.in1        # pointer; ndc+sel is the hit assertion
act.act1           # resolved rest-write ops on names
after.sg1          # rest after one ply (or clock.n ticks)
delta.sgd1         # named +/~/- of rest → after
clock.tk1          # shell playhead. Not a node. Gauntlet drives `n` steps of `dt`.
replay.rp1         # a whole game: steps, each with hit, outcome, cues, delta
well.tw1 | board.tt1   # game state: the version selects the game (the consumer's registry)
models/KEY.glb     # binary glTF, interned as model KEY (`Mesh Geom g=KEY`)
images/KEY.png     # decoded into `App::images` (`m=KEY`): contrast reads the texels
```

`App::load(dir)` reads disk; `App::from_source(label, read)` reads through a
function and does no I/O, so a wasm client embeds its folders. It reads no
models or images: such a client calls `App::add_model(key, bytes)` and
`App::add_image(key, bytes)` for each.

A model is one node: `scenegraph::model_from_gltf` bakes the file's node
transforms into its parts, one per surface (split under 65,535 vertices),
and returns its textures as `KEY/NAME`, which `App::images` collects. A
client hands those images to its renderer alongside its own `images/`. The
gauntlet dump's `models` block, after the census, lists each model in its
file's own units (`KEY model parts=N Ff Vv bbox=MIN:MAX`, plus `tiled=N` for
triangles whose UVs span texture tiles and so draw clamped); read the
bounds to choose `s=`. A rigged model's line ends `rig=N clips=NAME:LEN,…`:
the clips a node may name in `pose=CLIP@T`.

With a `clip.an1`, rest is judged through the clip as well as at its
start: at `CLIP_LOOKS` (8) moments evenly through it, each problem reported
once at the first that showed it, as `PROBLEM @clip.an1 t=SECONDS`. The
`pose` line gains `judged=8`.

## Who decides a move

The game. With a game (the consumer's registry finds it), the gauntlet checks
`emit == scene.sg1`, a session hit is `Game::pick_at(hit, at)`, `clock.n` is
that many `Game::tick`s, and `replay.rp1` plays from the game's start.

The game sees the camera, two ways, and both default to not caring:
- **`Game::pick_at(name, &Pointed)`**: every shell's pick (gauntlet, replay,
  scene-mq, scene-play) carries the live view's `eye` and the `point` the
  pointer landed on; `Pointed::reach()` is the distance between. Default
  calls `Game::pick(name)`. A game with "within reach" rules makes a
  camera-less `pick` out of reach.
- **`Game::moved(eye)`**: after every camera step and live orbit/walk.
  `Applied` is a move made of the walk (out through a door, onto a
  trigger): its cues and delta are that step's. `Blocked` (default) is
  nothing happened.
`escape` uses both: the key within 2.2, leaving by walking onto the porch. `act.act1`,
if present, must describe the same rest write the game made. Without a game,
no node is a move: only `act.act1` writes rest, and its target must be the hit.
`Game::pick` defaults to `Blocked`, and so does `Game::drag(from, to)`: a
drag is the node it went down on and the node under the release point, both
picked on the scene before the drag; `Game::drag_tags` (default empty) names
what a pane lets the pointer carry. Never add a selector here that reads a
Label's text or a tag to decide a move — that was tic-tac-toe living in the
shell, and it now lives in `tictac`.

Clock is never in `sg1`. Tests pass `dt` into `Clock::step`. `propose_tick` is
empty until a game emits `to`/`set` ops. Tick `Blocked` is game over; input
`Blocked` is a wall. `act1` is graph rest-write, not a game intent. `fx1` is
the step's IO (`sound KEY`). Not rest, not a node. The game crate names keys.
Slab is card stock only — board cells are `Quad`.

## Replay (`rp1`)

```
rp1
# X takes the centre
pick ndc=0,0 hit=c11
fx mark
+ mark-c11 Label font=ibm size=0.55 "X" c=1a1210 p=c11 t=0,0,0.05
~ turn Label font=ibm size=0.14 "O to play" p=ttt t=0,-2.15,0.04
pick ndc=0,0 hit=c11 blocked
input reset
tick blocked
```

- Steps: `pick ndc=X,Y|node=NAME [aspect=A] hit=NAME|-`, `input TOKEN`,
  `tick`; a trailing `blocked` says the game refused it (default: applied).
  `node=` aims where that node is drawn (the centre of a mesh's bounds, so a
  model is hit on its body, else its origin); the hit is still asserted,
  and a missing or undrawn node comes back as `unknown:NAME` / `undrawn:NAME`
  (one token, so a blessed replay still parses).
- Camera steps move the live view, never rest, and are never `blocked`:
  `orbit dx= dy= [zoom=]` (drag around the target; negative zoom is closer),
  `turn dx= dy=` (look from where you stand; `dx>0` turns right, `dy>0`
  looks up; 1.5 rad per unit, pitch limit ±1.35), `walk f= [s=]` (world
  units along the flattened look; stops 0.45 short of what the eye-height
  ray hits, so low furniture does not stop it). A `node=` aim that misses
  (`undrawn:X` / `unknown:X`) is never an outcome: bless writes it, the next
  run fails on it. Each is followed by `view eye=x,y,z at=x,y,z`, asserted.
  In a `world.sw1` folder a walk that crosses far enough pages the window:
  that page is the step's delta. Picks after it aim from the new view.
- A folder whose view moves (a `walk`/`orbit` session, or camera steps) is
  judged across its views: `off-camera` is not reported, and `small` /
  `covered` count only for what is in focus (near the centre, unobstructed),
  failing only if no look at it passed. The checked line adds `in-focus=N`,
  and an `in-focus NAME@AT…` line under it names every one with where it
  was first read: the first look by which each of its checks had passed
  once (`@rest`, `@LINE` of the replay, or `@never`, which is also a
  problem). `backwards` and `cropped` labels are judged the same way.
- In such a folder each pick also writes `reach=D`: metres from the eye to
  the point it landed on, two places. Bless writes it, a run asserts it. A
  folder whose view does not move writes none.
- `drag node=A|ndc=X,Y to=B|to=X,Y [aspect=A] hit=NAME drop=NAME`: down at
  one aim, up at the other, both asserted, then `Game::drag(hit, drop)`. The
  score line gains ` drags=N` only when a replay has one.
- After a step: one optional `fx KEY…` line (absent ⇒ no cues), then its
  `sgd1` delta lines of the emitted scene (absent ⇒ nothing changed).
- `#` lines are kept through a bless. Unknown steps and keys are load errors.
- A pick goes through the same `sceneinput` pick as the panes: a Label hit
  selects its parent, so a mark selects its cell.
- Without a game only picks and drags are legal, and they assert hits only.
- Authoring loop: write the steps bare, `--bless`, **read every outcome**,
  fix the comments to say what happened, re-run without `--bless`. A
  mismatch names the replay line and prints want and got.

## Problems

The dump leads with `problems N`: what rest, and every replay step that
changed the scene, show that a glance would miss (`scenegraph::problems`).
Any problem fails the gauntlet; the binary still prints the whole dump and
exits 1. Each is reported once, where first seen (`@replay.rp1:LINE` for a
step).

```
problems 2
small next em=6.1 min=8
off-print mark-c00 parent=c00 spills past the face @replay.rp1:8
```

| Kind | Fix |
|---|---|
| `off-print` | Move or shrink the Label onto its face, in front of it. |
| `missing` | Change the text or the face; `.notdef` is drawn. |
| `off-camera` | Move the node or the camera. Not checked when `session.in1` is `walk` or `orbit`. |
| `small` | Raise `size`: the em is in design units on a 640-tall stage, minimum 8. |
| `z-fight` | Lift one of the pair off the shared plane (`t` z). |
| `covered` | Uncover the control somewhere: nothing on its face picks it. |
| `contrast` | Change the Label's `c=` or its face's: under 3:1 (WCAG, large text) against a plain face. `scenegraph::problems::ink_for(face)` picks dark or white. |
| `cropped` | Move the Label or the camera: it is on screen but a corner is past the edge. Judged per view, like `small`. |
| `backwards` | Turn the Label (`r=`) or look at it from its front: its face points away from the eye, so it reads mirrored. Judged per view. |
| `floating` | Lower the model by the gap: its base is over 2 cm (or 2% of its size) above the surface under it, and under 0.5 m (higher is hung on purpose). A model's mesh may start above its own origin. |
| `sunk` | Raise the model by the depth: its base is below the surface under it. |
| `heading` | Turn the node or its path: it has `front=` and moved more than 60° off it between two clip moments or replay steps. Reported once per node. |
| `scale` | Give the model node an `s=`: it is drawn under 1 cm or over 50 m on its longest side, which is an exporter's units, not metres. |
| `off-origin` | The model sits far from its own origin, so `t=`/`r=` swing it around empty space. Use another model or pivot it under a `Group`. |
| `heavy` | Draw fewer triangles: the scene is over 200,000. Swap in a lighter model. |

Controls are `INSPECT_TAG` (`hand`) plus the game's `Game::control_tags`,
for picking. `covered` applies to every tagged node, not only those: an
author's `tag=exhibit` is a promise it can be seen. `contrast` also reads a
textured Quad's texels under the label (tinted by its `c=`), when the image
is in `App::images`. Fix the scene, never the check; a check that is wrong
for a whole class of scene (as `off-camera` was for a walk-through) is
narrowed in code, with a test.

## Seeing without a screenshot

The dump carries the frame as text. After `models`:

- `sketch 48x16 aspect=1.5`, then 16 rows of one symbol per pick-ray cell
  (`scenegraph::sketch`), a `key` of symbol=name:cells (most first), and
  `unseen` for tagged nodes nothing hit. A Label, and a tagged node a ray
  at its aim point hits, claims that cell even when smaller than one, so
  `unseen` agrees with a pick. A node with `front=` adds
  `faces=toward|away|left|right` (as seen from the eye) to its key entry.
- `Options { sketch_steps: true }` (`scene-gauntlet --sketch`) adds
  `after @replay.rp1:LINE` and a sketch after every step;
  `Options { close_up: Some(NODE) }` (`--close-up=NODE`) adds a close-up
  of that node after each sketch: `sketch NODE 48x16 window=X0,Y0:X1,Y1`,
  the same grid over its padded screen bounds, with `cut=EDGE` in the key
  when it runs off the view, or `sketch NODE off screen`.
- `images local=N remote=N absent=a,b`: textures the checks could read,
  asset prints the pane fetches (`policy:hex`), and keys nothing supplied.
  Printed only when the scene names images.

## Gauntlet dump

```
gauntlet
problems 0
checked scenes=8 labels=30 prints=22 controls=72 drawables=118
roundtrip scene.sg1 session.in1 act.act1 after.sg1 delta.sgd1 replay.rp1
game emit scene.sg1
census
quads=… meshes=…
sketch 48x16 aspect=1.5
… 16 rows …
key a=board:420 b=c11:24 …
clock t=0 dt=0.0167 n=0
hit ndc=0,0 name=c11
pick c11 applied
act act.act1
play after.sg1
delta delta.sgd1
replay replay.rp1 steps=10 picks=8 inputs=2 ticks=0 blocked=3
ok
```

Screenshots are not acceptance.
