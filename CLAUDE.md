<!-- BEGIN agent-playbook (project: shared-crates, target: claude-code) — generated, do not edit -->

# Agent rules

Generated from `.playbook/project.conf` + `models/claude-code`.
Each rule's source file is named in the comment above its heading.
To change a rule, change the rule — not this block.

## Non-negotiable

The lead of each rule below, repeated so it is read first. Full text follows in place.

- **Working first — fix problems, don't hide them** — Deliver working functionality before optimising architecture. Fix the cause; never comment out, disable, or route around a problem to get a build green.
- **Never invent data to satisfy an interface** — Never fabricate a value to make a function, fixture or screen look functional. Before writing a placeholder, find where the data actually comes from — a config file, a table, an API response, a sibling implementation, an env var. A real search turning up nothing → ask, do not invent.
- **Tests are the specification — never edit an assertion to make it pass** — **Never modify a test assertion without explicit permission.** When behaviour is reported as wrong, the fix goes in the code; if the fix makes the test fail, the fix is wrong.
- **Do not report success you have not observed** — Claim only what you ran and saw this session. "Tests pass" = the command ran and its output showed them passing · "it builds" = a build ran (a successful edit is not one) · "it renders" = it was rendered and looked at (see `org/defrag/look-at-what-you-built`) · "fixed" = the symptom was reproduced, then observed gone.

## Do not report success you have not observed
<!-- rule: rules/core/verify-before-claiming -->

- Claim only what you ran and saw this session. "Tests pass" = the command ran and its
  output showed them passing · "it builds" = a build ran (a successful edit is not one) ·
  "it renders" = it was rendered and looked at (see `org/defrag/look-at-what-you-built`) ·
  "fixed" = the symptom was reproduced, then observed gone.
- Validation not run → say so and say why (no toolchain reachable, needs a device, needs
  credentials). An unearned "done" moves the discovery of the failure to me.
- Also not evidence: a partial result reported as complete (three of four done → say which
  three) · a step silently skipped (name it, do not drop it) · a green exit code from a
  command that did nothing — a `str.replace` matching no anchor, a test filter matching no
  tests, check the output says what you think it says · a narrower run than claimed (a
  single-crate build is not a workspace build).


## Tests are the specification — never edit an assertion to make it pass
<!-- rule: rules/core/test-preservation -->

**Never modify a test assertion without explicit permission.** When behaviour is reported as
wrong, the fix goes in the code; if the fix makes the test fail, the fix is wrong.

1. Fix the code to produce correct behaviour.
2. Leave the existing assertions alone.
3. If tests still fail, the fix is still wrong.
4. Tests are the specification, not an obstacle.

Never: change an assertion to match current behaviour · weaken a test (loosen a bound, widen
a tolerance, add an early return) · remove specificity (`assert_eq!(sales, vec![expected_sale])`
→ `assert!(!sales.iter().any(is_false_positive))` throws away the thing being tested) ·
delete, `#[ignore]` or comment out a failing test · optimise for a green suite over a correct
system · assert what the code currently does and call it the specification.

Allowed without asking: **adding** a test for behaviour that had none · fixing a test that
cannot compile because a signature legitimately changed, provided its assertion survives ·
renaming a test to describe what it checks.

A test that is genuinely wrong → say so, explain why the assertion is incorrect, and get
sign-off before changing it.


## Working first — fix problems, don't hide them
<!-- rule: rules/core/working-first -->

- Deliver working functionality before optimising architecture. Fix the cause; never comment
  out, disable, or route around a problem to get a build green.
- Facing a compile error, a lifetime fight or a design question → ask **"what is the simplest
  thing that makes this actually work?"**, not "what is the correct architecture?".
- Stop and reconsider if you are about to:
  - add a `// TODO:` to disable functionality that was supposed to work
  - comment out code to silence a compilation error
  - fight type/lifetime/async issues before the plain logic is proven
  - rewrite a signature to make an error go away rather than understanding it
  - say "we'll implement that later" about a core feature rather than an edge
  - spend longer on the shape of the code than on whether the feature works
- Deviate only when continuing would break something that works, introduce a security hole,
  or risk data corruption — and then fix it properly, do not disable it.

**Make it work, make it right, make it fast — in that order.**


## Never invent data to satisfy an interface
<!-- rule: rules/core/never-make-things-up -->

- Never fabricate a value to make a function, fixture or screen look functional. Before
  writing a placeholder, find where the data actually comes from — a config file, a table, an
  API response, a sibling implementation, an env var. A real search turning up nothing → ask,
  do not invent.
- Same rule for: a plausible address, hash or timestamp invented for a fixture (it hides real
  parsing bugs, because invented data is already in the format the code expects) ·
  `unwrap_or(0)`, `Default::default()` or a hard-coded fallback on a value that should have
  been sourced · a struct written from memory of what an API "should" return (cite the actual
  response or the docs) · a test asserting what the code currently does · a confident
  explanation of a failure you have not verified.
