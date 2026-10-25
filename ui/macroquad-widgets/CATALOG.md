# macroquad-widgets catalogue

**Read this before building a widget.** One line per module, alphabetical, generated
from each module's own `//!` header by `tests/catalog.rs` — so it cannot drift.

Regenerate: `UPDATE_CATALOG=1 cargo test -p macroquad-widgets --test catalog`

Note: these are MACROQUAD widgets. They cannot use wasm-bindgen, so they do not
interchange with `egui-widgets` — see the shared-crates CLAUDE.md on runtime pairs.

18 widgets.

| module | what it is |
|---|---|
| `assets` | Loading external resources with progress, the way a game boots |
| `block_pulse` | `BlockPulse` — the chain's heartbeat on one line: a dot that pops on each new block, a ring filling towards "a block has probably landed", and the time since the last one |
| `block_train` | `BlockTrain` — recent blocks spaced by real time, with the wait since the last one growing at the right edge |
| `button` | `Button` atom — a rounded, accent button with idle / hover / pressed / disabled states and three weights (filled / tonal / ghost) |
| `choice_stepper` | `ChoiceStepper` molecule — `LABEL  [‹] value [›]`, for picking one of a list too long for a row of buttons |
| `detail_sheet` | `DetailSheet` — the detail behind a headline, out of the way until asked for: a modal on a wide screen, a sheet sliding up from the bottom on a phone |
| `floating_stick` | `FloatingStick` — a thumbstick that appears where the thumb lands, for touch surfaces with no keyboard |
| `fonts` | Host-owned font loading, for the faces [`crate::Painter`] borrows |
| `mint_checkout` | `MintCheckout` organism — the mint initiator: phase + eligibility, a [`quantity_stepper`], the live total, and the Mint CTA |
| `order_fulfilment` | `OrderFulfilment` — the buyer-facing "what's happening to my order" widget |
| `quantity_stepper` | `QuantityStepper` molecule — `[−] [ n ] [+]`, clamped to `[min, max]` |
| `radar` | `Radar` — a round scope: range rings, a turning sweep, blips that flare as the sweep passes and fade after, and an optional gauge arc around the rim |
| `route_graph` | `RouteGraph` — an amount splitting across several routes and joining again: a source node fanning out through pipes as thick as each share, one card per route, then a sink node; and `route_strip`, the same split as one labelled bar |
| `segmented` | `Segmented` molecule — a row of mutually exclusive options in one track, the picked one filled |
| `slider` | `AmountSlider` — pick an amount from a short ladder of snapping stops, for surfaces with no text input |
| `squad_picker` | Squad picker — choose N assets from a roster to deploy on a mission |
| `wallet_connect` | `WalletConnect` organism — the buyer-facing wallet selector + connected state, the front door of the mint flow |
| `wallet_list` | `WalletList` organism — a user's linked wallets, in their own order |

## Text placement

**A label in a box is placed by its capitals, through these `Painter` methods**, never
with `text_top` and an offset tuned by eye: that is how labels end up a point or two
off centre, differently per face and size. `text_in` for one line (start, centre or
end), `stack_in` for a block (a title over its subtitle). Generated from `painter.rs`.

| method | what it does |
|---|---|
| `Painter::mono` | Draw fixed-width data (a tx hash, an amount) in the monospace font |
| `Painter::top_baseline` | Baseline for text whose visual top sits at `top` — for top-anchored (box) layouts |
| `Painter::text_top` | Draw proportional text with its top at `top` (left-aligned at `x`) |
| `Painter::centre_baseline` | Baseline `y` that vertically centres text (at `size`) within a band of `height` starting at `top` — **independent of the string's own glyphs** |
| `Painter::cap_height` | Height of a capital above the baseline in the proportional face |
| `Painter::cap_height_mono` | Height of a capital above the baseline in the monospace face |
| `Painter::mid_baseline` | Baseline that puts the middle of the capitals on `mid` |
| `Painter::text_mid` | Proportional text whose capitals are centred on `mid`: a row's midline, an icon's centre |
| `Painter::mono_mid` | Monospace text whose capitals are centred on `mid` |
| `Painter::mono_top` | Monospace text with the top of its capitals at `top`: the monospace twin of [`Painter::text_top`] |
| `Painter::text_centred` | Proportional text centred in `rect` both ways, by its capitals: a stamp, a badge, a button's label |
| `Painter::mono_centred` | Monospace text centred in `rect` both ways, by its capitals |
| `Painter::text_in` | Proportional text in `rect` with its capitals on the rect's midline, against its start, its centre or its end: a row's label, a figure right against a tile's edge, a chip's word |
| `Painter::mono_in` | [`Painter::text_in`] in the monospace face |
| `Painter::stack_in` | Lines centred in `rect` as one block, each against `align`: a title over its subtitle in a list row, a figure over its caption in a tile |
