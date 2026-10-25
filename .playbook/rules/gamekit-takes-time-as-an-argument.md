---
id: shared-crates-gamekit-takes-time-as-an-argument
title: macroquad-gamekit never reads the clock, and every module has a catalogue line
layer: project
activation: project:shared-crates
priority: 50
overrides:
targets:
---

## Directive

**Nothing in `gaming/macroquad-gamekit` outside `harness` calls `get_time`, `get_frame_time` or
`Instant`.** Time is a parameter (`update(dt)`, `value(t)`, `phase`), owned by the game. The
harness is the one place that reads the wall clock: `frame_dt` chooses between the fixed capture
clock and the real one, and `FrameTimer` times a frame for `--frame-log` and caps the native loop.
The frame loop in `app::run` uses both and hands a `Game` only `dt`. A work limit that would be
natural in milliseconds is a count instead (`flush_glyphs(Some(sizes))`), so a capture does the
same work on the same frame on every machine.

- A new module gets a `//!` header whose first line is its catalogue line, then
  `UPDATE_CATALOG=1 nix develop -c cargo test -p macroquad-gamekit --test catalog`.
- A visual module gets a story in `macroquad-storybook` under "gamekit".
- Text goes through `macroquad-text` — `TextLayout::draw` in the kit, `draw_run` under the
  widgets' `Painter` — so the harness report sees it and the glyph gate covers it. A module that
  calls `draw_text_ex` itself is invisible to every reviewer that reads the report, and re-uploads
  the font atlas mid-frame.

## Rationale

Capturing an exact moment (`--at 1.25`) is what made the card pass reviewable, and it only works
because no animation reads the clock: the fixed 1/60 s step makes 1.25 s the same frame every run.
One `get_time()` in a tween would make captures irreproducible and its tests timing-dependent.
