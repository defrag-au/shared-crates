# macroquad-widgets catalogue

**Read this before building a widget.** One line per module, alphabetical, generated
from each module's own `//!` header by `tests/catalog.rs` — so it cannot drift.

Regenerate: `UPDATE_CATALOG=1 cargo test -p macroquad-widgets --test catalog`

Note: these are MACROQUAD widgets. They cannot use wasm-bindgen, so they do not
interchange with `egui-widgets` — see the shared-crates CLAUDE.md on runtime pairs.

16 widgets.

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
| `segmented` | `Segmented` molecule — a row of mutually exclusive options in one track, the picked one filled |
| `slider` | `AmountSlider` — pick an amount from a short ladder of snapping stops, for surfaces with no text input |
| `squad_picker` | Squad picker — choose N assets from a roster to deploy on a mission |
| `wallet_connect` | `WalletConnect` organism — the buyer-facing wallet selector + connected state, the front door of the mint flow |
| `wallet_list` | `WalletList` organism — a user's linked wallets, in their own order |
