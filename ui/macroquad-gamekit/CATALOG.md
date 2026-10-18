# macroquad-gamekit catalogue

**Read this before building game-feel code.** One line per module, alphabetical,
generated from each module's own `//!` header by `tests/catalog.rs` — so it cannot drift.

Regenerate: `UPDATE_CATALOG=1 cargo test -p macroquad-gamekit --test catalog`

The companion to `macroquad-widgets` (stateless widgets): everything here holds state
across frames and takes time as an argument. Stories are in `macroquad-storybook`
under "gamekit". MACROQUAD only: no wasm-bindgen, see the shared-crates CLAUDE.md on
runtime pairs.

18 modules.

| module | what it is |
|---|---|
| `app` | `run` — the frame loop every gamekit client runs, written once: the clock, the stage, startup and the preloader, the glyph gate, captures, the frame log and the native frame cap, around a [`Game`]'s update and draw |
| `audio` | `Audio` — sound effects and one looping music track behind a single mute, plus `wav`, which wraps synthesised samples so macroquad can load them (feature `audio`) |
| `bake` | `Baked` — draw something once into a texture, then move, turn and fade the texture: how a card face is drawn every frame without laying out its text every frame |
| `ease` | `Ease` — named easing curves, so an animation says how it moves rather than writing the maths at each call site |
| `fx` | `Shake`, `Popups` and `Particles` — the effects that make a hit land: screen shake that decays, numbers that pop and drift, and bursts of bits |
| `harness` | `harness` — the review loop a game client is built around: a fixed clock, an exact moment captured to a PNG *and* a text report of every line drawn, scripted pointers, and a window camera that is the right way up |
| `light` | `Lighting` — soft shadows, glows and a foil sheen, drawn by shader on a quad that turns with the thing it lights |
| `pads` | `Pads` — gamepads by player slot, with sticks deadzoned and running y-down like the screen (feature `gamepads`) |
| `paint` | `fill_gradient` and friends — the depth a printed card has and flat fills do not: gradient plates, bevels, inner shadows and a seeded grain texture |
| `pointer` | `Pointer` — the press, drag, long-press and tap a card game needs, built on the widgets crate's `Gesture`; and `HoverGrace`, which keeps a hover alive for a moment between targets |
| `preload` | `Preloader` — a cover over a screen's first frames, lifted once the screen can draw complete |
| `sequence` | `Sequencer` — plays queued cues one after another, each holding the stage until its animation is done |
| `shape` | `RoundedArt` — art cropped to fill a frame and clipped to its rounded corners, plus the rounded outline every card and badge needs |
| `sprite` | `Sprite` — a retained, animated display object: where a card is, how it is turned and scaled, how visible, and whether a point is on it |
| `startup` | `Startup` — the phases a screen goes through between opening and drawing whole: settling while the window finds its size, warming glyphs behind a [`Preloader`], then live |
| `text` | `RichText` — wrapped text in several styles with inline icon boxes, laid out once and drawn crisp at any pixel scale |
| `tween` | `Animated` — a value that moves towards a target over time: the tween, held by whatever owns the value |
| `web` | `web` — the miniquad JS plugins the kit's `audio` and `gamepads` features need in the page, shipped with the kit and stamped by a game's `build.rs` |
