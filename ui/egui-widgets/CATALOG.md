# egui-widgets catalogue

**Read this before building a widget.** One line per module, alphabetical, generated
from each module's own `//!` header by `tests/catalog.rs` — so it cannot drift.

Regenerate: `UPDATE_CATALOG=1 cargo test -p egui-widgets --test catalog`

102 widgets.

| module | what it is |
|---|---|
| `access_gate` | `AccessGate` — the app-level access screen: a "sign in" prompt for anonymous visitors and a "requirements" screen (what to join to gain access) for signed-in-but-unqualified users |
| `activity_lanes` | `ActivityLanes` — one thin lane per party, showing WHEN it acted, under the shared spine |
| `amount_input` | ADA amount input widget with preset buttons and validation |
| `animated_counter` | AnimatedCounter — smoothly interpolates a numeric value between snapshots |
| `arrival_field` | ArrivalField — every asset a dot, every holder a pile, and now they MOVE |
| `asset_strip` | Asset strip — a horizontal row of square asset thumbnails that overlap progressively as more items are added |
| `bullet_bar` | Bullet bar — a value fill against a track with a **target marker** |
| `button_group` | `ButtonGroup` — a row of related action buttons with shared layout |
| `buttons` | Button helpers that add consistent UX behavior (pointer cursor, etc.) |
| `capital_flow` | `CapitalFlow` — a project raised some money; watch where it went |
| `card_browser` | Composable master-detail card browser widget |
| `channel_bands` | `ChannelBands` — where a wallet's money came from, period by period |
| `chip` | `Chip` — small filled-tag label with optional remove (`×`) affordance |
| `claim_card` | `ClaimCard` — an assertion, what would refute it, and whether anyone has tried |
| `collection_composition` | Collection composition — a promotable "how this collection is generated" infographic |
| `collection_list` | Collection roster — the per-client collections list rendered on the admin portal dashboard |
| `coverage_delta_bar` | Coverage delta bar — before/after progress bar for trait coverage |
| `custody_walk` | `CustodyWalk` — where a specific sum came from, unit by unit |
| `data_table` | Data table — dense row-based table with column headers, selection, and optional detail panel |
| `distribution_waterfall` | `DistributionWaterfall` — how a buyer's payment flows down to what lands in each party's wallet under settle-as-you-mint |
| `donut_chart` | `DistributionChart` — a donut of banded shares with a legend and hover tooltip, for "how is this split" questions |
| `error_note` | `ErrorNote` — turns an ugly machine error string into a readable note |
| `exposure_bar` | Exposure bar — stacked horizontal bar showing total ADA exposure segmented by collateral token, colored by LTV risk |
| `fee_report` | Fee report widget — displays per-side fee breakdown for a trade |
| `file_upload` | File upload widget — opens a browser file picker and reads the selected file |
| `flip_counter` | FlipCounter — split-flap style animated digit counter |
| `flow_ledger` | `FlowLedger` — a wallet's movements in time order: what arrived, what left, who was on the other side, and what the balance was after each step |
| `flow_matrix` | `FlowMatrix` — who paid whom, across MANY wallets at once |
| `flow_ring` | `FlowRing` — value moving between parties, live, on the shared spine |
| `flow_stave` | `FlowStave` — one wallet's money story as a sequence chart on the spine |
| `focus_list` | Focus list — a fixed-geometry master–detail widget for inspecting one item out of many in a constrained surface (typically a pinned chart tooltip) |
| `fungibles_row` | Fungibles row — single horizontal row for a Cardano Native Token holding |
| `gated` | Entitlement-gated rendering — the frontend half of the `authorizations` framework |
| `grouped_section` | Grouped section header — hero icon + title + verified badge + subtitle + right-aligned bulk-action button, with caller-rendered body below |
| `holder_field` | HolderField — the holder graph, reshuffling as assets change hands |
| `holder_formation` | `HolderFormation` — people arriving, and how evenly the collection lands |
| `id_pill` | `IdPill` — small inline display of a long identifier with a copy affordance |
| `image_text_editor` | Image text overlay editor — place, style, and drag text on top of an image |
| `leaderboard` | `Leaderboard` — ranked standings: podium-tinted ranks, an optional prize thumbnail, one headline metric, supporting stats, and a share bar |
| `leaderboard_table` | `LeaderboardTable` — a dense, virtual-scrolled ranked table |
| `listing_grid` | `ListingGrid` — a responsive grid of marketplace listing cards, each with a lazily-loaded image, price and trailing badges |
| `managed_wallet_utxos` | Managed-wallet UTxO breakdown — a structured, role-aware view of a custodial wallet's on-chain UTxOs |
| `marquee` | Scrolling marquee ticker widget |
| `metric_card` | MetricCard — a dashboard stat card with label, value, optional trend, and sparkline |
| `mint_arrivals` | `MintArrivals` — watch a collection land in people's hands, one asset at a time |
| `mint_checkout` | `MintCheckout` — the buyer-facing mint offer + CTA, composed as one widget |
| `mnemonic_display` | BIP-39 mnemonic display with copy-to-clipboard + optional confirmation gate |
| `named_group_list` | named_group_list — a list of named groups, each a `name` + a member multiselect (+ an optional boolean flag like "allow none") |
| `offer_slot` | Offer slot widget — a single asset card placed on a trade table |
| `offer_tile` | `OfferTile` — fixed-size picker tile with state-aware visual treatment and a top-right quantity badge |
| `order_list` | `OrderList` — the mint-orders dashboard |
| `palette_editor` | palette_editor — edit colorization palettes: each palette has a name, a base color, and a list of variants (name + color + weight) |
| `party_annotator` | `PartyAnnotator` — decide what a wallet IS to the project, on the record |
| `party_badge` | `PartyBadge` — a counterparty as it should appear everywhere in a forensic trace: resolved name, **how firmly that name is known**, and the cluster it belongs to |
| `party_finder` | PartyFinder — hunt down a wallet by ANY of its names, then watch it |
| `persona_strip` | Persona strip — italic one-liner describing a wallet (or any tagged entity), with an optional row of small tag chips beneath |
| `phase_card` | `PhaseCard` — read-only display of one mint phase row |
| `pip_row` | Horizontal pip row widget — a label on the left and a bar of colored pips (or density heatmap) on the right, each positioned proportionally by value |
| `pool_liquidity_indicator` | Pool liquidity indicator — per-pool depth and health context cards |
| `price_impact_curve` | Price impact curve chart — visualizes why split routing helps |
| `price_timeline` | Price timeline widget — a time-axis scatter of realized prices (sales, offer fills) with optional reference lines (current floor / listing price) and a reference band (e.g |
| `printing_timeline` | Printing timeline widget — shows a card's printings across sets over time |
| `progress_bar` | Themed progress bar widget with optional label, percentage, and countdown |
| `property_list` | `PropertyList` — compact label/value grid for read-only key data |
| `quantity_stepper` | `QuantityStepper` — a compact `−  [n]  +` control with min/max clamping |
| `radar_chart` | Radar / spider chart widget for N-dimensional normalized data |
| `range_bar` | Horizontal range bar widget for visualizing a set of labeled price/value points along a gradient |
| `rarity_target_editor` | rarity_target_editor — a labelled list of 0–100% target sliders with an optional budget indicator (running total vs a budget, coloured over/under/ok) |
| `relationship_editor` | relationship_editor — edit a list of directed `source → target` edges over a known option set |
| `relative_time` | `RelativeTime` — a tiny auto-scaling "time ago" label |
| `route_summary` | Route summary widget — compact display of split routing results |
| `seven_segment` | SevenSegmentDisplay — retro LED-style numeric display |
| `signing_status` | Signing status widget — concurrent signing checklist for the trade desk |
| `slippage_selector` | Reusable slippage selector widget |
| `slot_table` | slot_table — the trait/slot list with enable / required toggles and an optional z-order field |
| `sparkline` | Sparkline widget — compact inline line chart for trend visualization |
| `split_allocation_bar` | Split allocation bar — segmented horizontal bar showing ADA allocation across multiple DEXes |
| `stat_strip` | StatStrip — a horizontal row of windowed summary "stat cards" |
| `supply_bar` | Two-band mint supply bar — `minted` (on chain) + `ordered` (the backlog of ordered-but-not-yet-minted units), over the unsold track |
| `swap_modal` | Reusable swap modal widget for egui frontends |
| `tag_list` | Tag list — a wrapping row of removable tags with an optional trailing "clear all" button |
| `time_spine` | TimeSpine — ONE time axis for a surface of many faces |
| `timestamp` | `Timestamp` — a tiny atom that renders a unix-seconds timestamp **consistently** as ISO-8601 (UTC), with an optional clean badge presentation |
| `toast` | `Toast` / `ToastQueue` — transient overlay messages with frame-countdown auto-dismiss |
| `token_multiselect` | token_multiselect — pick a subset from a known set of options |
| `trade_flow` | Trade-flow widget — the local user's view of a P2P swap in plain give / get / net terms, decoupled from the raw eUTxO structure |
| `trade_table` | Trade table widget — TCG-style top/bottom offer display for the trade desk |
| `trait_delta` | Trait delta widget — shows traits gained and lost in a trade |
| `trait_filter` | Compound-key prefix trie tag filter widget |
| `tx_cart` | TX Cart widget — displays a list of pending chain actions with batch execution |
| `tx_estimate` | Per-wallet transaction estimate widget — shows the local user's ADA impact |
| `typeahead_search` | `TypeaheadSearch` — a search box with a keyboard-navigable result dropdown |
| `user_badge` | `UserBadge` — a compact "logged in as" pill (avatar + name) with a click-to-open popup carrying a sign-out action |
| `utxo_map` | UTxO terrain map — a Voronoi-based wallet visualization |
| `utxo_shelf` | UTxO Shelf — wallet health visualization |
| `variant_split` | Variant split — explains a `variant_flow` source slot's **derived** variant distribution and *why* it isn't uniform |
| `wallet` | Framework-agnostic wallet connector for egui frontends |
| `wallet_asset_picker` | Wallet asset picker — modal widget for browsing and selecting NFTs from a wallet, grouped by policy in an accordion layout |
| `wallet_button` | Reusable wallet connection button widget for egui frontends |
| `wallet_editor` | Wallet bundle editor widget |
| `wallet_identity_header` | Wallet identity header — the big "this is who we're showing" strip at the top of a wallet-profile view |
| `wallet_list` | Wallet roster — the per-client list rendered on the admin portal dashboard |
