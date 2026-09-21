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

<!-- BEGIN agent-playbook (project: shared-crates, target: claude-code) — generated, do not edit -->

# Agent rules — shared-crates

Generated from agent-playbook (`projects/shared-crates` + `models/claude-code`).
Rule sources live under `/Users/damo/code/defrag/agent-playbook` — each heading's HTML comment names its file there.
To change a rule, change the rule — not this block.

## Non-negotiable

The lead of each rule below, repeated so it is read first. Full text follows in place.

- **Working first — fix problems, don't hide them** — Deliver working functionality before optimising architecture. Fix the cause of a problem; never comment it out, disable it, or route around it to get a build green.
- **Never invent data to satisfy an interface** — Never fabricate values to make a function, a fixture or a screen look functional.
- **Tests are the specification — never edit an assertion to make it pass** — **Never modify a test assertion without explicit permission.**
- **Do not report success you have not observed** — Never claim something works, passes, builds or renders unless you ran it and saw the result in this session.

## Do not report success you have not observed
<!-- rule: rules/core/verify-before-claiming -->

Never claim something works, passes, builds or renders unless you ran it and saw the result
in this session.

- "Tests pass" means the test command ran and its output showed them passing. Not "the change
  looks correct".
- "It builds" means a build ran. A successful edit is not a successful build.
- "The widget renders correctly" means it was rendered and looked at — see
  `org/defrag/look-at-what-you-built`.
- "Fixed" means the reported symptom was reproduced and then observed to be gone.

If validation was not run, say so plainly and say why — no toolchain reachable, needs a
device, needs credentials. That is a useful answer. An unearned "done" is worse than a
failure report, because it moves the discovery of the failure to me.

### Related failure modes

- **Reporting a partial result as complete.** If three of four things were done, say which
  three.
- **Silently skipping a step.** A step that could not be run must be named in the final
  message, not dropped.
- **Treating a green exit code as evidence.** A command that exits zero while doing nothing —
  a `str.replace` that found no anchor, a test filter that matched no tests — is not
  validation. Check that the output says what you think it says.
- **Extrapolating from a narrower run.** A single-crate build is not a workspace build.


## Tests are the specification — never edit an assertion to make it pass
<!-- rule: rules/core/test-preservation -->

**Never modify a test assertion without explicit permission.**

When behaviour is reported as wrong, the fix goes in the code. The test says what the system
is supposed to do; if the fix makes the test fail, the fix is wrong.

1. Fix the code to produce correct behaviour.
2. Leave the existing assertions alone.
3. If tests still fail, the fix is still wrong.
4. Tests are the specification, not an obstacle.

### Never

- Change an assertion so it matches the behaviour the code currently has
- Weaken a test when fixing a bug — loosening a bound, widening a tolerance, adding an early
  return
- Remove specificity: `assert_eq!(sales, vec![expected_sale])` becoming
  `assert!(!sales.iter().any(|s| s.is_false_positive()))` throws away the thing being tested
- Delete, `#[ignore]`, or comment out a failing test
- Optimise for "the suite is green" instead of "the system is correct"
- Add a test that asserts what the code currently does, then call the behaviour specified

### Permitted without asking

- **Adding** a test for behaviour that had none
- Fixing a test that cannot compile because a signature legitimately changed — but the
  assertion it makes must survive the change
- Renaming a test to describe what it checks

### If the test is genuinely wrong

Say so explicitly, explain why the assertion is incorrect, and **get sign-off before
changing it**. "This test encodes a bug" is a legitimate finding. Silently rewriting the
assertion to match the bug is not. The difference is whether the change is visible and
argued.


## Working first — fix problems, don't hide them
<!-- rule: rules/core/working-first -->

Deliver working functionality before optimising architecture. Fix the cause of a problem;
never comment it out, disable it, or route around it to get a build green.

When facing a compilation error, a lifetime fight, or a design question, the first question
is **"what is the simplest thing that makes this actually work?"** — not "what is the correct
architecture for this?". Prove the concept end-to-end, then iterate.

### The two paths

**Fix and prove.** Identify the root cause. Implement the minimal working solution. Verify it
end-to-end. *Then* improve it.

**Hide and perfect.** Comment out the code that will not compile. Fight an async or lifetime
problem before the basic logic is proven. Optimise something that has never run. Prioritise
"it builds" over "it works".

The second path is faster for about ten minutes and then costs a session.

### Red flags — stop and reconsider

- Adding a `// TODO:` to disable functionality that was supposed to work
- Commenting out code to silence a compilation error
- "We'll implement that later" applied to a core feature rather than an edge
- Fighting type/lifetime/async issues before the plain logic is proven
- Spending longer on the shape of the code than on whether the feature works
- Rewriting a signature to make an error go away rather than understanding it

### Green lights — keep going

