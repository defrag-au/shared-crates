# macroquad-widgets catalogue

**Read this before building a widget.** One line per module, alphabetical, generated
from each module's own `//!` header by `tests/catalog.rs` — so it cannot drift.

Regenerate: `UPDATE_CATALOG=1 cargo test -p macroquad-widgets --test catalog`

Note: these are MACROQUAD widgets. They cannot use wasm-bindgen, so they do not
interchange with `egui-widgets` — see the shared-crates CLAUDE.md on runtime pairs.

7 widgets.

| module | what it is |
|---|---|
| `button` | `Button` atom — a rounded, accent button with idle / hover / pressed / disabled states and three weights (filled / tonal / ghost) |
| `mint_checkout` | `MintCheckout` organism — the mint initiator: phase + eligibility, a [`quantity_stepper`], the live total, and the Mint CTA |
| `order_fulfilment` | `OrderFulfilment` — the buyer-facing "what's happening to my order" widget |
| `quantity_stepper` | `QuantityStepper` molecule — `[−] [ n ] [+]`, clamped to `[min, max]` |
| `squad_picker` | Squad picker — choose N assets from a roster to deploy on a mission |
| `wallet_connect` | `WalletConnect` organism — the buyer-facing wallet selector + connected state, the front door of the mint flow |
| `wallet_list` | `WalletList` organism — a user's linked wallets, in their own order |
