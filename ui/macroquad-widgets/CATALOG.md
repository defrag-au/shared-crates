# macroquad-widgets catalogue

**Read this before building a widget.** One line per module, alphabetical, generated
from each module's own `//!` header by `tests/catalog.rs` — so it cannot drift.

Regenerate: `UPDATE_CATALOG=1 cargo test -p macroquad-widgets --test catalog`

Note: these are MACROQUAD widgets. They cannot use wasm-bindgen, so they do not
interchange with `egui-widgets` — see the shared-crates CLAUDE.md on runtime pairs.

34 widgets.

| module | what it is |
|---|---|
| `assets` | Loading external resources with progress, the way a game boots |
| `block_pulse` | `BlockPulse` — the chain's heartbeat on one line: a dot that pops on each new block, a ring filling towards "a block has probably landed", and the time since the last one |
| `block_train` | `BlockTrain` — recent blocks spaced by real time, with the wait since the last one growing at the right edge |
| `button` | `Button` atom — a rounded, accent button with idle / hover / pressed / disabled states and three weights (filled / tonal / ghost) |
| `callout` | `Callout` — one sentence about what just happened, in the colour that says how it went, with an optional action at its end |
| `change_chip` | `ChangeChip` — a price's change as an arrow and a signed percentage, with the stretch it is measured over under it |
| `choice_stepper` | `ChoiceStepper` molecule — `LABEL  [‹] value [›]`, for picking one of a list too long for a row of buttons |
| `detail_sheet` | `DetailSheet` — the detail behind a headline, out of the way until asked for: a modal on a wide screen, a sheet sliding up from the bottom on a phone |
| `feed` | `Feed` — a stack of activity rows, newest on top: a chip for what happened, who or what it was, the amount and the time, and the newest row sliding in |
| `floating_stick` | `FloatingStick` — a thumbstick that appears where the thumb lands, for touch surfaces with no keyboard |
| `fonts` | Host-owned font loading, for the faces [`crate::Painter`] borrows |
| `holder_board` | `HolderBoard` — holdings ranked, as a holders list shows them: rank, a chip for what is not a person, the amount and the share, with the share as a bar behind the row |
| `holder_map` | `HolderMap` — holdings by area, grouped by what they are, on a treemap that holds still as shares move |
| `life_strip` | `LifeStrip` — a token's whole life on one axis: a fuzzy price band, how busy each stretch was, the window being replayed as a brush, and the playhead |
| `mint_checkout` | `MintCheckout` organism — the mint initiator: phase + eligibility, a [`quantity_stepper`], the live total, and the Mint CTA |
| `momentum` | `Momentum` — inertial scrolling for a list a finger drags: the content follows the finger while it is down, and after a flick it keeps going and slows to a stop, as a phone's own lists do |
| `order_fulfilment` | `OrderFulfilment` — the buyer-facing "what's happening to my order" widget |
| `pill` | `Pill` — a modal's buttons: fully rounded, drawn in the sheet's own fade so they leave with it |
| `price_chart` | `PriceChart` — a price over time as a step line, with the volume it traded split into buys and sells at the foot, a mark for each trade while there is room for one, the playhead, and a finger's scrub |
| `quantity_stepper` | `QuantityStepper` molecule — `[−] [ n ] [+]`, clamped to `[min, max]` |
| `radar` | `Radar` — a round scope: range rings, a turning sweep, blips that flare as the sweep passes and fade after, and an optional gauge arc around the rim |
| `route_graph` | `RouteGraph` — an amount splitting across several routes and joining again: a source node fanning out through pipes as thick as each share, one card per route, then a sink node; and `route_strip`, the same split as one labelled bar |
| `scroll` | `Scroll` — one scrolling container's offset, as a phone's lists move: the content follows a finger that drags it, coasts on after a flick and slows to a stop, and a wheel or trackpad moves it too |
| `segmented` | `Segmented` molecule — a row of mutually exclusive options in one track, the picked one filled |
| `shape` | Rounded shapes and gradient fills: the surfaces every card, sheet, badge and pill is drawn from |
| `slider` | `AmountSlider` — pick an amount from a short ladder of snapping stops, for surfaces with no text input |
| `split_bar` | `SplitBar` — one horizontal bar split between two amounts, with a label at each end and a marker at the split |
| `squad_picker` | Squad picker — choose N assets from a roster to deploy on a mission |
| `stat_tiles` | `StatTiles` — a row of equal stat tiles: a figure over its caption, with an optional ADA mark, sharing one baseline and one width |
| `token_header` | `TokenHeader` — a token's identity and replay controls on one line: a disc with its picture or its initial, the ticker, the venues it trades on, the mode chip, and the settings / speed / pause buttons |
| `treemap` | `Treemap` — weights laid out as rectangles whose areas are proportional, by a kept tree of cuts that is built once and only re-weighted, then drawn with labels where they fit |
| `wallet_connect` | `WalletConnect` organism — the buyer-facing wallet selector + connected state, the front door of the mint flow |
| `wallet_list` | `WalletList` organism — a user's linked wallets, in their own order |
| `well` | `Well` — facts in a panel of their own: a recessed, bordered plate with a row per fact and a hairline between them |

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