- The user can exercise the feature end-to-end right now
- Core functionality works, even if the implementation is plain
- Each change leaves the working state working
- Problems are being solved rather than hidden
- Value is demonstrable this session, not next session

### Exceptions

Deviate only when continuing would break something that currently works, would introduce a
security hole, or would risk data corruption. Even then: fix it properly. Do not comment it
out and do not disable it.

**Make it work, make it right, make it fast — in that order.**


## Never invent data to satisfy an interface
<!-- rule: rules/core/never-make-things-up -->

Never fabricate values to make a function, a fixture or a screen look functional.

If an interface needs data, **find out where that data actually comes from before writing a
placeholder.** The source almost always exists — a config file, a table, an API response, a
sibling implementation, an environment variable. Look for it. If a genuine search turns up
nothing, ask where it should come from rather than inventing it.

This applies to more than literals:

- **Sample data in a fixture** — a realistic-looking address, hash or timestamp that was
  invented rather than captured. It hides real parsing bugs, because invented data is
  already in the format the code expects. The interesting inputs are the ones an external
  source actually sends.
- **A default that looks like a decision** — `unwrap_or(0)`, `Default::default()`, a
  hard-coded fallback. A silent default on a value that should have been sourced is
  fabricated data wearing a type.
- **A plausible API shape** — writing a struct from memory of what an API "should" return,
  then deriving the parser from it. Cite the actual response or the actual docs.
- **A test that asserts what the code currently does** — that is inventing a specification
  to match an implementation.
- **An explanation of why something is broken** — a confident mechanism you have not
  verified. "It's probably a caching issue" is a made-up value in prose form.

### The distinction that matters

Making things up is not the same as *choosing*. Choosing a sensible default and stating it
is fine. Inventing a value and presenting it as data is not. The test: if someone later asks
"where does this number come from?", is there an answer?

If the answer is "nowhere yet", say so plainly and ask.


## Build what was asked, then stop
<!-- rule: rules/core/dont-rush-new-features -->

If I asked for X, build X and stop.

Do not assume Y and Z are wanted and build them too. Do not add a config option "for
flexibility", a trait "for testability", or an abstraction "for later" unless the task named
it. Plan each logical step of implementation together, one at a time.

Scope creep is not generous, it is expensive: it enlarges the diff I have to review, hides
the change I actually asked for, and usually encodes a guess about a requirement that was
never stated. When the guess is wrong, the cost of removing it is higher than the cost of
never adding it.

### The only three exceptions

- The task cannot be completed without it — a caller that must be updated to keep the
  workspace compiling, a type that must exist for the requested feature to typecheck.
- Not doing it would break something that currently works.
- I explicitly asked you to use your judgement.

Everything else: **mention it and let me decide.** A one-line "this would also allow X if you
want it next" is welcome. Building X uninvited is not.

### Applies to cleanup too

Renaming things, reordering imports, tidying a neighbouring function, upgrading a dependency
"while I'm here" — all of it is scope. If a nearby thing is genuinely broken, say so in the
final message rather than fixing it silently.


## Ask before changing dependencies or architecture
<!-- rule: rules/core/conservative-package-changes -->

Before making a change that alters the *approach* rather than the *implementation*, stop and
ask. Present the problem, offer two or three specific options with their trade-offs, and
wait for an answer.

### Triggers — ask first

- Editing `Cargo.toml`, `package.json`, `flake.nix`, or any other manifest
- Adding, removing or bumping a dependency
- Swapping one library for another (`smlang` → `rust-fsm`, `reqwest` → `ureq`)
- Replacing a whole module or implementation rather than fixing it
- Choosing a pattern — state machine style, database access layer, error strategy, where a
  type lives
- Anything that changes how the project is built, deployed or configured

### Does not trigger

- A bug fix that stays inside the existing approach
- A direct instruction ("change X to Y") — that is already a decision
- Formatting, lint fixes, renames local to one function

### Why

These are the decisions that are cheap to make and expensive to unmake. A dependency is a
supply-chain commitment, a build-time cost, and a future upgrade obligation. A framework
choice reshapes every file that follows it. I usually have context you do not — a reason the
current choice was made, a constraint from elsewhere in the ecosystem — and I would rather
spend one message than one refactor.

The options you offer should be *specific*: name the crates, say what each costs. "We could
use a library or write it ourselves" is not a set of options.


## Git history is mine — do not commit, push, merge or branch
<!-- rule: rules/core/git-is-the-users-domain -->

Do not run `git commit`, `git push`, `git merge`, `git rebase`, `git tag`, or any other
command that mutates history or a remote, unless I specifically ask for it.

Making file edits is the job. **Committing is not** — leave changes staged or unstaged for
me to review and commit myself. I want to see the diff before it becomes history, and I
usually want to write the message.

Read-only git is always fine: `git status`, `git diff`, `git log`, `git show`, `git blame`.
Use `--no-pager` on all of them.