- Choosing a sensible default and **stating it** is fine. Inventing a value and presenting it
  as data is not. Test: if someone asks "where does this number come from?", is there an
  answer? "Nowhere yet" → say so and ask.


## Build what was asked, then stop
<!-- rule: rules/core/dont-rush-new-features -->

- Asked for X → build X and stop. Do not add Y and Z because they seem wanted: no config
  option "for flexibility", no trait "for testability", no abstraction "for later" — unless
  the task named it.
- Plan each logical step of implementation together, one at a time.
- Three exceptions only: the task cannot be completed without it (a caller that must be
  updated to keep the workspace compiling, a type the requested feature needs to typecheck) ·
  not doing it would break something that currently works · I asked you to use your judgement.
- Everything else → mention it and let me decide. A one-line "this would also allow X if you
  want it next" is welcome. Building X uninvited is not.
- Applies to cleanup too: renames, import reordering, tidying a neighbouring function, bumping
  a dependency "while I'm here". If a nearby thing is genuinely broken, say so in the final
  message rather than fixing it silently.


## Ask before changing dependencies or architecture
<!-- rule: rules/core/conservative-package-changes -->

Stop and ask before a change that alters the *approach* rather than the *implementation*.
Present the problem, two or three specific options with their trade-offs, and wait.

Ask first for:

- editing `Cargo.toml`, `package.json`, `flake.nix` or any other manifest
- adding, removing or bumping a dependency
- swapping one library for another (`smlang` → `rust-fsm`, `reqwest` → `ureq`)
- replacing a whole module or implementation rather than fixing it
- choosing a pattern — state machine style, database access layer, error strategy, where a
  type lives
- anything that changes how the project is built, deployed or configured

