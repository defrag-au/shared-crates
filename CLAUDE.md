# shared-crates — agent notes

## Before you build a widget: READ THE CATALOGUE

- **`ui/egui-widgets/CATALOG.md`** — ~100 widgets, one line each, alphabetical.
- **`ui/macroquad-widgets/CATALOG.md`** — the macroquad set (see the runtime-pair note below).

Read the whole relevant file. Do **not** grep instead: grep only finds the name you
already guessed, which is how `IdPill` (middle-elided identifier + copy button) came to
be reimplemented, worse and inline, in a project that already depended on this crate.
A wrapped 60-character stake address shipped for weeks because nobody knew it existed.

Both files are **generated** from each module's own `//!` header by
`tests/catalog.rs`, and a test asserts the committed copy matches — the same contract
as `cargo fmt --check`. So:

- Adding a widget means giving it a `//! \`Name\` — one-line purpose.` header. The test
  **fails** on a module without one; that is deliberate, an undiscoverable widget is a
  widget that gets built twice.
- After adding or renaming, regenerate:
  `UPDATE_CATALOG=1 nix develop -c cargo test -p egui-widgets --test catalog`
  (and the same with `-p macroquad-widgets`).
- Keep that first sentence a *summary*. It is cut at the first full stop, so detail
  belongs in the paragraphs below it, where it does not bloat the index.

The two crates do not interchange — egui-widgets targets wasm-bindgen frontends,
macroquad-widgets targets miniquad, which has no wasm-bindgen glue. Runtime-specific
widgets come in pairs on purpose.

## Sizes, spacing and colour come from the THEME — enforced

`theme.rs` exists because the suite once carried ~294 inline `.size(11.0)`
literals and no way to change the type ramp, the density or the scale without
editing every one. **The vocabulary itself lives in `ui/ui-theme`** — colour
tokens, `Ink`, the colour science, and the type ramp (`TextSize`, `TextScale`,
`TextRole`, `TypeScale`). Each renderer aliases it and implements `Palette`, so
an `Ink` written on either side resolves on both. Nothing in that crate names a
renderer; a front end supplies a `Paint` impl and gets the whole model.

**Never copy a palette or a ramp into a renderer crate.** That is exactly how
the colour model came to be maintained twice, and it was measured: six colours
byte-identical, three diverged, `success` a different hue per side, and
relative luminance implemented four times in two precisions.

Four tests hold that line, in the same spirit as `cargo fmt --check`:

- **`egui-widgets/tests/theme_tokens.rs`** — no widget may write a point size
  down. `.size(…)` takes a resolved step: `ui.text_size(TextSize::Base)`. A
  config struct holds `TextSize`, not `f32`, resolved in `show()` where a `Ui`
  finally exists — `route_quote` and `pool_inspector` are the shape to copy. A
  genuinely non-textual size (a pixel dimension like `ImageStack::size(96.0)`)
  opts out with a trailing `// theme-exempt: <reason>`.
  The second half of the migration — config fields still holding `f32` — is a
  **ratchet**: the count may only go down. Convert one, lower the baseline.
- **`macroquad-widgets/tests/text_sizes.rs`** — the same rule, for a renderer
  whose size argument is positional: `p.text(s, x, y, p.size(TextSize::Sm), c)`.
  `Painter::size` / `Painter::role` are the only ways a widget gets a number.
  `Button::font_size` takes raw pixels and means "this glyph is sized from its
  own geometry" — ordinary labels want `Button::text_size`.
- **`macroquad-widgets/tests/contrast.rs`** — every preset stays readable on
  every surface, and presets vary the accent and nothing else (driven off
  `Token::ALL`, so a new token is covered the day it exists).
- **`cardano-tx/tests/miniquad_linkable.rs`** — the default-feature-free build
  reaches no wasm-bindgen, so a macroquad host can still link it. Note the
  `--target wasm32-unknown-unknown`: a native `cargo tree` is blind to this.

Clippy cannot do the first two: `disallowed_methods` matches a method PATH, not
its arguments, so it cannot tell `.size(11.0)` from
`.size(ui.text_size(…))` — and banning `RichText::size` outright would ban the
blessed form too. A source scan is exact and runs in `cargo test`.

The same reasoning applies to `Space`, `Radius` and `Breakpoint`: the point of
a named step is that one place decides what it means.

### Changing a ramp is a restyle, not a refactor

`TextScale::canvas` is the macroquad ramp and it was **derived from the
sources**, not chosen: the 73 sizes that crate used land on exactly eight
values, so those are the eight. Picking a nicer-looking ramp during the
migration would have restyled every macroquad surface under cover of a rename.
If a ramp should move, move it as its own change, with the reason written down.