Branching for work is fine when it helps. Creating a branch is not rewriting anything I
have to unpick.

### Also

- Do not add `[skip ci]`, `--no-verify`, or bypass hooks without asking
- Do not amend, force-push, or reset anything
- Do not commit generated artefacts that `.gitignore` excludes — if `dist/` is ignored, it
  stays uncommitted even when it is up to date
- Do not write a commit message and leave it staged in a way that suggests it was committed


## Edit files with the editor tools, never with a shell script
<!-- rule: rules/core/edit-via-editor-tools -->

Edit files with the read/edit/write tools. Never shell out to `python`, `sed`, `awk`,
`perl`, `truncate`, or a heredoc to modify a file — not for a big change, and not for "just
one small change".

This applies to every file: source, config, docs, rules, memory.

Multi-edit convenience scripts are included. If a change touches ten places, that is ten edit
calls, not one script.

### Why

- The editor tools verify the file was read first and **fail loudly** on an ambiguous or
  stale match.
- They show a reviewable diff.
- A script's `str.replace` **silently does nothing** when the anchor text has moved.

That last one is not hypothetical. A write was reported as done after `cargo fmt` reflowed
the anchor it was matching against — the script found nothing, wrote nothing, exited zero,
and the change was never made. The failure was invisible until much later. Editor tools make
that class of bug impossible.

### What is still fine

Generating a file's *content* with a script is fine when the content is genuinely computed —
a catalogue built from source headers, a table derived from data. Writing it to disk still
goes through the write tool.

Reads through the shell are fine too: `cat`, `grep`, `find` to gather information, then edit
with the editor tools.


## Typed structs only — no serde_json::Value, no json! macro
<!-- rule: rules/core/typed-json-only -->

Never use `serde_json::Value` or the `serde_json::json!()` macro for data structures, API
responses, configuration, queue messages, SSE payloads, or anything persisted.

Define concrete types with `#[derive(Serialize, Deserialize)]`.

### Banned

- `serde_json::Value`, `json!()`
- `Map<String, Value>`, `Vec<Value>`, any composition of `Value`
- `#[serde(untagged)]` as a way to avoid deciding on a shape

### Required

- Concrete structs and enums with serde derives
- `#[serde(tag = "type")]` for tagged unions rather than a stringly-typed discriminant field
- `Option<T>` for nullable fields, newtypes for ids and timestamps
- `#[serde(flatten)]` where a shape genuinely composes

```rust
// no
let event = json!({ "type": "progress", "message": msg, "pct": pct });

// yes
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum CatchupEvent {
    Progress { message: String, pct: u8 },
    Complete { assets: u32 },
}
```

### Limited exceptions

Allowed only with a justification written in the code:

1. Consuming a genuinely dynamic external API whose schema cannot be known
2. Temporary debugging code that will be removed
3. A low-level JSON utility whose whole purpose is handling arbitrary JSON

### Why

Type safety catches the error at compile time instead of in production. Structs document the
shape. The frontend can deserialise into a matching type instead of indexing into a map.
Schema generation works. And when the shape changes, the compiler finds every call site
rather than the runtime finding one.


## Plan in reasoning, not in the response
<!-- rule: rules/core/planning-stays-in-thinking -->

I see the answer and the diff. I do not need the plan for getting there.

Do not narrate what you are about to look up — look it up. Do not open with a summary of
what you intend to do and then do it; the summary is only useful if the work fails, and if
it fails you can explain then.

### What this rules out

- "I'll start by reading X, then check Y" as a preamble to reading X and checking Y
- Restating the request before acting on it
- Announcing each step as you take it ("Now let me look at the config…")
- A plan-shaped response where a tool call was the answer

### What it does not rule out

- A genuine plan when the task is large enough that I should agree to the approach first —
  that is a decision I need to make, not narration
- One sentence before a group of related tool calls, so I can follow what is happening
- Explaining reasoning in the final message when it affects whether I trust the result

### On nudges to think first

Some harnesses inject a reminder to "first privately list what you need next", or similar.
That means: list it *in your reasoning*, then batch the independent tool calls in one
response. It is an instruction about batching, not a phrase to emit. Never begin a response
with "Privately," or any variant of it.


## Never commit a secret
<!-- rule: rules/core/no-committed-secrets -->

No credential, token, private key, connection string or API key goes into the repository — not
in source, not in a test fixture, not in a config file, not in a commit message, not in a
comment explaining what the value used to be.

If a change needs a secret to run, the secret comes from the environment or a secrets manager,
and the repository holds only the *name* of the variable and an `.env.example` showing its
shape with an obvious placeholder.

```
## .env.example — committed
KOIOS_API_KEY=your-key-here

## .env — gitignored
KOIOS_API_KEY=<real value>
```

### When you need one to test something

