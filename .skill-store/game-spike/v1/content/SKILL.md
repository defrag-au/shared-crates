---
name: game-spike
description: Use when starting a new game, a new game screen, or a visual pass on one, in a defrag repo that draws with macroquad. The procedure that produced fight-macroquad — spec first, pure rules crate with events, thin client on macroquad-gamekit and its harness, capture-and-read review loop, soak, web build.
argument-hint: <game idea, e.g. "a ToeJam-style island wander, two-player split-screen">
---

# Building a game screen

The reference is `fight-macroquad` in augminted-bots (`games/_spikes/apps/fight-macroquad`), on
`macroquad-gamekit` in shared-crates. Read its `main.rs` before writing a shell; copy its shape.

Every command runs through the devshell: `nix develop -c <cmd>` from the repo root, or
`direnv exec . <cmd>` where the nix daemon socket is unreachable.

## 1. Write the screen spec first

A markdown spec beside the code, before any client: the design size (portrait 390×844 for phone
first; landscape 844×390 where the genre is played sideways, as `turbo-turns`' racer is), every
element's position and size as numbers, the timings of each animation, and what the
screen shows in each state. The card game's spec is `games/_spikes/web/README.md` — its card anatomy
table is the model: each element, its size as a fraction of the card width, and why.

Decide readability up front: what must be legible when the thing is partly covered (a card in a
fan shows only its top-left strip, so cost, kind and numbers live there).

## 2. Rules in a pure crate that reports events

See the rules `defrag-runtime-pairs` and `defrag-rules-emit-events`. In short:

- No macroquad, no wasm-bindgen, no I/O. `cargo tree --target wasm32-unknown-unknown -p <crate>`
  shows no wasm-bindgen.
- A step returns typed events plus the new view. Numbers and icons arrive as data, not prose.
- A golden test (a seeded game played to the end) and a replay test, before any client exists.
- A catalogue function (like `deck_fight::pack_cards`) lists every face the client can show.
- Physics that decides an outcome (who is knocked off the screen) is a rule: it lives here, not in
  the client or the gamekit. Keep it replayable: `+ - * /` and `sqrt` only, no platform
  `sin`/`cos`/`atan2` (`turbo-sim`'s `angle.rs` turns headings into vectors with polynomials).
- **Feel tests** for anything the player feels: set up the moment by hand, run it, print the number
  and pin a range (`turbo-sim`: a full-speed knock moves a car 70 units and turns it 128°; a hoop
  is 34 ticks off the table). Tune against those numbers, and a "feels soft" report becomes a
  value to move.
- CPU drivers that never err make modes that depend on mistakes (an elimination screen, a race
  gap) stall. Give them seeded lapses, and assert in the golden test that the mode resolves.

## 3. A thin client on the gamekit and its harness

Read `shared-crates/gaming/macroquad-gamekit/CATALOG.md` first, and the rule `defrag-macroquad-traps`.

- **Never write the frame loop.** Implement the kit's `app::Game` (`update`, `draw`, `backdrop`,
  and as needed `ready`, `capture_due`, `capture_note`, `shake`) and hand it to `app::run(DESIGN,
  game)`. The loop brings the clock, `--shot`/`--at`/`--size` captures, settling while a tiling
  window manager resizes the window, the glyph gate, a preloader cover until the screen draws
  whole, `--frame-log`, `--seconds N` and the native 60 FPS cap. `fight-macroquad`, `backrooms`
  and `turbo-turns` are the references.
- **Pick the stage.** `app::run` draws in design units at the window's device pixels. A
  pixel-art game runs on `app::run_on(Stage::Pixel { width, height }, game)`: the kit owns the
  canvas, scales it by whole numbers and letterboxes it, and a `--shot` is the canvas at 1:1
  (`island-funk`). An arcade game with a fixed play field runs on `Stage::board(SIZE)`: the
  board is fitted whole and centred, anything drawn past its edge is hidden, and the margins
  belong to `Game::draw_margin`. Draw text at `frame.px`, and read the mouse with
  `frame.pointer()`. Taps, drags and touches arrive in stage units as `frame.input.gesture`. Feed
  it to a `Pointer`, or to `Swipes` for four-way swipes. Never map window points by hand.
- **Bind input to actions, never poll `KeyCode`s.** Give the game an enum of its verbs, bind
  each to keys, swipes, taps and pad buttons in an `Actions`, and call `update(frame.input, dt)`
  once a frame. Act on a press with `take` when the game is ready for it (a hop has landed, a
  tick is running): the press is buffered, so a tap during a cooldown still lands. `pressed` is
  this frame only and misses in a tick that skips a frame.
- **A scored arcade game runs in `arcade-harness`'s shell**, not its own loop. Its rules crate
  implements `arcade_core::ArcadeGame` and draws randomness only from `arcade_core::Rng`, which
  is the same on native and wasm32. Never use `rand::rngs::SmallRng`: it changes algorithm with
  pointer width, so a replay verified natively fails on wasm. The client implements
  `ArcadeView` (actions, one input per tick, draw) and calls `run_arcade` on a stage. The shell
  brings sign-in, the server's seed, recording, submission and the title and game-over screens.
  Give it a real font through `ArcadeView::font`. The built-in font is ASCII only, and draws a
  tofu box for anything else. `xeno-invaders` in augminted-bots is the reference.
- **On a phone, use the kit's `Thumbs` for controls**, unless the game needs something else. The
  first finger held anywhere is a stick centred where it landed, any other tap is an activation (a
  second finger, or one finger's quick tap), and only a few reserved areas, such as a map button,
  take their own taps. Nothing covers the game until a thumb is down. Reserve only what is drawn,
  keep reserved areas off the bottom edge (where the stick starts and the home indicator lives), and
  make them at least 44 points. Inventory and other choices go in a panel opened by a button, with
  big tiles that act on lift (`ThumbFrame::lifted`), not a strip of small slots. Fill the screen and follow
  rotation: run on `Stage::PixelFit` with a function that sizes the canvas as the screen divided by a
  whole number (Island Funk's `canvas_for`), zoomed closer on a phone than on a desktop, and read the
  canvas size from `frame.viewport.size` each frame. For fullscreen, stamp the kit's
  `PageScript::Fullscreen` from `build.rs` (Android and iPad go fullscreen on the first tap); an
  iPhone cannot, so the bundler's manifest makes Add to Home Screen launch it without browser bars.
- **Reach mid-game moments with `--keys`, not a bot.** `--keys "5:Up,10:Up,20-40:Right"` presses
  and holds keys by frame, merged into `frame.input`, so `--shot --at` can capture a moment deep
  in play. Read a flag with no value (`--play`, `--debug`) with `harness::switch`; `flag` reads
  the argument after a name and cannot see one.
- **Step the simulation on a fixed tick** when anything moves fast or must replay: return
  `Some(hz)` from `Game::tick_rate` and put physics, collisions and spawns in `Game::tick`.
  Read input in `update`, which runs first each frame, and draw moving things at
  `prev.lerp(now, frame.alpha)`. Variable `dt` tunnels a fast ball through a brick.
- **Carry the macroquad `[patch]`** in the root `Cargo.toml` of the game's own workspace. The
  rule `defrag-macroquad-traps` has the lines, and `mq-bundle` refuses a gamekit build without
  them. The kit needs rustc 1.88 or newer.
- A game that reviews itself — bots playing to set moments, a screenshot at each — returns true
  from `Game::scripted` and saves each finished frame in `Game::presented` with
  `harness::write_capture` (`island-funk`'s bot).
- The game reads its own flags (seed, level) with `harness::flag_num`; modes that are not the
  game loop (`--soak`, `--sheet`) stay in `main`, before `app::run`.
- Load fonts with the kit's `load_font`, so the glyph gate can warm them.
- Draw text only through the kit — `RichText`/`TextLayout`, or `draw_run` for one line — so the
  report sees it. Measure with `MeasureAt(frame.px)`.
- A game that binds render targets of its own (a light map, split-screen halves) wraps them in
  `push_camera_state`/`pop_camera_state`, so `draw` ends on the camera the loop set.
- **Never animate a font size.** Text that pops or pulses keeps one size: bake it once and scale
  the texture through a `Pose` (`turbo-turns`' `hud::Stamps`), or fade it while its shape scales
  (`fight-macroquad`'s `ui::grown`).
- Faces that rarely change are baked with `Baker` and drawn through a `Pose`. For a card game,
  read the kit's `CARD-FACES.md` first: what goes in the strip a fan leaves visible, the layers,
  baking and lighting.
- Map the harness's pointer scripts (`--hover N`, `--drag N`, `--rclick N`) onto the game's
  targets with `Script::pointer`.
- Give the game a `--soak N` (play N seeds to the end through the real screen, checking what is
  shown against the engine every step) and a `--sheet` (every face at once).

## 4. Review loop: capture, read, fix

```sh
nix develop -c cargo build -p <game>
./target/debug/<game> --shot .tmp/s.png --at 1.25 --hover 2
./target/debug/<game> --sheet .tmp/sheet.png
```

Each capture writes `s.png` and `s.txt`.

- **Look at `s.png`.** Judge it against the spec and against the standard the user asked for:
  readability, colour, depth, alignment, art cropping, things covering other things.
- **Then check `s.txt`'s Problems section**, which catches what a glance misses: text cut short,
  too small, off its surface or overlapping. Fix each, or say why it stands.
- Capture animations at several moments with `--at` (start, middle, end), not only at rest.
- Change one thing, rebuild, recapture the same moment. Keep the captures in `.tmp/`.

## 5. Check the window, soak, build for the web

```sh
./target/debug/<game>                     # the real window: right way up, input lands where aimed
./target/debug/<game> --frame-log         # play through the start: nothing slow after the first second
./target/debug/<game> --soak 20           # must end "0 with a problem"
nix develop -c cargo clippy -p <game> --all-targets -- -D warnings
```

The window check is not optional after any camera, input or window change (rule
`defrag-look-at-the-game`). On macOS, look at the real window by starting the game, taking a
screenshot, and killing the process you started by its PID (`rules/core/kill-only-what-you-started`):

```sh
./target/debug/<game> >/dev/null 2>&1 & pid=$!; sleep 3; screencapture -x .tmp/window.png; kill $pid
```

The screenshot is the whole screen, including the user's desktop: keep it in `.tmp/` and say so.

Captures and soaks run on the fixed clock, so they never show a stall: a frame that took two
seconds looks like any other. `--frame-log` is how a beachball is found (`turbo-turns`' was the
countdown rasterising a new font size every frame).

Web: build with the repo's bundling script (augminted-bots: rule
`augminted-macroquad-client-bundles`), report the wasm size raw and gzipped, and confirm no
wasm-bindgen import:

```sh
grep -c "__wbindgen" <dist>/<game>.wasm    # must print 0
```

- **Sound or pads** come from the kit's `audio` and `gamepads` features. Their JS goes in the page:
  a `build.rs` with `macroquad_gamekit::web::stamp(web_dir, &[Plugin::Audio, Plugin::Gamepads])`,
  and the bundling script's `--web` pointing at that directory. `island-funk` is the shape.
- **Pixel-art characters** are text in code: rows of characters, a `Palette` mapping each to a
  colour, baked by the kit's `PixelSprite`. `Grid::outlined`, `silhouette` and `flipped` bake
  the variants a state needs (an angry red edge, a hit flash, facing left), so a mood is a
  different texture rather than drawing tricks. Parse every sprite in a test so a ragged row
  fails `cargo test`. `island-funk`'s `sprites.rs` is the shape.
- **Sounds placed in a world** use `Panned` and `Voices`. macroquad cannot pan a playing sound,
  so `Panned` bakes a mono sound at a few stereo positions and `Audio::play_panned` picks the
  nearest. `Voices` caps how many play at once. Do not reach for kira or another audio crate:
  they pull in wasm-bindgen, which cannot run under miniquad.
- **Every import needs a script.** The bundler (`cargo run -p mq-bundle`, shared-crates'
  `macroquad-bundle`; augminted-bots wraps it as `scripts/stage-macroquad-client.sh`) fails a
  bundle whose wasm imports a function no staged script provides. `mq-bundle check <dir> <wasm>`
  checks a bundle staged some other way.
- **Then load it in a browser and read the console.** A web build that passes every native check
  and capture can still die on its first frame: `fight-macroquad` did, on a WebGL-1-only call.
  Expect at most a notice that an unused `sapp_jsutils` is present, and the audio autoplay warning
  until the first key press. Use the `browser-shot` skill — it launches its own headless browser,
  reports console errors, uncaught exceptions and failed requests, and captures the canvas:

  ```sh
  browser-shot "<url>" 390 844 .tmp/game.png --select canvas --dpr 3 --touch \
    --settle-ms 5000 --no-settle
  ```

  A macroquad client has no browser-side readiness signal by default — `Game::ready` is the
  native harness — so a first look uses `--no-settle` and a five-second floor. If the client sets
  `window.__ready`, pass `--ready "window.__ready === true"` instead. Read `.tmp/game.txt` before
  judging the image. On a Linux box with no GPU the sidecar says the renderer is software, which
  is the same SwiftShader caveat as
  `--use-gl=angle --use-angle=swiftshader --enable-unsafe-swiftshader`.

## 6. Write it down

Keep a short journal beside the client (`NOTES.md`): what was tried, what each capture showed,
which traps were hit. A trap that is not already in `defrag-macroquad-traps` goes there, through
the `capture-rule` skill, so the next game does not hit it.
