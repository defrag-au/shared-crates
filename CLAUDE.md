# shared-crates — agent notes

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

`_storybook-egui/src/lib.rs` has **six** registration sites for each story: the `Story` enum variant, `all()` (which controls ordering and grouping), `label()`, `category()`, the blurb `match`, and the `show` dispatch. Miss one and it either fails to compile or silently never appears.

`trunk build` is worth running on its own: the storybook is `crate-type = ["cdylib"]`, so `cargo build -p storybook-egui` compiles without proving the wasm target works.