Say so and stop. Do not invent a placeholder that looks real, and do not reach for a real key
that is already in the environment — a test that only passes with a live credential is not a
test, it is a scheduled failure.

### If you find one already committed

Report it. Do not quietly delete the line and move on: the value is in the history, so deleting
it from the working tree does not revoke it. **The secret has to be rotated**, and that is a
decision for me, not a cleanup you perform silently.

### The related failure

A secret pasted into a code comment or a `// TODO: replace before merge` is the same leak with
an expiry date that nobody enforces.


## Rust tooling lives behind the Nix devshell
<!-- rule: rules/rust/devshell-first -->

`cargo`, `rustc`, `clippy`, `rustfmt`, `trunk`, `wrangler`, `node` and the wasm targets are
**not on `PATH`**. Wrap every command:

```sh
nix develop -c cargo build --workspace
nix develop -c cargo test -p <crate>
nix develop -c cargo clippy --workspace --all-targets --all-features -- -D warnings
nix develop -c cargo fmt
```

`nix develop --command <cmd>` is equivalent.

**In a sandboxed shell, `nix develop` cannot reach the daemon socket** —
`cannot connect to socket … Operation not permitted`. Use direnv instead, which reads the
already-realised devshell out of `.direnv/` and needs no daemon:

```sh
direnv exec . cargo build -p <crate>
```

If `.direnv/` is cold, run `direnv allow .` once from an unsandboxed shell.

Never: install a separate toolchain, source `~/.bash_profile` to find cargo, or reach for the
network. Add `--offline` if cargo tries to fetch a crate the devshell cache already holds.


## Build often — the compiler is the best information you have
<!-- rule: rules/rust/build-often -->

In a Rust project, building is the fastest way to find out what is wrong.

The compiler knows more about the code than you can work out by reading it. It resolves every
path, checks every type, and reports every mismatch with a location and a suggestion. That is
strictly better information than reasoning about whether a reference is still valid, or
searching the tree for a call site that may not exist.

So: after a change that could plausibly break a type, **compile it**. Do not read your way to
confidence through three files when one `cargo check` answers the question in seconds.

### Practically

- `nix develop -c cargo check` while iterating; `cargo build` when you need artefacts
- Scope it when the workspace is large: `-p <crate>`
- `cargo clippy` for the lints the compiler does not carry — see
  `rust-tooling-handles-grunt-work`
- When a build fails, **read the whole error before editing.** Rust errors cascade: the first
  one is usually the real one and the rest are consequences. Fixing a consequence first
  produces a second round of errors that look new.

### The exception

Do not build when you have already established that the build is expensive and the change
cannot affect it — editing a markdown file does not need a compile. Use judgement about what
a change can reach.


## Inline format arguments everywhere
<!-- rule: rules/rust/inline-format-args -->

Always use inline format arguments. Clippy warns on the alternative, and the lint is baked
into the lint command as an error.

```rust
// no
format!("Hello {}", name)
println!("wrote {} rows to {}", count, path)

// yes
format!("Hello {name}")
println!("wrote {count} rows to {path}")
```

Applies to every formatting macro: `format!`, `println!`, `eprintln!`, `write!`,
`writeln!`, `panic!`, `assert!`, `assert_eq!`, `debug_assert!`, `tracing` macros.

### When writing new code

Use inline args from the first draft. Do not write `{}` and expect a later pass to catch it —
that pass is the one that costs a round trip.

### When editing existing code

Convert to inline args in the lines you are already touching. Do not sweep the file: an
unrelated formatting change buries the diff I asked for. If a file is broadly non-compliant,
let `clippy --fix` handle it as its own change — see
`rust-tooling-handles-grunt-work`.


## Let fmt and clippy do the mechanical work
<!-- rule: rules/rust/tooling-handles-grunt-work -->

Formatting and mechanical lint fixes are tool work. Do not spend a turn hand-aligning code or
typing out the fix for a lint the tool will apply itself.

```sh
nix develop -c cargo fmt
nix develop -c cargo clippy --fix --allow-dirty --all-targets --all-features -- -D warnings
```

Run them, then **read what changed** and check the remaining diagnostics manually. The
ordering matters: `--fix` first, then review, because the automated pass changes the lines
you were about to read.

### `-D warnings` is not optional

The lint command treats warnings as errors. A warning is a failure, not a note. That is
deliberate — this codebase does not accumulate "known warnings", because a suite with a
hundred warnings cannot show you the one that matters.

### Where the tooling stops

`--fix` only applies suggestions that are mechanically safe. It will not:

- Choose between two valid shapes
- Know that a `disallowed_methods` match is a false positive on a blessed call
- Fix anything requiring a semantic decision

Some rules in this playbook cannot be enforced by clippy at all — a source scan in
`cargo test` is the tool for those, in the same spirit as `cargo fmt --check`. When you add a
rule of that kind, add the test with it.


