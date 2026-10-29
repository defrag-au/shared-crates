# macroquad-gamekit catalogue

**Read this before building game-feel code.** One line per module, alphabetical,
generated from each module's own `//!` header by `tests/catalog.rs` — so it cannot drift.

Regenerate: `UPDATE_CATALOG=1 cargo test -p macroquad-gamekit --test catalog`

The companion to `macroquad-widgets` (stateless widgets): everything here holds state
across frames and takes time as an argument. Stories are in `macroquad-storybook`
under "gamekit". MACROQUAD only: no wasm-bindgen, see the shared-crates CLAUDE.md on
runtime pairs.

37 modules.

| module | what it is |
|---|---|
| `app` | `run` — the frame loop every gamekit client runs, written once: the clock, the stage, startup and the preloader, the glyph gate, captures, the frame log and the native frame cap, around a [`Game`]'s update and draw |
| `audio` | `Audio` — sound effects and one looping music track behind a single mute, plus `wav`, which wraps synthesised samples so macroquad can load them (feature `audio`) |
| `bake` | `Baked` — draw something once into a texture, then move, turn and fade the texture: how a card face is drawn every frame without laying out its text every frame |
| `dir` | `Dir4` — up, down, left and right: a swipe's direction, a hop, a step on a grid |
| `ease` | `Ease` — named easing curves, so an animation says how it moves rather than writing the maths at each call site |
| `face` | `face` — the parts of a printed card face: dark plates, cut gems, stat emblems and tabs, and names that shrink before they are cut |
| `film` | `Film` — consecutive frames laid out and measured, so motion (a sprite swimming through its gait, a card gliding, a tween easing) can be judged from one image and its report |
| `fx` | `Shake`, `Popups` and `Particles` — the effects that make a hit land: screen shake that decays, numbers that pop and drift, and bursts of bits |
| `gait` | `Gait` — how a creature's frames play over time: each frame held as long as it should be, an eased beat for the body to move with, and every instance its own rate and phase from a seed, so many clones of one sprite never beat in step |
| `harness` | `harness` — the review loop a game client is built around: a fixed clock, an exact moment captured to a PNG *and* a text report of every line drawn, scripted pointers, and a window camera that is the right way up |
| `icon` | `Icon` — small icons drawn with shapes (a sword, a shield, a heart, a coin), so a stat or an effect shows without an icon font |
| `input` | `Actions` — a game's verbs (hop, fire, pause) bound to keys, swipes, taps and pad buttons, with key repeat and press buffering |
| `light` | `Lighting` — soft shadows, glows and a foil sheen, drawn by shader on a quad that turns with the thing it lights |
| `motion` | `motion` — a sprite's animation measured as it plays: how long each pose shows at 60 Hz, how far the moving part travels at each change, whether the mass holds, and the path the moving part traces, with the problems that make motion look cheap named |
| `pads` | `Pads` — gamepads by player slot, with sticks deadzoned and running y-down like the screen (feature `gamepads`) |
| `paint` | `fill_gradient` and friends — the depth a printed card has and flat fills do not: gradient plates, bevels, inner shadows, a seeded grain texture and CRT scanlines |
| `pixel` | `PixelSprite` — pixel art written as text in code, one character a pixel, baked into nearest-filtered textures: sprites with no asset files, diffable and editable by anyone |
| `plan` | `plan` — a capture plan (`--plan plan.txt`): many conditions, many captures, one launch |
| `pointer` | `Pointer` — the press, drag, long-press and tap a card game needs, built on the widgets crate's `Gesture`; `Swipes`, the four-way swipe an arcade game needs; and `HoverGrace`, which keeps a hover alive for a moment between targets |
| `preload` | `Preloader` — a cover over a screen's first frames, lifted once the screen can draw complete |
| `profile` | `profile` — where each frame's time goes, measured inside the game, in the browser as well as natively |
| `sequence` | `Sequencer` — plays queued cues one after another, each holding the stage until its animation is done |
| `shape` | `RoundedArt` — art cropped to fill a frame and clipped to its rounded corners, plus the rounded outline every card and badge needs |
| `sprite` | `Sprite` — a retained, animated display object: where a card is, how it is turned and scaled, how visible, and whether a point is on it |
| `sprite_file` | `SpriteFile` — a sprite as a text file: its rows, palette, gait, scale and body motion in one place, so a game embeds it and `sprite-view` reviews it, with no game built |
| `starfield` | `Starfield` — a seeded sky of stars in depth layers that drift and twinkle, drawn behind a space scene, a title or a whole window |
| `startup` | `Startup` — the phases a screen goes through between opening and drawing whole: settling while the window finds its size, warming glyphs behind a [`Preloader`], then live |
| `step` | `FixedStep` — a simulation that steps at a fixed rate, whatever the frame rate |
| `tech` | `TechPanel` — a workbench's panel for a tech tree, a tier at a time: tiles for the tier's nodes (built, ready, short or locked, and a server goal's fill), arrows between tiers, and a line about the picked one |
| `text` | `RichText` — wrapped text in several styles with inline icon boxes, laid out once and drawn crisp at any pixel scale |
| `thumbs` | `Thumbs` — touch controls for a phone game: the first finger down anywhere is a stick, any other tap is an activation, and a few reserved areas (a map button) take their own taps |
| `tween` | `Animated` — a value that moves towards a target over time: the tween, held by whatever owns the value |
| `viewer` | `Viewer` — a window for reviewing a game's sprites by eye, moving: one sprite looping large and at real size against guides, a crowd of its clones each on its own phase, and keys to tune the gait and body live and print the lines to paste back |
| `wall` | `Wall` — square tiles packed densely by priority, that shuffle with easing when the priorities change |
| `wallet_connection` | `Connection` — one wallet connection's lifecycle as a state machine, the same on every chain: asked, answered, refused, failed, let go |
| `wallet_modal` | `WalletModal` — a wallet connector a game opens from one button: which chain, which wallet, then what the wallet answered, on one card that glides between the steps |
| `web` | `web` — the miniquad JS plugins the kit's `audio`, `gamepads` and `discord-auth` features need in the page, and the kit's own page scripts (fullscreen on a phone), shipped with the kit and stamped by a game's `build.rs` |
