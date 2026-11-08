# tx-grammar

One transaction as a **phrase**, not a sentence.

`tx-grammar` is the semantic layer between a producer (`tx-classifier`, a
`FlowRow`, an OpenSea frame, a Midnight ledger read) and a renderer (the egui
explorer, a macroquad client, an OG card, a Discord message, a text-only page).
It keeps the composition that a union-of-verbs model throws away.

It is **not** a classifier. It does not fetch, detect, score, resolve names,
choose colours, or know a collection's name. It has no thresholds, no policy
IDs, and no `Display` impls outside [`text`](src/text.rs).

## The three layers

```text
raw data → Primitive[] → Reading[] → Phrase { Cell[] } → renderer
                                                   ├── tokens()
                                                   └── sentence()
```

1. **Primitives** — nine physical actions.
2. **Readings** — the domain vocabulary, derived from primitives.
3. **Phrase** — the renderer-neutral projection. A `Cell` carries the semantic
   marks; it never carries a colour, an icon or a pixel size.

## Primitives

| Primitive | What it means |
|---|---|
| `Move` | Value or an asset changed hands. |
| `Create` | An asset came into existence. |
| `Destroy` | An asset ceased to exist. |
| `Hold` | Custody changed in place. It says `from` and `to` custody. |
| `Declare` | An intention: an ask, a bid, an auction. |
| `Retract` | An intention withdrawn. |
| `Revise` | An intention changed, with the old and new figure where known. |
| `Grant` | A permission: an ERC-20 allowance, an ERC-721 operator. |
| `RevokeGrant` | A permission withdrawn. |

Everything else is a composition:

| Domain event | Primitives |
|---|---|
| sale | `Move(item, seller→buyer)` + `Move(price, buyer→seller)` |
| offer accepted | the same two moves + `Retract(bid)` |
| listing | `Hold(item, seller, InHand→Escrow)` + `Declare(ask)` |
| delisting | `Hold(item, seller, Escrow→InHand)` + `Retract(ask)` |
| offer created | `Hold(item, bidder, InHand→Escrow)` + `Declare(bid)` |
| offer cancelled | `Hold(item, bidder, Escrow→InHand)` + `Retract(bid)` |
| price change | `Revise(ask, old, new)` |
| mint | `Create(asset, to)` |
| burn | `Destroy(asset, from)` |
| transfer | `Move(asset, from, to, InHand)` |
| swap | `Move(asset A, A→B)` + `Move(asset B, B→A)` |
| lock / release | `Hold(asset, holder, InHand↔Script)` |
| stake / unstake | `Hold(asset, holder, InHand↔Staked)` |
| vest | `Hold(…, InHand↔Script)` with `Mechanism::Vesting` |
| self transfer | `Move` with both actors `Role::Own` |
| bundle / sweep | `Object::Lot(distinct, total)` + one reading per action |
| EVM approval | `Grant` / `RevokeGrant` |

A listing is deliberately **not** a `Move`. The UTxO goes to a marketplace
script; ownership does not. Rendering it as a transfer is how a marketplace
became the largest holder on a collection page.

A claim's `Hold` is about the **subject**, not the money. A bid holds against
the thing being bid on; the figures live on the `Declare`. This is what lets a
`Hold` + `Declare` pair collapse into one cell, and it is why a seller's
listing and a buyer's offer over the same item are the same two primitives.

## The axes

Every `Cell` carries the same axes, whatever the chain or the venue:

| Axis | Values |
|---|---|
| `reading` | sale, list, offer, mint, burn, swap, lock, stake, approve, … |
| `object` | one asset, a lot, a collection, or a hidden asset |
| `figure` | paid / ask / bid / fee / refund / price, plus its own asset |
| `actor` | role (`Own`, `Peer`, `Venue`, `Contract`, `Pool`, `Network`) and party |
| `other` | the far end of a movement, when the phrase has no owner to stand on |
| `direction` | in, out, both, in place, unchanged |
| `custody` | in hand, escrow, script, staked |
| `basis` | observed, derived, asserted, inferred, proven |
| `outcome` | succeeded, reverted, failed, pending, dropped, unknown |
| `visibility` | public, shielded, unknown |
| `absence` | hidden, unknown, ambiguous |
| `count` / `more` | aggregated cells and overflow |

`actor` and `other` are the two ends of a movement. From a wallet, `actor` is
the counterparty and `other` is absent — naming an own side twice says nothing.
From a pair (a policy feed), both ends are facts, so `other` carries the second
party and the cell draws `actor → other`.

A figure carries **its own asset**. An EVM sale settled in WETH is
`paid 0.000227 WETH`, not `0.000227 ADA`. A USDC bid is not an ETH bid. A
`Quantity` is 256-bit and decimal on the wire, because an EVM token id or balance
does not fit a `u64`.

## Identity is `chains`, not a new model

`tx-grammar` depends on [`chains`](../chains) for identity:

- `ChainRef` — `cardano:mainnet`, `eip155:4663`, `midnight:mainnet`, …
- `TokenRef` / `AssetType` — CAIP-19, e.g.
  `eip155:4663/erc721:0x…/781` or `cardano:mainnet/cnt:<policy>/<name>`.
- `ChainFamily` — the mechanics axis.

