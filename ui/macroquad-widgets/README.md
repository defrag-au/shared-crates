# macroquad-widgets

VM-driven, immediate-mode widgets for **buyer-facing macroquad surfaces** — the
txmints mint app and its kin (mint checkout, order/fulfilment heartbeat, reveal).
A widget takes a [`Painter`](src/painter.rs) (fonts + this frame's tap) and a plain
VM, draws, and returns actions. No async, no I/O, no backend types, **no retained
widget state**. Mirrors `egui-widgets` (VM in → actions out) so the same projection
pattern works on both renderers. Compiles native + wasm32.

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

### The four stop signs

The moment a surface needs one of these, **stop** — you're sliding back toward
femtovg. Do **not** build it here:

1. **Scrolling that needs clipping** — long / virtualised lists. (A *short* list with
   a clamped wheel/drag offset is fine; clipped long-scroll is the wall — macroquad
   has no high-level clip rect.)
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
| Operator dashboards (dense, scrolling, filtering, tables) | **egui** (`egui-widgets`) | needs the machinery the stop-signs list |
| Marketing / galleries / browse-all-N grids | **HTML / Leptos** | virtualised scroll, SEO, social cards |

**The named trap:** a "browse all N NFTs" gallery grid (thousands of items,
virtualised scroll). Do **not** build it in macroquad — that's femtovg with your name
on it. Paginate, or use HTML.

---

## Layout

- `theme` — palette + `with_alpha`.
- `painter` — `Painter` (dual font + this frame's tap) draw surface + `frame_tap()`.
- `order_fulfilment` — the `OrderFulfilment` widget (status heartbeat + 1 order → N
  fulfilment txs).