## Where tests live, and what they may touch
<!-- rule: rules/rust/test-layout -->

- **Unit tests beside the code**, in a `mod tests` block in the same file. They can reach
  private items, which is usually the point.
- **Integration tests in `tests/`**, one file per scenario. They exercise the public API only.
- **Runnable examples in `examples/`** when they clarify the API — `cargo run -p <crate>
  --example <name>` is the test that a consumer can actually use the thing.
- **WASM tests use `wasm-bindgen-test`** and need a WASM-capable runner. They do not run under
  plain `cargo test`, so a green suite is not evidence that a WASM test passed.
- **Tests are deterministic.** No network calls unless they are feature-gated or mocked. A test
  that depends on a live API is a test that fails for reasons unrelated to the change.

### Why the last two matter most

They are the two ways a suite reports success without having checked anything. A `wasm-bindgen`
test that silently does not run, and a network test that passes because the fixture is cached,
both look identical to a passing test — and both mean the same thing when a real bug ships.

### Reusable fixtures

Capture a fixture in the shared `data/` crate when a second crate needs it, rather than copying
it. Shared serialization fixtures live in `test_datum_serialization.rs`.


## Read the widget catalogue before building any UI
<!-- rule: rules/org/defrag/widget-catalogue-first -->

**Before writing any widget, read the catalogue.**

- `~/code/defrag/shared-crates/ui/egui-widgets/CATALOG.md` — ~100 widgets, one line each
- `~/code/defrag/shared-crates/ui/macroquad-widgets/CATALOG.md` — the macroquad set

Read the whole file. **Do not grep instead.** Grep only finds the name you already guessed,
which is exactly how the duplicate got built: `IdPill` — middle-elided identifier plus a copy
button — was reimplemented inline, and worse, in a project that already depended on the crate
containing it. A wrapped 60-character stake address shipped for weeks because nobody knew it
already existed.

If you are about to hand-build something widget-shaped — a chip, an id display, a
label-value grid, a card, a chart, a stat strip — scan first. It probably exists.

### If a new widget is genuinely warranted

Build it in `egui-widgets` with a `//!` header, add a storybook story, and regenerate the
catalogue. The three-step checklist is at the bottom of `WIDGETS.md`.

### Both catalogues are generated

From each module's own `//!` header, by `tests/catalog.rs`, and a test asserts the committed
copy matches — the same contract as `cargo fmt --check`. Adding a widget means giving it a
`//! \`Name\` — one-line purpose.` header; the test **fails** on a module without one, and
that is deliberate. An undiscoverable widget is a widget that gets built twice.

```sh
UPDATE_CATALOG=1 nix develop -c cargo test -p egui-widgets --test catalog
```

Keep the first sentence a summary — it is cut at the first full stop, so detail belongs in
the paragraphs below, where it does not bloat the index.


## Toggle local [patch] blocks as a unit
<!-- rule: rules/org/defrag/patch-blocks -->

When engaging a local `[patch]` override block in `Cargo.toml` — to consume a sibling working
tree instead of the pinned git rev — **uncomment the entire block as a unit.** Never
selectively un-comment individual entries.

Selective edits cause two failures:

- A line ends up in both the active and the still-commented copy → duplicate-key error
- A transitively-required crate is left commented → unrelated workers break

Toggling the whole `[patch."…"]` table on and off is the only safe shape.

The same applies in reverse: when re-commenting before commit, re-comment the whole block.
Never leave a partial active block behind.

### Why it is worth a rule

The failure is never at the patch site. It shows up as a build error in a crate you did not
touch, or as a duplicate-key error whose line number points at the copy you did not mean to
edit. Both cost more to diagnose than the block took to toggle.


## Sizes, spacing and colour come from the theme
<!-- rule: rules/org/defrag/theme-tokens-only -->

No widget writes a point size down. No renderer crate copies a palette or a ramp.

### Sizes resolve through the ramp

```rust
// no
ui.label(RichText::new("total").size(11.0));

// yes
ui.label(RichText::new("total").text_size(ui.text_size(TextSize::Base)));
```

A config struct holds a `TextSize`, not an `f32`, and resolves it in `show()` where a `Ui`
finally exists — `route_quote` and `pool_inspector` are the shapes to copy.

A genuinely non-textual size (a pixel dimension, e.g. `ImageStack::size(96.0)`) opts out with
a trailing `// theme-exempt: <reason>`.

### The vocabulary lives in one place

`ui/ui-theme` owns colour tokens, `Ink`, the colour science and the type ramp. Each renderer
aliases it and implements `Palette`, so an `Ink` written on either side resolves on both.
Nothing in that crate names a renderer.

**Never copy a palette or a ramp into a renderer crate.** That is how the colour model came
to be maintained twice, and it was measured: six colours byte-identical, three diverged,
`success` a different hue per side, and relative luminance implemented four times in two
precisions.