No need to ask for: a bug fix inside the existing approach · a direct instruction ("change X
to Y") · formatting, lint fixes, or renames local to one function.

Options must be **specific** — name the crates, say what each costs. "We could use a library
or write it ourselves" is not a set of options.


## Git state is mine — do not commit, push, merge or stage
<!-- rule: rules/core/git-is-the-users-domain -->

- Do not run `git add`, `git reset`, `git restore --staged`, `git stash`, `git commit`, `push`,
  `merge`, `rebase`, `tag` or anything else that changes the index, history or a remote unless I
  specifically ask. The same prohibition covers discarding working-tree changes — `git restore
  --source`, `git checkout -- <paths>`, `git apply --reverse`, `git clean` — because the working
  tree is mine to review too. Leave the index exactly as you found it; do not stage or unstage
  anything.
- For reads, `at-recall` and `at-peek` are the default, not the alternative. An agent shell may
  not inherit the devshell; if `which at-recall` is empty, reach it as `direnv exec . at-recall …`
  (and the same for `at-peek`). A bare `git status`, `git diff` or `git log` is a last resort:
  only after that invocation fails, and say which one you tried. `git show` and `git blame` have
  no `at-recall` verb yet — say so rather than widening the habit silently. Use `--no-pager` for
  any git read that remains.
- Branching for work is fine when it helps. Creating a branch rewrites nothing I have to
  unpick.
- If you change the index by accident, undo exactly that and say so.
- Also: no `[skip ci]`, `--no-verify` or bypassing hooks without asking · no amend or
  force-push · do not commit artefacts `.gitignore` excludes, even when they are up to date ·
  do not write a commit message and leave it staged in a way that suggests it was committed.


## Edit files with the editor tools, never with a shell script
<!-- rule: rules/core/edit-via-editor-tools -->

- Edit files with the read/edit/write tools. Never `python`, `sed`, `awk`, `perl`, `truncate`
  or a heredoc — not for a big change, and not for "just one small change". This applies to
  every file: source, config, docs, rules, memory.
- A change that touches ten places is ten edit calls, not one script.
- The editor tools verify the file was read first, fail loudly on a stale or ambiguous
  match, and show a reviewable diff. A script's `str.replace` **silently does nothing** when
  the anchor text has moved.
- Still fine: generating a file's *content* with a script when the content is genuinely
  computed (a catalogue from source headers, a table derived from data) — writing it to disk
  still goes through the write tool. And reads through the shell (`cat`, `grep`, `find`) to
  gather information, before editing with the editor tools.


## Kill only the processes you started — never by name pattern
<!-- rule: rules/core/kill-only-what-you-started -->

- Never clean up with `pkill -f "<app>"`, `killall <app>`, or any pattern that matches a
  binary by name — it also matches the copy the user is running interactively. Kill only the
  instances you started, by an identifier unique to them: a PID you captured, a flag you
  passed, or a port only your process bound.
- A long-running helper you spawn — a headless browser, a dev server, a watcher — gets a
  unique handle on its own command line: `--user-data-dir=.tmp/<task>-profile`, a `--port` you
  chose, an output path under `.tmp/`. Clean up with *that string*, not the app's name.
- The handle must be one only your process carries. `pkill -f "Brave Browser"` is the failure;
  `pkill -f "brave-profile-<task>"` is the fix, because the user's own Brave has no such flag.
- Cannot give it a unique handle → kill nothing. Say what you left running instead.
- Put scratch profiles and artefacts under `.tmp/` in the repo, so the handle and the cleanup
  target are unambiguous.


## Typed structs only — no serde_json::Value, no json! macro
<!-- rule: rules/core/typed-json-only -->

- Never `serde_json::Value` or `json!()` for data structures, API responses, configuration,
  queue messages, SSE payloads, or anything persisted. Define concrete types with
  `#[derive(Serialize, Deserialize)]`.
- Banned: `Value` · `json!()` · `Map<String, Value>` · `Vec<Value>` · any composition of
  `Value` · `#[serde(untagged)]` used to avoid deciding on a shape.
- Required: structs and enums with serde derives · `#[serde(tag = "type")]` for tagged unions
  rather than a stringly-typed discriminant field · `Option<T>` for nullable fields and
  newtypes for ids and timestamps · `#[serde(flatten)]` where a shape genuinely composes.

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

Exceptions, allowed only with the justification written in the code: consuming a genuinely
dynamic external API whose schema cannot be known · temporary debugging code that will be
removed · a low-level JSON utility whose whole purpose is handling arbitrary JSON.


## Plan in reasoning, not in the response
<!-- rule: rules/core/planning-stays-in-thinking -->

- I see the answer and the diff, not the plan for getting there. Do not narrate what you are
  about to look up — look it up.
- Not: "I'll start by reading X, then check Y" as a preamble to reading X and checking Y ·
  restating the request before acting on it · announcing each step ("Now let me look at the
  config…") · a plan-shaped response where a tool call was the answer.
- Still fine: a genuine plan when the task is large enough that I should agree to the approach
  first (a decision I need to make, not narration) · one sentence before a group of related
  tool calls · reasoning in the final message when it affects whether I trust the result.
- A nudge to "first privately list what you need next" means list it *in reasoning*, then
  batch the independent tool calls into one response. Never begin a response with
  "Privately," or any variant.


## Never commit a secret
<!-- rule: rules/core/no-committed-secrets -->

- No credential, token, private key, connection string or API key in the repository — not in
  source, a test fixture, a config file, a commit message, or a comment explaining what the
  value used to be.
- A change that needs a secret gets it from the environment or a secrets manager. The
  repository holds the variable *name* and an `.env.example` with an obvious placeholder:

```
# .env.example — committed
KOIOS_API_KEY=your-key-here

# .env — gitignored
KOIOS_API_KEY=<real value>
```

- Need one to test something → say so and stop. Do not invent a placeholder that looks real,
  and do not reach for a real key already in the environment; a test that only passes with a
  live credential is not a test, it is a scheduled failure.
- Find one already committed → **report it**, do not quietly delete the line. It is in the
  history, so deleting it from the working tree does not revoke it. The secret has to be
  rotated, and that is my decision rather than a cleanup you perform silently.
- A secret in a code comment or a `// TODO: replace before merge` is the same leak with an
  expiry date that nobody enforces.


## Prefer ownership over shared mutable state
<!-- rule: rules/core/ownership-over-shared-state -->

Prefer an ownership design to shared mutable state. Do not reach for `Rc<RefCell<T>>`,
`Arc<Mutex<T>>` or a `static` lock to get around a borrow, lifetime or callback problem —
restructure the value instead, and say what the restructure is.

Three shapes that usually replace it:

- **Split the value.** The thing being mutated and the things reading it become separate
  objects, wired together by the caller that owns both.
- **Move the state to the caller.** The count, the cache, the cursor is owned by whoever
  drives the loop and passed in, so there is one writer by construction.
- **Replace the check, not the state.** If the shared state existed only to catch a misuse, a
  type parameter, a consumed token, or a `Result` from the one place that can see the misuse
  costs less and cannot be reached around.

If shared mutable state is genuinely the answer, name what breaks without it and ask before
building on it. Do not offer it as the low-effort option in a list of designs either.

Narrow existing uses are not this rule: a counter or a memo already behind `&self` inside one
type is an implementation detail, not a design, and it does not need a decision from me.


## Rust tooling lives behind the Nix devshell
<!-- rule: rules/rust/devshell-first -->

`cargo`, `rustc`, `clippy`, `rustfmt`, `trunk`, `wrangler`, `node` and the wasm targets are
**not on `PATH`** — they come only from the devshell defined in `flake.nix`. Wrap every
command:

```sh
nix develop -c cargo build --workspace
nix develop -c cargo test -p <crate>
nix develop -c cargo clippy --workspace --all-targets --all-features -- -D warnings
nix develop -c cargo fmt
```

`nix develop --command <cmd>` is equivalent.

**In a sandboxed shell, `nix develop` cannot reach the daemon socket**
(`/nix/var/nix/daemon-socket/socket`) — `cannot connect to socket … Operation not
permitted`. Use direnv instead, which reads the already-realised devshell out of `.direnv/`
and needs no daemon:

```sh
direnv exec . cargo build -p <crate>
```

If `.direnv/` is cold, run `direnv allow .` once from an unsandboxed shell.

Never: install a separate toolchain, source `~/.bash_profile` to find cargo, or reach for the
network. Add `--offline` if cargo tries to fetch a crate the devshell cache already holds.


## Build often — the compiler is the best information you have
<!-- rule: rules/rust/build-often -->

- A change that could plausibly break a type → compile it. Do not read your way to confidence
  through three files when one `cargo check` answers the question in seconds.
- `nix develop -c cargo check` while iterating · `cargo build` when you need artefacts · `-p
  <crate>` to scope a large workspace.
- `cargo clippy` for the lints the compiler does not carry — see
  `rust-tooling-handles-grunt-work`.
- Build fails → read the whole error before editing. Rust errors cascade; fixing a consequence
  first produces a second round of errors that look new.
- Build already established as expensive and the change cannot reach it (editing a markdown
  file does not need a compile) → do not build.


## Inline format arguments everywhere
<!-- rule: rules/rust/inline-format-args -->

Inline format arguments everywhere — clippy warns on the alternative and the lint command
treats warnings as errors.

```rust
// no
format!("Hello {}", name)
println!("wrote {} rows to {}", count, path)

// yes
format!("Hello {name}")
println!("wrote {count} rows to {path}")
```

- Every formatting macro: `format!`, `println!`, `eprintln!`, `write!`, `writeln!`, `panic!`,
  `assert!`, `assert_eq!`, `debug_assert!`, `tracing` macros.
- New code → inline args from the first draft.
- Editing existing code → convert the lines you are already touching · do not sweep the file.
  Broadly non-compliant file → `clippy --fix` as its own change — see
  `rust-tooling-handles-grunt-work`.


## Let fmt and clippy do the mechanical work
<!-- rule: rules/rust/tooling-handles-grunt-work -->

Formatting and mechanical lint fixes are tool work. Do not hand-align code or type out a fix
the tool will apply itself.

```sh
nix develop -c cargo fmt
nix develop -c cargo clippy --fix --allow-dirty --all-targets --all-features -- -D warnings
```

- Run them, then read what changed and check the remaining diagnostics manually. `--fix` first,
  then review — the pass changes the lines you were about to read.
- A warning is a failure, not a note. This codebase does not accumulate "known warnings".
- `--fix` applies only mechanically safe suggestions — it will not choose between two valid
  shapes · know a `disallowed_methods` match is a false positive on a blessed call · fix
  anything needing a semantic decision.
- A rule clippy cannot enforce → add a source scan in `cargo test`, in the same spirit as
  `cargo fmt --check`. Add the test with the rule.


## Where tests live, and what they may touch
<!-- rule: rules/rust/test-layout -->

- Unit tests → a `mod tests` block in the same file. They can reach private items, which is
  usually the point.
- Integration tests → `tests/`, one file per scenario, exercising the public API only.
- Runnable examples → `examples/` when they clarify the API. `cargo run -p <crate> --example
  <name>` is the test that a consumer can actually use the thing.
- WASM tests → `wasm-bindgen-test` and a WASM-capable runner. They do not run under plain
  `cargo test`, so a green suite is not evidence that a WASM test passed.
- Tests are deterministic. No network calls unless they are feature-gated or mocked.
- A fixture a second crate needs → capture it in the shared `data/` crate rather than copying
  it. Shared serialization fixtures live in `test_datum_serialization.rs`.


## Read the widget catalogue before building any UI
<!-- rule: rules/org/defrag/widget-catalogue-first -->

**Before writing any widget, read the catalogue.**

- `ui/egui-widgets/CATALOG.md` in `shared-crates` — ~100 widgets, one line each.
- `ui/macroquad-widgets/CATALOG.md` — the macroquad set.

Read the whole file. Do not grep instead.

- Hand-building something widget-shaped — a chip, an id display, a label-value grid, a card, a
  chart, a stat strip → scan first. It probably exists.
- A new widget → build it in `egui-widgets` with a `//!` header, add a storybook story, and run
  `UPDATE_CATALOG=1 nix develop -c cargo test -p egui-widgets --test catalog`. A module without
  the header fails that test.


## Read code and history with the agent tools, not with the shell
<!-- rule: rules/org/defrag/agent-tools -->

- Reading code with `rg`, `grep`, `sed`, `cat`, `head` or `wc` → `at-peek`. Installed crate source or
  build-script output → `at-crate`. History or working-tree state → `at-recall`. A Cardano chain
  question → `at-cardano`. All four are read-only and say what they did not show.
- `git show` and `git blame` have no `at-recall` verb yet — say so rather than reaching for them.

| Instead of | Use |
| --- | --- |
| `rg`, `grep` | `at-peek search <pattern> [path…]` |
| `sed -n '40,60p' <file>`, `head`, `cat` | `at-peek slice <file>:40-60` |
| `wc -l`, `ls -l`, `stat` | `at-peek stat <path>…` |
| `grep -rn … ~/.cargo/…` | `at-crate search <pattern> <crate>` · `slice` · `where` |
| `git status` | `at-recall state` |
| `git diff` | `at-recall diff [<rev>] [<path>…]` — `--patch` for hunks |
| `git log` | `at-recall log [<rev>] [<path>…]` |
| PR archaeology | `at-recall pr [--base <rev>]` |
| `curl …koios…` | `at-cardano <noun> [<aspect>]` · `query` · `run --scenario` |
| anything not listed | `at-describe` — the catalogue, one screen |

- `which at-peek` empty → `direnv exec . at-peek <verb>`. Never wrap an `at-*` call in
  `nix develop`: the evaluation is paid before the read starts (7.5 s vs 5 ms).
- Output that was cut says so, and a `# next:` line is the next read — run it as printed.
  Never `| tail -3`: the bound is the part that makes the answer reportable.
- These tools are for understanding, not for preparing an edit. Read the file with the editor
  tools before changing it.
- The `inspect-code` skill has the flags, exit semantics, recipes and traps; the `at-cardano`
  skill has the chain verbs. Invoke one when a read is not a plain `search` or `slice`.


## Toggle local [patch] blocks as a unit
<!-- rule: rules/org/defrag/patch-blocks -->

Engaging a local `[patch]` override block in `Cargo.toml` — to consume a sibling working tree
instead of the pinned git rev → uncomment the **entire `[patch."…"]` table as a unit**.

- Never selectively un-comment individual entries.
- Re-commenting before commit → re-comment the whole block. Never leave a partial active block
  behind.

Selective edits cause two failures:

- A line ends up in both the active and the still-commented copy → duplicate-key error
- A transitively-required crate is left commented → unrelated workers break


## Sizes, spacing and colour come from the theme
<!-- rule: rules/org/defrag/theme-tokens-only -->

No widget writes a point size down. No renderer crate copies a palette or a ramp.

#### Sizes resolve through the ramp

```rust
// no
ui.label(RichText::new("total").size(11.0));

// yes
ui.label(RichText::new("total").text_size(ui.text_size(TextSize::Base)));
```

- A config struct holds a `TextSize`, not an `f32`, and resolves it in `show()` where a `Ui`
  finally exists — `route_quote` and `pool_inspector` are the shapes to copy.
- A genuinely non-textual size (a pixel dimension, e.g. `ImageStack::size(96.0)`) opts out with a
  trailing `// theme-exempt: <reason>`.

#### The vocabulary lives in one place

`ui/ui-theme` owns colour tokens, `Ink`, the colour science and the type ramp. Each renderer
aliases it and implements `Palette`, so an `Ink` written on either side resolves on both. Nothing
in that crate names a renderer.

**Never copy a palette or a ramp into a renderer crate.** The same applies to `Space`, `Radius` and
`Breakpoint`.

#### The `f32` ratchet

`f32` config fields are a ratchet — the count may only go down. Convert one, lower the baseline.

#### Changing a ramp is a restyle, not a refactor

Move a ramp as its own change, with the reason written down. Never during a migration.


## Keep logic pure — runtime bridges come in pairs
<!-- rule: rules/org/defrag/runtime-pairs -->

**wasm-bindgen code can never run under macroquad/miniquad** — the miniquad `gl.js` runtime has no
wasm-bindgen glue; browser interop goes through the miniquad plugin protocol (`sapp_jsutils`)
instead. Permanent, not a version issue.

- Runtime-specific bridges come in pairs: `wallet-core` for wasm-bindgen frontends,
  `wallet-miniquad` for macroquad games. egui-widgets and macroquad-widgets do not interchange.
- Keep logic in small pure crates — no wasm-bindgen, no I/O, no runtime deps — so the same crate
  can be consumed from a macroquad game, a wasm-bindgen frontend (Leptos) **and** a Cloudflare
  worker. Push runtime bindings to thin edge crates.
- Macroquad target → check a dependency's transitive tree for wasm-bindgen before adding it. A
  native `cargo tree` is blind to this; the target flag is required:

```sh
nix develop -c cargo tree --target wasm32-unknown-unknown -p <crate>
```

- WASM build → minimise the features enabled on a wasm32 target. Default features are chosen for a
  native build, so a wasm consumer inherits capabilities it cannot use; a feature that pulls in a
  system library fails at link time, not at the crate that introduced it. Check the transitive
  tree, not the direct dependency.


## Every macroquad modal draws through the widgets' sheet chrome, and a better look upgrades it
<!-- rule: rules/org/defrag/macroquad-one-modal-chrome -->

Every macroquad modal draws its card with `macroquad_widgets::sheet_card` and a `Sheet` spec
(title, subtitle, tint, footer). Never paint a scrim, a card or a close button of its own. When
one modal looks better than the shared chrome, upgrade `sheet_card` and its parts to draw that
way, then move every modal onto them. Do not fork a one-off chrome for the good-looking one.

Inside the card:

- **Facts** go in a `macroquad_widgets::well`, as `WellRow`s: inline for label and value,
  stacked for a label over its value, with a trailing figure or Copy, a meter, or `.strong(true)`
  for a total.
- **Actions** are `Pill`s (filled, ghost or danger) in `SheetCard::footer`, laid out with
  `pill_row`. Pass `.live(card.live)` so a sheet that is still sliding takes no tap.
- **Closing** is the header's cross plus a tap on the scrim, both reported as
  `SheetCard::dismissed`. A sheet has no Close button.
- **A modal with its own motion and layout**, like the gamekit's wallet modal, still paints with
  `sheet_surface` and closes with `sheet_close`, so it cannot drift from the rest.

The rounded shapes and the gradient they draw with live in `macroquad_widgets::shape`. The
gamekit re-exports them.


## A macroquad scrolling container scrolls with momentum, through the widgets' Scroll
<!-- rule: rules/org/defrag/macroquad-scroll-momentum -->

Every macroquad container that scrolls holds a `macroquad_widgets::Scroll` and reads its offset
from `Scroll::offset`. Never hand-roll an `f32` offset with its own wheel and drag code. A list
that steps a whole row at a time uses `macroquad_widgets::Momentum::rows` instead.

Each frame, before laying out:

```rust
let input = ScrollInput::read(frame.input.gesture.drag, frame.pointer());
self.scroll.update(area, &input, Some(content_len - area.h), frame.dt);
let tap = self.scroll.filter_tap(frame.input.gesture.tap);
```

- Pass `None` for the maximum until the content's length is known, so a position asked for
  early (a `--scroll` flag) is not clamped to nothing.
- Hit-test with the tap from `filter_tap`. A finger that lands on a coasting list only stops it.
- Behind a modal, pass `ScrollInput::none()`. The container then takes no new input, and a coast
  already under way runs out.
- Jump with `Scroll::set` for a reveal, a key or a restored position. It stops any coast.

Between them, `Scroll` and `Momentum` decide which drag is the container's, the flick and its
coast, the wheel in notches or points, the clamped ends, and the tap that only caught a coast.
A hand-rolled offset gets each of these wrong in its own way.


## A game's rules crate reports what happened; the client animates it and decides nothing
<!-- rule: rules/org/defrag/rules-emit-events -->

**The rules crate returns what happened as data, and the client only animates it.** A client never
re-derives a rule — not damage, not which card was drawn, not where a stripped card went.

- Each step returns typed events (a card moved from pile to pile, an effect resolved with an
  amount) beside the new view. The client queues animations from the events, then shows the view.
- Anything the screen shows as a number or an icon arrives as data, not only as a sentence:
  `effects: Vec<EffectView>` beside `text: Vec<String>`, so the face draws icons without parsing
  prose.
- The client checks itself against the engine: after each step it compares what it shows (the
  hand, the pile counts) with the view, and the soak fails on any mismatch.
- The rules crate is pure (see `defrag-runtime-pairs`): one crate serves the macroquad client, a web
  bridge and a replay verifier.


## macroquad's silent traps — each fails with no error and a wrong picture
<!-- rule: rules/org/defrag/macroquad-traps -->

Before drawing with macroquad directly, check `shared-crates/gaming/macroquad-gamekit/CATALOG.md`.
Each trap below compiles, runs, and shows a wrong picture with no error.

- **The window is upside down under `Camera2D::from_display_rect`.** Use `harness::window_camera`
  for the window and `harness::target_camera` for a target. A capture that looks right says
  nothing about the window.
- **Dropping a `RenderTarget` frees its texture**, and the next texture reuses the id. Hold it.
- **A render target's texture is upside down** when drawn: `flip_y: true`.
- **Translucent layers wear down a target's alpha.** Bake layered art with `Baker`; capture
  through `Capture::save`, which writes opaque.
- **A capture is only the size you asked for if the window is.** Take the viewport from
  `harness::Args::viewport`.
- **Every new glyph re-uploads the whole font atlas.** Run the screen through
  `macroquad_gamekit::app::run` (the glyph gate) and load fonts with `load_font`.
- **Text queued before the atlas grows vanishes.** The gate grows it between frames; `Baker`
  draws twice for the same reason.
- **Bake text that must never wait a frame.** Large text warms one character at a time; use a
  `Baker` face scaled through a `Pose`.
- **A capture can outrun the glyph gate and photograph an empty line.** Wait for two drawn frames
  of unchanged state; draw long sentences at a body size.
- **macroquad 0.4.16 doubles a glyph atlas at random.** Carry the workspace `[patch.crates-io]`
  pinned to `8d602d3f8ebb8a3a582ee15d92983bbb81ee3ef6`.
- **Never animate a font size.** Use `TextLayout::draw_scaled`, or bake once and scale through a
  `Pose`; `--frame-log` shows the stall.
- **Fractional text sizes must measure and draw at the same size.** Use `RichText` with
  `MeasureAt(pixel_scale)`.
- **Text centred by its size or line box sits high.** Place by midlines: `Painter::text_in`,
  `Painter::stack_in`, `text_mid`/`mono_mid`. The catalogue's "Text placement" section lists them.
- **Images decode from PNG and TGA only.** Convert art to PNG at build time.
- **With `high_dpi`, touches are device pixels and the mouse is points.** Convert once through
  `Viewport::to_design`; for several fingers use `macroquad_widgets::touch_points()`.
- **A touch has one phase a frame, its last event's.** Treat a first-seen finger as landing,
  whatever its phase; `Thumbs`, `Gestures` and `FloatingStick` do.
- **`get_screen_data` returns black on macOS** when unfocused or uncomposited. Capture through a
  render target: `harness::Capture` or `harness::save_texture_png`.
- **`draw_arc` thickness grows outward from the radius**, not centred.
- **miniquad's log macros print a lone literal verbatim.** Pass the value positionally
  (`"refused: {}", why`) when the string is the only argument.

These pass every native run and capture, and fail only in the browser:

- **`render_target()` kills the web build.** Use `harness::plain_target` (sample count 0).
- **A missing miniquad plugin never stops the page loading.** Take audio and gamepads from the
  kit's features, and stamp their JS from `build.rs` with `macroquad_gamekit::web::stamp`.
- **A copied `gl.js` drifts from the miniquad beside it.** Bundle with `mq-bundle`; never copy
  the JS or write `index.html` by hand.
- **`set_cursor_grab(true)` at load breaks the page.** Grab the pointer on the first click.


## Never drive a terminal UI with tmux — use tui-shot or the crate's harness
<!-- rule: rules/org/defrag/tui-capture -->

**Never drive a TUI with tmux.** `tmux` is a generic executor: approving it approves `send-keys`
into any pane and `attach` to sessions it did not start, so it cannot be granted per-prefix.

| Want | Use |
| --- | --- |
| a live screen, keys, a resize, a narrow terminal | `tui-shot` — one PTY, one workspace binary target, no shell |
| a frame while iterating on the renderer | the crate's own `TestBackend` harness |
| the second check | `tui-shot`'s `<out>.json` · the harness's `run.json` |

```sh
tui-shot run --scenario .tmp/factotum.dsl
tui-shot check --scenario .tmp/factotum.dsl
```

- Wait for content, never sleep: `wait … contains=`, `absent=`, `idle=`, `exit=true`.
- Not: a hand-started tmux session, `send-keys`, `capture-pane`, or a fixed `sleep`.
- The `tui-shot` skill has the grammar, fields and failures.


## Capture the exact moment, read its report, then check the real window
<!-- rule: rules/org/defrag/look-at-the-game -->

A macroquad screen is not done until a capture of each changed state has been looked at, its
report's problems dealt with, and the real window checked once after any camera or input change.

```sh
nix develop -c cargo build -p <game>
./target/debug/<game> --shot .tmp/x.png [--at 1.25] [--hover N | --drag N | --rclick N]
./target/debug/<game> --sheet .tmp/sheet.png     # every face at once, where the game has one
```

- Each capture writes `x.png` and `x.txt`. **The image is the review.** The report is the second
  check: it leads with what a glance misses (text cut short, too small, off its surface,
  overlapping), then lists every line of text with its box and size.
- Capture **the moment**, not the end state: `--at` on the fixed clock catches a card mid-lift or a
  sheen mid-sweep. An animation is reviewed at three moments, not one.
- Problems listed in the report are fixed or explained before the work is reported done.
- **The window is not a capture.** After touching the camera, input or window setup, run the game
  without `--shot` and screenshot the real window (the `game-spike` skill has the command) before
  calling it done.
- Not evidence: it compiles · the soak passes · a capture from before the change.


## Render it and look at it before reporting it done
<!-- rule: rules/org/defrag/look-at-what-you-built -->

Every widget in `ui/egui-widgets` has a story in `ui/_storybook-egui`. A new or changed widget must
be rendered and looked at before it is reported as done.

- Applies to narrow widths too, where most of the real failures live — see the
  `widget-screenshot` skill for the exact invocation,
  including why `--window-size` cannot produce a mobile shot.
- Not evidence: it compiles · its tests pass · the storybook entry renders without a panic · it
  looks right in the code.
- A *deployed* app is checked the same way — the helper points at a real URL.


## Bulk uploads to R2 run on the arm box, not the workstation
<!-- rule: rules/org/defrag/bulk-uploads-run-on-the-arm-box -->

**Build and verify on the workstation; run the transfer on the arm box.** The workstation's
uplink sets the ceiling for a GB-scale write to R2.

| step | where |
| --- | --- |
| build the artifact · dry-run the publish · check the bytes | workstation |
| the `rclone`/`publish` run that moves GB to R2 | arm box |

- `--run`-gated publishers print their exact `rclone` invocation without `--run`; copy that line
  to the box rather than re-running it here.
- The box already holds the chunk store, so an artifact derived from chunks is rebuilt there,
  never copied to it.
- Native build on the box, not `aarch64-unknown-linux-musl`: the musl build is slower.


## egui layout traps that cost an afternoon
<!-- rule: rules/org/defrag/egui-layout-traps -->

Read before laying out a panel, sizing a `Ui`, or wiring an async result into a widget.

- **A detail pane beside content → `detail_split`, not a right-hand `Panel`.** A top-down `Ui`
  never reads the reservation, so the following `CentralPanel` takes full width and the pane
  floats over the content.
- **`Color32` stores PREMULTIPLIED channels.** Each channel must be `<= alpha`.
- **Images load when the widget is BUILT, not drawn.** Reserve the space, then gate on
  `ui.is_rect_visible` — `activity_feed` is the shape.
- **`Ui::set_max_width` WIDENS a `Ui` with less room.** Use `viewport::fit(ui, 520.0)`.
- **An overflowing `ui.horizontal` widens the parent.** Use `horizontal_wrapped`.
- **egui repaints ON DEMAND — an async result can sit unread.** Wake the `Context` on send, or
  tick `request_repaint_after` off an explicit pending flag, never off `data.is_none()`.
- **Compact-breakpoint touch sizing inflates non-interactive rows.** Opt out with
  `ui.spacing_mut().interact_size = Vec2::ZERO` on the region.


## Never use raw Unicode symbols in egui
<!-- rule: rules/org/defrag/egui-icons-only -->

Never use raw Unicode symbols — `●` `○` `✓` `✕` `→` `★` and friends. Neither the default egui
font nor the Phosphor font covers the geometric and symbol Unicode blocks, so they render as
broken boxes in the browser.

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

Adding an icon → look the codepoint up in the Phosphor CSS
(`https://unpkg.com/@phosphor-icons/web@2.1.1/src/regular/style.css`), then add the variant to
`PhosphorIcon` in `icons.rs` — enum, `codepoint()`, `ALL`, `name()`.


## Let the marks carry it — egui is weak at prose
<!-- rule: rules/org/defrag/egui-marks-over-prose -->

Blocks of text are the wrong tool in egui — reach for an encoding instead:

| Instead of | Use |
| --- | --- |
| state described in a sentence | a pip track or progress marks |
| composition spelled out | shaped or coloured marks — `PartyBadge`'s filled/half/hollow language, reused as support pips on `ClaimCard` |
| a number the reader must compare by eye | bar height or width |
| long-form detail | behind an expand, on hover, or in a side panel |

- A list of twenty should be scannable; keep at most one line of irreducible text.
- Storybook captions: one or two short lines, not an essay.


## Commit and PR conventions for this ecosystem
<!-- rule: rules/org/defrag/commit-conventions -->

Applies only when asked to prepare a commit or draft a PR — see
`core/git-is-the-users-domain` for the standing rule
that you do not commit uninvited.

#### Commits

Prefix with a type, then the change, then the PR number:

```
feature: indexer retries (#164)
fix: wrap the stake address in the compact breakpoint (#171)
chore: bump pallas to 0.31 (#168)
```

- Prefixes are `feature:`, `chore:`, `fix:` · never `feat:`/`refactor:`/`perf:` — match the
  existing log, not conventional-commits defaults.
- Append the PR number as `(#164)`.
- One concern per commit — never bundle a formatting sweep with a behaviour change.
- Imperative, present tense: "wrap the address", not "wrapped the address".

#### Pull requests

- Include a concise summary, the linked issue, test coverage for the change, and example output or
  screenshots where they apply.
- Note any feature flags involved (`native`, `wasm`).
- `[skip ci]` → needs a clear justification in the description, and any manual deployment or data
  task named (CI cannot catch what CI did not run).


## Prefer rustls over OpenSSL
<!-- rule: rules/org/defrag/rustls-over-openssl -->

Use `rustls` rather than OpenSSL for TLS in any new dependency or HTTP client, and when choosing
between two crates that differ only in which they pull in.

- Check a crate's default features for `native-tls` or `openssl-sys`; prefer a `rustls` feature
  where one exists:

```toml
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }
```

- Check the transitive tree, not the crate you typed — OpenSSL usually arrives because a crate's
  *default* features pulled it in:

```sh
nix develop -c cargo tree --target wasm32-unknown-unknown -p <crate> | grep -e openssl -e native-tls
```


## Subdirectory devshells are entered from the parent flake
<!-- rule: .playbook/rules/subdir-devshell -->

`ui/_storybook-egui` is served with the **root** devshell, run from that directory:

```sh
cd ui/_storybook-egui
nix develop ../.. -c trunk serve      # http://127.0.0.1:8095 (see Trunk.toml)
```

See the `widget-screenshot` skill for the full screenshot loop.


## macroquad-gamekit never reads the clock, and every module has a catalogue line
<!-- rule: .playbook/rules/gamekit-takes-time-as-an-argument -->

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


## A story has three registration sites plus a module declaration
<!-- rule: .playbook/rules/storybook-registration -->

`_storybook-egui/src/lib.rs` has **three** places a story must be registered:

1. The `stories! { … }` entry — one line, generating the enum variant, the sidebar ordering,
   the group heading and the render dispatch
2. `label()`
3. The blurb `match`

plus `pub mod` in `stories/mod.rs`.

Miss one → it either fails to compile or silently never appears in the sidebar. The silent case
is the expensive one.

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
