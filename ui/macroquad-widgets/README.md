# macroquad-widgets

VM-driven, immediate-mode widgets for **buyer-facing macroquad surfaces** — the
txmints mint app and its kin (mint checkout, order/fulfilment heartbeat, reveal),
and behind the `trading` feature a token's market and lifecycle (the token arena's
price chart, life strip, holders board and map, header and feed).
A widget takes a [`Painter`](src/painter.rs) (fonts + this frame's tap) and a plain
VM, draws, and returns actions. No async, no I/O, no backend types, **no retained
widget state**. Mirrors `egui-widgets` (VM in → actions out) so the same projection
pattern works on both renderers. Compiles native + wasm32.

Two features gate the surfaces a mint app has no use for, both OFF by default:
`chain` (block pulse, block train) and `trading` (the market views). The storybook
turns both on, which is where they are developed.

Develop and fine-tune widgets in the storybook: `cargo run -p macroquad-storybook`
(native window — sidebar selector, knobs, and `simulate` stories).

---

## Scope charter — read before adding anything

This crate exists because a **narrow class of surfaces fits macroquad beautifully**.
It is **not** a UI framework and must never become one.

> We've paid the "build-a-UI-toolkit-on-a-bare-renderer" tax before — femtovg — and
> bailed to egui once custom scrolling/input piled up. This charter is the tripwire
> that stops us repeating that arc. The femtovg mistake wasn't "too low-level"; it
> was building **general-purpose UI machinery** for surfaces that turned out to need
> it. macroquad is also just pixels — so the same trap is available. The discipline
> is to only build here for surfaces that genuinely **don't** need that machinery.

### What belongs here

Stateless, immediate-mode draws for **tap-first, single-column, mostly-static**
surfaces:

- **Atoms** — text / mono, button, chip, progress bar, status dot, divider, image.
- **Molecules** — stat (label + value), list-row, quantity stepper, card/panel, tx-link.
- **Organisms** — MintCheckout, OrderFulfilment, hero / identity.
- **Views** — a bounded, fixed-layout read of one dataset: a price over time,
  holdings ranked or laid out by area, a token's whole life on one axis. Bounded
  is the word that admits it: the host decides how many rows, marks or tiles.

Rules:

1. **Stateless. VM in, actions out.** All state lives in the host app, never in the
   widget. This single rule is what keeps us out of the egui-reimplementation tarpit.
2. **Promote from real usage.** Build inline in the app first; lift to a widget once
   it's actually reused. Don't spec the API cold.
3. **One storybook story per widget.** Anything with dynamics gets a `simulate`-style
   story so the live behaviour is captured, not just static fixtures.
4. **Two fonts.** Proportional (NotoSans-Bold) for chrome, monospace (JetBrains Mono)
   for hashes / fixed-width data — `p.text` vs `p.mono`. Never mono-everywhere; it
   reads as a terminal.
5. **A view's measured state is the host's as well.** A treemap's kept cuts, a
   feed's scroll offset, a chart's range and its clock are values the host holds and
   passes in each frame (`TreemapState`, `HolderMapState`, `Gesture`). The widget
   computes and draws; it still remembers nothing between frames. The storybook's
   `trading_stories` is the worked example: the story owns the state and hands it over.

### The four stop signs

The moment a surface needs one of these, **stop** — you're sliding back toward
femtovg. Do **not** build it here:

1. **Scrolling that needs clipping** — long / virtualised lists. (A *short* list with
   a clamped wheel/drag offset is fine; clipped long-scroll is the wall — macroquad
   has no high-level clip rect. A feed of the rows the host has already bounded is
   the worked edge: `feed` draws that slice and shows a pill when the reader is
   scrolled back, rather than growing a list.)
2. **Text input / IME / a virtual keyboard.** (The wallet owns text entry.)
3. **Focus / tab management.**
4. **A layout / flex engine** — auto-sizing, reflow.

When you hit one, in order: **(a) design the need away** — paginate instead of
scroll, let the wallet handle input, use a fixed layout; **(b)** if you genuinely
can't, the surface is the **wrong shape for macroquad** — move it (see below).

### Where each surface lives

| Surface | Renderer | Why |
|---|---|---|
| Mint / reveal / order heartbeat (buyer, tap-first) | **macroquad** (this crate) | bounded, lightweight (~1.4 MB release wasm, ~½ that over the wire), survives the wallet webview |
| Token market & lifecycle (chart, holders, replay) | **macroquad** (this crate, `trading`) | one dataset, fixed layout, bounded marks; pan/zoom is the line — a browsable chart belongs in HTML |
| Operator dashboards (dense, scrolling, filtering, tables) | **egui** (`egui-widgets`) | needs the machinery the stop-signs list |
| Marketing / galleries / browse-all-N grids | **HTML / Leptos** | virtualised scroll, SEO, social cards |

**The named trap:** a "browse all N NFTs" gallery grid (thousands of items,
virtualised scroll). Do **not** build it in macroquad — that's femtovg with your name
on it. Paginate, or use HTML.

---

## Layout

[`CATALOG.md`](CATALOG.md) is the module list — one line per module, generated from
each module's own `//!` header and held current by `tests/catalog.rs`. Read it before
building anything; the list below is only the furniture a host needs around the
widgets, which the catalogue does not explain.

- `theme` — palette + `with_alpha`.
- `painter` — `Painter` (dual font + this frame's tap) draw surface + `frame_tap()`.
- `fonts` — the loader for the faces a `Painter` borrows: non-blocking, paths
  relative to the wasm, so a screen appears while the real face is still arriving.
- `assets` — loading with progress, the way a game boots.