The same reasoning applies to `Space`, `Radius` and `Breakpoint`: the point of a named step
is that one place decides what it means.

### Why the tests exist

`egui-widgets/tests/theme_tokens.rs` and `macroquad-widgets/tests/text_sizes.rs` are source
scans. Clippy cannot do this: `disallowed_methods` matches a method *path*, not its
arguments, so it cannot tell `.size(11.0)` from `.size(ui.text_size(…))` — and banning
`RichText::size` outright would ban the blessed form too. A source scan is exact and runs in
`cargo test`.

The `f32` config fields are a **ratchet**: the count may only go down. Convert one, lower the
baseline.

### Changing a ramp is a restyle, not a refactor

`TextScale::canvas` was **derived from the sources**, not chosen — the 73 sizes that crate
used land on exactly eight values, so those are the eight. Picking a nicer-looking ramp
during a migration would have restyled every macroquad surface under cover of a rename. If a
ramp should move, move it as its own change, with the reason written down.


## Keep logic pure — runtime bridges come in pairs
<!-- rule: rules/org/defrag/runtime-pairs -->

**wasm-bindgen code can never run under macroquad/miniquad.** The miniquad `gl.js` runtime has
no wasm-bindgen glue; browser interop goes through the miniquad plugin protocol
(`sapp_jsutils`) instead. This is permanent, not a version issue.

Runtime-specific bridges therefore come in pairs: `wallet-core` for wasm-bindgen frontends,
`wallet-miniquad` for macroquad games. egui-widgets and macroquad-widgets do not interchange
for the same reason.

### The consequence for structure

**Keep logic in small pure crates** — no wasm-bindgen, no I/O, no runtime deps — so the same
crate can be consumed from a macroquad game, a wasm-bindgen frontend (Leptos) **and** a
Cloudflare worker. Push runtime bindings to thin edge crates.

Macroquad targets demand more discipline here than other runtimes: check a dependency's
transitive tree for wasm-bindgen before adding it to a macroquad game.

```sh
## a native cargo tree is blind to this — the target flag is required
nix develop -c cargo tree --target wasm32-unknown-unknown -p <crate>
```

`cardano-tx/tests/miniquad_linkable.rs` asserts the default-feature-free build reaches no
wasm-bindgen, so a macroquad host can still link it.

### For a WASM build, audit the feature flags

Minimise the features enabled on a wasm32 target. Default features are chosen for a native
build, so a wasm consumer inherits capabilities it cannot use — and a feature that pulls in a
system library is a build failure at link time, not at the crate that introduced it. Check the
transitive tree rather than the direct dependency:

```sh
nix develop -c cargo tree --target wasm32-unknown-unknown -p <crate>
```


## Render it and look at it before reporting it done
<!-- rule: rules/org/defrag/look-at-what-you-built -->

Every widget in `ui/egui-widgets` has a story in `ui/_storybook-egui`. **A new or changed
widget must be rendered and looked at before it is reported as done.**

Unit tests do not catch layout. They do not catch a default that every test case happens to
share. They do not catch a panel floating over the content it was supposed to sit beside. A
widget that compiles and passes its tests can still be unusable on the screen it was built
for.

This applies to narrow widths too, which is where most of the real failures live — see the
`widget-screenshot` skill for the exact
invocation, including why `--window-size` cannot produce a mobile shot.

### Do not report a widget as done on the strength of

- It compiles
- Its tests pass
- The storybook entry renders without a panic
- It looks right in the code

### Related trap

The same reasoning applies to a *deployed* app: the helper points at a real URL, which is the
only way to check a widget against real data at real width. Real data is wider than fixture
data — that is how the wrapped stake address shipped.


## egui layout traps that cost an afternoon
<!-- rule: rules/org/defrag/egui-layout-traps -->

These are catalogued modules, so `defrag-widget-catalogue-first`
already covers *finding* them — but these are the ones that look correct, compile, and are
wrong at runtime. Read this before laying out a panel, sizing a `Ui`, or wiring an async
result into a widget.

### A detail pane beside content: use `detail_split`, not a right-hand `Panel`

A right `Panel` reserves its strip by shrinking the parent's `cursor.max.x`, and a **top-down**
`Ui` never reads `cursor.max.x` (`Layout::available_from_cursor_max_rect` takes only `min.y` in
its `TopDown` arm). The reservation is dropped, the following `CentralPanel` takes full width,
and the pane floats over the content's right edge — hiding exactly the column a reader came for.

Panels want a `Ui` that is arbitrating a whole region, not one you are laying out yourself
mid-column.

### `Color32` stores PREMULTIPLIED channels

Each channel must be `<= alpha`. `from_rgba_premultiplied` with larger channels blends
additively and renders far lighter than intended.