A `Phrase` is single-chain. A bridge is a `Composition::AcrossChains` sequence
of steps with different chains, linked by `LinkKind::CrossChain`. There is no
single "net" across chains; reconciliation is per asset.

## The honesty rules

1. **A party has a key and a label, and several ways to be absent.**
   `BelowFloor` resolves by deepening; `Ambiguous(n)` never does; `Untraced` is
   an EVM internal transfer nobody followed; `Shielded` is hidden by design.
   They are not one `Option`.
2. **An intention is not a settlement.** `FigureKind::Ask` is not `Paid`. Only
   a `Sale` or `Accept` carries a paid price; a listing carries an asking price.
3. **A bundle price is a lot price.** `Figure.per` says item / lot / total, so a
   bundle of five at 500 is never rendered as five at 500. A settlement's `per`
   is derived from its subject: a lot's price is the lot's. A `Declare` carries
   the figure the producer set, so a listing or bid says `Per::Lot` itself.
4. **A sampled lot still counts every item.** `Object::lot_sampled(items, n, …)`
   keeps the honest total when the producer capped what it could carry and name.
5. **A semantic count is not a raw count.** A CIP-68 mint of five NFTs is five
   items, not ten including the reference tokens; an ERC-1155 id at quantity
   five is one item at five units, not five items.
6. **A cancellation the source cannot attribute is ambiguous.** OpenSea's
   `item_cancelled` fires for both a listing and a bid. The adapter emits a
   `Retract` with `ClaimKind::Unknown`; the grammar never guesses `Delist`.
7. **A reverted transaction is not a completed action.** `Step.outcome` is
   `Reverted`, and every cell inherits it. The text says "Attempted" and
   "The transaction reverted."
8. **A shielded amount is hidden, not zero.** A Midnight object is
   `Object::Hidden`, a shielded party is `Party::Shielded`, and the amount is
   absent. The grammar never invents either.
9. **No prose in the model.** `token` and `sentence` live in `text`; the visual
   renderers draw cells.

## Forward projections

| Projection | Use |
|---|---|
| `Phrase::cells()` | The compact card strip: one cell per reading. |
| `Phrase::tokens()` | The canonical linear line for logs, capture notes, debug. |
| `Phrase::sentence()` | The text-only interface, OG body and test oracle. |
| `Step::primitives` | The detail sheet: the physical actions beneath a reading. |

A cell's `label()` is its reading's one word, so a glyph-only cell cannot appear
without a word the capture report and the text renderer can verify.

Example — a batched Wayup submission:

```text
list · 3 items · ask 343 ADA · wayup · escrow
∥ offer · collection aaaaaa…aaaa · bid 147 ADA · buyer · escrow
```

The same phrase as a sentence:

```text
On cardano:mainnet: Listed 3 items on wayup at 343 ADA. Offered 147 ADA for a collection.
```

Example — an EVM sale:

```text
sale · Madjacket #781 · paid 0.000227 WETH · seller
```

Example — a Midnight transfer whose parties and amount are hidden:

```text
transfer · shielded asset · a shielded party · shielded
```

## What is derived today

The matcher currently derives: `Sale`, `Accept`, `Listing`, `Offer`, `Transfer`,
`SelfTransfer`, `Mint`, `Burn`, `Swap`, `Wrap` (with `Mechanism::Wrap`), `Lock`,
`Release`, `Stake`, `Unstake`, `Vest` (with `Mechanism::Vesting`), `Approve`,
`RevokeApproval`, `PriceChange`, `Delist`, `CancelOffer`, `Cancel`, `Unknown`.

`Borrow`, `Repay`, `Liquidate`, `Shield` and `Unshield` are reserved vocabulary
for adapters that can establish them; they are not guessed from a shape.

## Producer and consumer map

| Crate | Role |
|---|---|
| `tx-classifier` | Cardano detector; `TxClassification::phrase(&Viewpoint)` wraps every `TxType` into primitives. |
| `types/shared` (`flow_explorer`, `policy_feed`) | Cardano row projections: `flow_row()` and `policy_row()` into `Sequence`. |
| `evm/evm-chain-reader` + `evm/opensea-stream` | EVM adapter: `Transfer`, `ItemSold`, `ItemListed`, `ItemCancelled`, `PaymentToken`. |
| `tx-verdict` | Social-card view derived from `Phrase`; `Shape`/`Headline`/`Tags` become a lossy projection. |
| `tx-insights` | Notification projection derived from readings. |
| `text` | The only prose renderer. |

The grammar sits underneath those; none of them should define a parallel
vocabulary.

## Tests

`tests/shapes.rs` is the shape sheet. It covers Cardano sale/listing/batch/
accept/pair/lock/bundle, EVM WETH sale/ERC-1155/ambiguous cancellation/approval/
reverted/untraced, Midnight shielded transfer, and a cross-chain bridge. The
source captures that these shapes represent live with the producers:

- Cardano: `pipeline/tx-classifier/resources/test/txs/*`
- EVM: `evm/evm-chain-reader/tests/transfers.rs`,
  `evm/opensea-stream/tests/{frames,movements,market}.rs`
- Midnight: hand-built until a ledger producer exists.

Run:

```sh
direnv exec . cargo test -p tx-grammar
direnv exec . cargo clippy -p tx-grammar --all-targets -- -D warnings
```