## egui traps that cost an afternoon

These are catalogued modules, so the "read the catalogue" rule already covers them —
but they are the ones that look correct, compile, and are wrong at runtime:

- **A detail pane beside content: use `detail_split`, NOT a right-hand `Panel`.**
  A right `Panel` reserves its strip by shrinking the parent's `cursor.max.x`, and a
  **top-down** `Ui` never reads `cursor.max.x` (`Layout::available_from_cursor_max_rect`
  takes only `min.y` in its `TopDown` arm). The reservation is dropped, the following
  `CentralPanel` takes full width, and the pane floats over the content's right edge —
  hiding exactly the column a reader came for.
  (Names updated for egui 0.36: `SidePanel`/`TopBottomPanel` were aliases of `Panel` and
  are gone, and `show_inside` is now `show`. The **behaviour** above is unchanged —
  `available_from_cursor_max_rect` is byte-identical in 0.34.3 and 0.36.2, so the bump
  neither caused nor fixed this. Panels still want a `Ui` that is arbitrating a whole
  region, not one you are laying out yourself mid-column.)
- **`Color32` stores PREMULTIPLIED channels** — each must be `<= alpha`.
  `from_rgba_premultiplied` with larger channels blends additively and renders far
  lighter than intended. `tests/contrast.rs` asserts this for the theme; the `theme_states`
  story shows the interaction states a resting-state story cannot.
- **Images load when the widget is BUILT, not when it is drawn.** `ui.add(Image::new(url))`
  in a long list starts a fetch for every row, including those below the fold. Reserve the
  space, then gate on `ui.is_rect_visible` — see `activity_feed`.
- **`Ui::set_max_width` WIDENS a `Ui` that has less room.** It assigns `max_rect.max.x`
  outright rather than taking a minimum, so `set_max_width(520.0)` inside a 342pt phone
  lays out at 520 and overflows off both edges. `AccessGate` shipped like this — the one
  screen whose job is explaining how to get in was clipped on every phone. Use
  `viewport::fit(ui, 520.0)`.
- **An overflowing `ui.horizontal` does not just clip — it widens the parent**, and
  everything drawn *after* it inherits the inflated width. A legend row running 80pt long
  took a 180pt-tall scatter plot off-screen with it, and made the wrapping paragraphs
  below stop wrapping. When a row might not fit, it is `horizontal_wrapped`.
- **egui repaints ON DEMAND, so an async result can sit unread indefinitely.** A fetch
  that completes on a JS callback and pushes to an `mpsc` wakes nothing; the channel is
  only drained on the next frame. On a desktop the first mouse move hides this completely.
  On a phone nothing moves — collection-ownership's index sat on "Loading collections…"
  forever with the response already in memory. Either wake the `Context` at the point of
  send, or tick `request_repaint_after` while work is outstanding — and key that tick off
  an explicit *pending* flag, never off `data.is_none()`, which is also what failure looks
  like and will spin forever on battery.
- **Compact-breakpoint touch sizing inflates non-interactive content too.**
  `spacing.interact_size.y = 44` is a floor on allocated space *and* sets row height in
  `horizontal`/`horizontal_wrapped`, so read-only status chips came out as 34pt squares
  around 10pt text. Opt a dense region out with
  `ui.spacing_mut().interact_size = Vec2::ZERO` — on the region, not inside the chip,
  because by then the row height is already decided.

## Toolchain access

Rust toolchain (cargo, clippy, rustfmt, wasm targets, wrangler, node, aiken) is provided **only** inside the Nix devshell defined in `flake.nix`. `cargo` is not on `$PATH` outside the shell.

To run cargo from an agent shell:

```sh
nix develop --command cargo build -p <crate>
nix develop --command cargo test  -p <crate>
nix develop --command cargo clippy --workspace --all-targets -- -D warnings
```

Or, since `.envrc` uses `use flake`, run via direnv if `direnv` is on `$PATH`:

```sh
direnv exec . cargo build -p <crate>
```

Do **not** install a separate cargo toolchain — the devshell pins the channel so this repo stays in lock-step with `mitos` and `cnft.dev-workers`.

## Looking at egui widgets (do this — don't ship a widget unseen)

Every widget in `ui/egui-widgets` has a story in `ui/_storybook-egui`. The storybook builds to wasm and can be screenshotted headlessly, so **a new or changed widget must be rendered and looked at before it is reported as done.** Unit tests do not catch layout, and they do not catch a default that every test case happens to share.