`egui-widgets/tests/contrast.rs` asserts this for the theme. The `theme_states` story shows the
interaction states a resting-state story cannot.

### Images load when the widget is BUILT, not when it is drawn

`ui.add(Image::new(url))` in a long list starts a fetch for every row, including those below
the fold. Reserve the space, then gate on `ui.is_rect_visible` — `activity_feed` is the shape
to copy.

### `Ui::set_max_width` WIDENS a `Ui` that has less room

It assigns `max_rect.max.x` outright rather than taking a minimum, so `set_max_width(520.0)`
inside a 342pt phone lays out at 520 and overflows off both edges. `AccessGate` shipped like
this — the one screen whose job is explaining how to get in was clipped on every phone. Use
`viewport::fit(ui, 520.0)`.

### An overflowing `ui.horizontal` widens the parent

It does not just clip: everything drawn *after* it inherits the inflated width. A legend row
running 80pt long took a 180pt-tall scatter plot off-screen with it, and made the wrapping
paragraphs below stop wrapping. When a row might not fit, it is `horizontal_wrapped`.

### egui repaints ON DEMAND, so an async result can sit unread indefinitely

A fetch that completes on a JS callback and pushes to an `mpsc` wakes nothing; the channel is
only drained on the next frame. On a desktop the first mouse move hides this completely. On a
phone nothing moves — collection-ownership's index sat on "Loading collections…" forever with
the response already in memory.

Either wake the `Context` at the point of send, or tick `request_repaint_after` while work is
outstanding — and key that tick off an explicit *pending* flag, **never** off `data.is_none()`,
which is also what failure looks like and will spin forever on battery.

### Compact-breakpoint touch sizing inflates non-interactive content too

`spacing.interact_size.y = 44` is a floor on allocated space *and* sets row height in
`horizontal`/`horizontal_wrapped`, so read-only status chips came out as 34pt squares around
10pt text.

Opt a dense region out with `ui.spacing_mut().interact_size = Vec2::ZERO` — on the region, not
inside the chip, because by then the row height is already decided.


## Never use raw Unicode symbols in egui
<!-- rule: rules/org/defrag/egui-icons-only -->

**Never use raw Unicode symbols** — `●` `○` `✓` `✕` `→` `★` and friends. They render as
broken boxes in the browser, because neither the default egui font nor the Phosphor font
includes the geometric and symbol Unicode blocks.

Use `PhosphorIcon` from `icons.rs`:

| Instead of | Use |
| --- | --- |
| `✓`, `●` | `PhosphorIcon::CheckCircle` |
| `✕`, `×` | `PhosphorIcon::X` |
| `○`, `◌` | `PhosphorIcon::Clock` |
| `⚠` | `PhosphorIcon::Warning` |
| `+`, `−` | `PhosphorIcon::Plus` / `PhosphorIcon::Minus` |
| `→` | `PhosphorIcon::ArrowRight` |

Basic ASCII (`!`, `?`, `#`, `+`, `-`) is fine.

### Adding an icon

Look the codepoint up in the Phosphor CSS
(`https://unpkg.com/@phosphor-icons/web@2.1.1/src/regular/style.css`), then add the variant
to `PhosphorIcon` in `icons.rs` — enum, `codepoint()`, `ALL`, `name()`.

### Why it is a rule and not a nitpick

The failure is invisible in a native build on a machine whose system fonts happen to cover the
glyph, and visible to every user in a browser. "It looked fine for me" is the default outcome
of testing it the wrong way.


## Let the marks carry it — egui is weak at prose
<!-- rule: rules/org/defrag/egui-marks-over-prose -->

egui has no real text shaping and poor typographic hierarchy, so **blocks of text are the
wrong tool.** If a widget is explaining itself in paragraphs, the design is wrong, not the
copy. Reach for an encoding instead:

| Instead of | Use |
| --- | --- |
| state described in a sentence | a pip track or progress marks |
| composition spelled out | shaped or coloured marks — see `PartyBadge`'s filled/half/hollow basis language, reused as support pips on `ClaimCard` |
| a number the reader must compare by eye | bar height or width |
| long-form detail | behind an expand, on hover, or in a side panel |

### The test

A list of twenty of these should be **scannable**. If reading twenty means reading twenty
paragraphs, redesign.

Keep at most one line of irreducible text — a title, a statement — and put the rest on demand.

### Applies to story captions too

Storybook captions are held to the same standard: one or two short lines, not an essay. A
caption that needs a paragraph to explain the widget is a caption describing a widget that
needs redesigning.


## Commit and PR conventions for this ecosystem
<!-- rule: rules/org/defrag/commit-conventions -->

Applies when you are asked to prepare a commit or draft a PR — see
`core/git-is-the-users-domain` for the standing rule
that you do not commit uninvited.

### Commits

Prefix with a type, then the change, then the PR number:

```
feature: indexer retries (#164)
fix: wrap the stake address in the compact breakpoint (#171)
chore: bump pallas to 0.31 (#168)
```

- The prefixes in use are **`feature:`, `chore:`, `fix:`** — not `feat:`/`refactor:`/`perf:`.
  Match what is already in the log rather than conventional-commits defaults.
- **Append the PR number** as `(#164)`. The log is read as a list of changes and their
  discussions.
- Scope narrowly. One concern per commit — a formatting sweep bundled with a behaviour change
  makes the behaviour change unreviewable.
- Imperative, present tense: "wrap the address", not "wrapped the address".

### Pull requests

Include a concise summary, the linked issue, test coverage for the change, and example output
or screenshots where they apply. Note any feature flags involved (`native`, `wasm`).

`[skip ci]` needs a clear justification in the description, and any manual deployment or data
task has to be named — CI cannot catch what CI did not run.


## Prefer rustls over OpenSSL
<!-- rule: rules/org/defrag/rustls-over-openssl -->

Use `rustls` rather than OpenSSL for TLS in any new dependency or HTTP client, and when
choosing between two crates that differ only in which they pull in.

The HTTP clients in this ecosystem already do. When adding a crate, check its default features
for `native-tls` or `openssl-sys` and prefer a `rustls` feature where one exists:

```toml
reqwest = { version = "0.12", default-features = false, features = `"rustls-tls", "json"] }
```

### Why

Two reasons, and the second is the one that bites:

- **Cross-compilation.** OpenSSL needs a system library and a matching toolchain; rustls is
  pure Rust. A `wasm32-unknown-unknown` build cannot link OpenSSL at all — see
  [`defrag-runtime-pairs`` for the same constraint in its other form.
- **A dependency that arrives transitively.** You rarely choose OpenSSL directly; it appears
  because a crate's *default* features pulled it in. That is why the check is on the transitive
  tree, not on the crate you typed:

```sh
nix develop -c cargo tree --target wasm32-unknown-unknown -p <crate> | grep -e openssl -e native-tls
```


## Subdirectory devshells are entered from the parent flake
<!-- rule: projects/shared-crates/rules/subdir-devshell -->

`ui/_storybook-egui` is served with the **root** devshell, run from that directory:

```sh
cd ui/_storybook-egui
nix develop ../.. -c trunk serve      # http://127.0.0.1:8095 (see Trunk.toml)
```

`nix develop` with no path looks for a flake in the current directory, which has none. This is
the invocation that works, and it is easy to lose an afternoon to `trunk: command not found`
before working it out.

See the `widget-screenshot` skill for the full
screenshot loop.


## A story has three registration sites plus a module declaration
<!-- rule: projects/shared-crates/rules/storybook-registration -->

`_storybook-egui/src/lib.rs` has **three** places a story must be registered:

1. The `stories! { … }` entry — one line, which generates the enum variant, the sidebar
   ordering, the group heading and the render dispatch
2. `label()`
3. The blurb `match`

plus `pub mod` in `stories/mod.rs`.

Miss one and it either fails to compile or silently never appears in the sidebar. The silent
case is the expensive one.

### Why it is not one site

This said "six" until the `stories!` macro landed; `src/registry.rs` documents what was broken
before. `label()` and the blurb stay hand-written **on purpose** — they are exhaustive
matches, so the compiler catches an omission, and moving ~130 prose strings into a macro would
risk pairing one with the wrong story for no safety gain.

### Also

`trunk build` is worth running on its own: the storybook is `crate-type = ["cdylib"]`, so
`cargo build -p storybook-egui` compiles without proving the wasm target works.

## Planning stays in reasoning

The harness occasionally injects a nudge to "first privately list what you need next", or a
variant of it. It means: do the listing in reasoning, then batch the independent tool calls
into one response. It is an instruction about batching.

**Never begin a response with "Privately," or any variant.** The nudge is not a phrase to
emit. More generally, the plan for getting to an answer is not part of the answer — see
`core/planning-stays-in-thinking`.

## Tool names

- **Files are edited with `Read` / `Edit` / `Write`.** Never `python`, `sed`, `awk`, `perl`
  or a heredoc. See `core/edit-via-editor-tools`
  for why this is a hard rule rather than a preference.
- **`Bash` for commands.** Read-only git is fine; history-mutating git is not.
- **Read-only git takes `--no-pager`** — `git --no-pager log`, not `git log`. Without it the
  command blocks waiting for a pager that will never receive input.
- **Anything that may open an editor takes a prefix**: `GIT_EDITOR=true git rebase …`,
  `PAGER=cat`, `EDITOR=true`.

### Crate references

When working with a crate that may have moved on from training data, check
`references/crates/` in the playbook for a cheat sheet before assuming an API. The
`crate-research` skill writes one when a crate is adopted.

<!-- END agent-playbook -->