```sh
# 1. serve (background). trunk is only inside the devshell.
cd ui/_storybook-egui
nix develop ../.. -c trunk serve                 # http://127.0.0.1:8095 (Trunk.toml)

# 2. screenshot one story. Brave is Chromium — nothing extra to install.
BRAVE="/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"
"$BRAVE" --headless=new --disable-gpu --use-gl=swiftshader --enable-unsafe-swiftshader \
  --hide-scrollbars --window-size=1400,620 --virtual-time-budget=8000 \
  --screenshot=".tmp/flow-ledger.png" "http://127.0.0.1:8095/#/flow-ledger"
```

- **Write screenshots to `.tmp/` in the repo**, not `/tmp` — they stay easy to open and to point someone at. `.tmp/` is gitignored.
- `--use-gl=swiftshader --enable-unsafe-swiftshader` is **required**: egui renders to a WebGL canvas and headless has no GPU. The `GPU stall due to ReadPixels` lines on stderr are noise, not failure.
- `--virtual-time-budget=8000` lets wasm boot and the remote font fetch settle. Too low gives a blank canvas.
- Size the window to the content; the sidebar is ~180px.

### Narrow / mobile widths — `--window-size` CANNOT do this

Chromium headless **floors the window near 620px** (measured on this machine, 2026-08-29), lays out at the floor and then **crops the capture** to whatever width you asked for. A "390px" shot is therefore a wide layout with its right edge sliced off — which reads as an overflow bug that is not there, and hides the real one. Two things are needed:

1. **`?nav=0`** drops the storybook sidebar, so the story gets the whole viewport instead of `viewport − 180`.
2. **CDP `Emulation.setDeviceMetricsOverride`** sets a real layout viewport. Helper: `ui/_storybook-egui/tools/cdp-shot.mjs`; Node has a global `WebSocket`, so there is nothing to install.

```sh
BRAVE="/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"
"$BRAVE" --headless=new --disable-gpu --use-gl=swiftshader --enable-unsafe-swiftshader \
  --hide-scrollbars --remote-debugging-port=9222 --user-data-dir=/tmp/brave-cdp-profile \
  about:blank &
node ui/_storybook-egui/tools/cdp-shot.mjs \
  "http://127.0.0.1:8095/?nav=0#/activity-feed" 390 844 .tmp/narrow.png
```

`--user-data-dir` is required alongside `--remote-debugging-port` or Brave refuses to expose the port. The same helper points at a **deployed app** (pass a longer settle for socket + first payload), which is the only way to check a widget against real data at real width.

### Let the marks carry it — egui is weak at prose

egui has no real text shaping and poor typographic hierarchy, so **blocks of text are the wrong tool**. If a widget is explaining itself in paragraphs, the design is wrong, not the copy. Reach for an encoding instead:

- state → a pip track / progress marks, not a sentence
- composition → shaped or coloured marks (see `PartyBadge`'s filled/half/hollow basis language, reused as support pips on `ClaimCard`)
- magnitude → bar height or width, never a number the reader has to compare by eye
- long-form detail → behind an expand, on hover, or in a side panel

The test: a list of twenty of these should be **scannable**. If reading twenty means reading twenty paragraphs, redesign. Keep at most one line of irreducible text (a title, a statement) and put the rest on demand. Story captions in the storybook are held to the same standard — one or two short lines, not an essay.

### Story deep links

Stories are addressable as `#/<slug>`, where the slug is derived from the story's `label()` — so a new story is linkable with no extra registration. `#/party-badge`, `#/flow-ledger`, `#/stat-strip`. Clicking in the sidebar updates the hash, so a URL you copy matches what you are looking at. Native builds have no address bar; use `STORYBOOK_STORY=flow-ledger` instead.

### Adding a story

`_storybook-egui/src/lib.rs` has **three** registration sites for each story: the `stories! { … }` entry (one line, which generates the enum variant, the sidebar ordering, the group heading and the render dispatch), `label()`, and the blurb `match` — plus `pub mod` in `stories/mod.rs`. Miss one and it either fails to compile or silently never appears.

(This said "six" until the `stories!` macro landed; see `src/registry.rs` for what was broken before. `label()` and the blurb stay hand-written on purpose — they are exhaustive matches, so the compiler already catches an omission, and moving ~130 prose strings would risk pairing one with the wrong story for no safety gain.)

`trunk build` is worth running on its own: the storybook is `crate-type = ["cdylib"]`, so `cargo build -p storybook-egui` compiles without proving the wasm target works.
