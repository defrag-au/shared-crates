# egui-widgets — Catalog

One-line index of every widget in the crate, grouped by use case. Before
building anything UI-shaped from scratch, scan this list first. Every
entry follows the same shape:

> `module` → `MainType` — what it renders — **when to reach for it**

For full API: `cargo doc --open -p egui-widgets`. For visual examples:
run the storybook (`shared-crates/ui/_storybook-egui`) and navigate to
the matching story.

---

## Primitives

Small composables — labels, chips, pills, counters. Cheap, no
domain assumptions. Stack these into larger displays.

- **`chip`** → `Chip` — Small filled-tag label with semantic variants
  (`Success` / `Warning` / `Danger` / `Tag` / `Info` / `Muted`) +
  optional `×` remove. **When you need a status / category / role
  indicator that says what it means, not what colour it is.**
- **`id_pill`** → `IdPill` — `label policy_id_truncated [Copy]` — owns
  middle-elision + clipboard. **When you'd otherwise hand-build the
  recurring "label + ellipsed hex + copy" pattern (policy_id, addresses,
  tx hashes).**
- **`property_list`** → `PropertyList` — Label/value grid via `.add(k,v)` /
  `.add_optional(k, opt)`. **When you'd otherwise hand-build an
  `egui::Grid` of "field: value" descriptive rows.**
- **`animated_counter`** → `AnimatedCounter` — Smoothly interpolates a
  numeric value between data snapshots. **When the value updates on a
  poll cadence and you want it to feel alive (vs jumping).**
- **`flip_counter`** → `FlipCounter` — Split-flap digit counter with
  perspective. **When the kinetic vibe matters — leaderboards, scoreboards.**
- **`seven_segment`** → `SevenSegmentDisplay` — Painter-drawn LED-style
  digits. **When you want a scoreboard/clock aesthetic for time or
  integers.**
- **`progress_bar`** → `ProgressBar` — Themed determinate bar with
  optional countdown / label modes. **Any determinate progress display
  — supply minted, sync ratio, time remaining.**
- **`marquee`** → `Marquee` — Continuously scrolling ticker of coloured
  items. **Live feeds, announcements, anything ambient horizontal.**
- **`button_group`** → `ButtonGroup` — Row of related action buttons
  (`add(ButtonGroupButton::new(id, "label")…)`) with optional Phosphor
  icons + tooltips + disabled state, `horizontal_wrapped` by default.
  Returns the clicked button's caller-supplied `id`. **Any time you'd
  otherwise stack 3+ small_button calls into an inline action bar.**
- **`toast`** → `ToastQueue` + `show_toasts` — Transient overlay
  messages with frame-countdown auto-dismiss; Success / Error / Warning
  / Info kinds, host-owned queue, bottom-right stack. **Acknowledging
  an action without committing to a status bar — "Copied to clipboard",
  "Save failed", "Refuel submitted".**

## Cards & rows

Composed display blocks for a single domain entity. One card = one
row of data. These compose primitives internally.

- **`metric_card`** → `MetricCard` — Label + large value + optional trend
  + optional sparkline. **Dashboard KPIs.**
- **`phase_card`** → `PhaseCard` — Read-only mint phase: header (name +
  status + priority + Edit/Delete), Price/Window/Per-wallet via
  `PropertyList`, gate chips with × remove. **Mint configuration UI.**
- **`asset_card`** → `CardEffect` (+ projection / geometry / mesh /
  overlay) — 3D-projected NFT card with tilt + holo + rarity border.
  **Hero asset display where presentation matters.**
- **`offer_tile`** → `OfferTile` — Fixed-size picker tile with
  Active/InCart/Spent state + quantity badge. **Cart-driven asset
  selection.**
- **`offer_slot`** → `OfferSlotData` — Square card with IIIF thumbnail
  + name overlay on hover. **Trade-table asset slots.**
- **`fungibles_row`** → `FungiblesRow` — Compact "icon | name | ticker
  chip | qty (ADA value)" row. **CNT holdings list (no thumbnails).**
- **`pip_row`** → `PipRow` — Label + horizontal bar of coloured pips OR
  density heatmap. **Distributions, market depth, listing spreads.**

## Lists & tables

Collections with a row-VM input + drained-action-list output. Same
shape: builder → `.show(ui) -> Response { actions: Vec<…> }`.

- **`collection_list`** → `CollectionList` — Per-client collections
  roster (Card / List layouts). **Mint dashboard, collection management.**
- **`wallet_list`** → `WalletList` — Per-client wallets grouped by role
  (Primary / Collection / Custom). **Wallet management.**
- **`data_table`** → `DataTable` — Dense column-headed table with row
  selection + expandable detail. **Numeric dashboards (loans, audit).**
- **`listing_grid`** → `ListingGrid` — Marketplace card grid with
  thumbnails, prices, hover details. **Listing browsers.**
- **`card_browser`** → `CardBrowserConfig` — Master-detail card grid
  with right detail slider. **Browse + drill-in flows.**
- **`trade_table`** → trade-offer display (split top/bottom) — Two-party
  trade workspace with NFT slots + ADA sweetener + lock mechanics.
  **Trade-desk concurrent dual-side offers.**
- **`asset_strip`** → `AssetStripItem` row — Horizontal overlapping
  thumbnails that lift on hover. **Compact related-assets row.**

## Charts & visualisations

- **`donut_chart`** → `DistributionChart` — Concentric orbital rings
  with arc-layout toggle. **Proportional distribution in compact form.**
- **`radar_chart`** → `RadarChartConfig` + `RadarPoint` — N-axis spider
  chart. **Multi-axis profiles (trait coverage, wallet metrics).**
- **`sparkline`** → `Sparkline` — Inline mini line chart with optional
  fill + reference + endpoint highlight. **Embed in metric cards.**
- **`range_bar`** → `RangePoint` + `RangeBarConfig` — Gradient bar with
  ticks + coloured dots + auto-staggered labels. **Price/value points
  along a continuous axis.**
- **`exposure_bar`** → `ExposureSegment` — Stacked bar by collateral
  token, coloured by LTV risk. **Loan-dashboard ADA exposure.**
- **`coverage_delta_bar`** → `CoverageDeltaConfig` — Before/after
  progress with gain/loss regions. **Trait-coverage trade impact.**
- **`split_allocation_bar`** → `AllocationSegment` — Stacked bar
  segmented by DEX with % labels. **Split-routing allocation display.**
- **`variant_split`** → `VariantSegment` + `VariantSplitConfig` — Derived
  variant distribution for a `variant_flow` source slot: coloured shares +
  per-variant asset counts + a dotted uniform baseline, with a generated
  "why" caption. **Explaining cardinality-weighted CSP splits (why a
  variant isn't 50/50).**
- **`collection_composition`** → `CollectionComposition` (+ `CompositionLayer`
  / `CompositionFlow` / `CompositionStat` / `CompositionConfig`) — Promotable
  "how this collection generates" infographic: z-ordered layer stack (front
  on top) with per-layer presence bar, option count, variant badges, and
  curved `variant_flow` connectors in a gutter, under a headline stats band.
  **A read-only collection overview you could screenshot to promote it.**
- **`price_impact_curve`** → `ImpactCurvePool` — AMM impact curves per
  pool with optimizer allocation overlay. **Split-routing optimisation
  rationale.**
- **`pool_liquidity_indicator`** → `PoolInfo` — Per-pool depth + health
  with relative depth bar + TVL + impact tint. **DEX pool comparison.**
- **`utxo_map`** → `UtxoCell` Voronoi terrain — Force-directed UTxO
  visualisation with policy territories + water/land. **Wallet
  composition as an organic map.**
- **`utxo_shelf`** → `ShelfTier` — UTxO health classification into
  shelves (collateral/liquid/clean/bloated/dust). **Fragmentation
  diagnosis.**

## Inputs & form controls

- **`amount_input`** → `AmountInputState` — Text input with "ADA" suffix
  + MAX button + preset chips + validation. **ADA amount entry with
  wallet-balance awareness.**
- **`slippage_selector`** → `SlippageSelectorState` — Preset chips
  (0.5%, 1%, 3%) + custom + warning bands. **DEX slippage tolerance.**
- **`trait_filter`** → `FilterEntry` + `TraitFilterConfig` — Prefix-trie
  search over `category:value` with removable tag chips. **Hierarchical
  trait filtering.**
- **`file_upload`** → `FileUploadButton` *(wasm32 only)* — Browser file
  picker via async inbox. **Web-only file uploads.**
- **`image_text_editor`** → `ImageTextEditor` *(feature-gated)* — Place,
  style, drag text overlays on an image; flatten to RGBA. **Composite
  image+text authoring.**
- **`wallet_asset_picker`** → `PickerAsset` + `PickerPolicyGroup` —
  Modal browser grouped by policy with multi-select summary bar.
  **Pick NFTs from wallet inventory.**

## Wallet / identity

- **`wallet`** → `WalletConnector` — Framework-agnostic CIP-30 wallet
  state (connection, addresses, balance, API). **State container for
  any wallet-aware UI.**
- **`wallet_button`** → `WalletAction` + `WalletConnector` display —
  Compact status bar with picker popup. **Connect/disconnect entry
  point.**
- **`wallet_editor`** → `WalletEditorEntry` + `WalletEditorAction` —
  List editor for stake addresses / handles with resolving / loading /
  ready / failed states. **Wallet bundle management flows.**
- **`wallet_identity_header`** → `WalletIdentityAction` — Hero strip:
  handle (or short address) + full address + copy. **"Whose profile?"
  page header.**
- **`persona_strip`** → `PersonaStrip` — Italic one-liner tagline with
  optional chip row. **Wallet / collection persona summary.**
- **`mnemonic_display`** → `MnemonicDisplay` — BIP-39 phrase on dark
  card with warning + copy + confirmation gate. **Show-mnemonic-once
  flows (provisioning, Art. 20 export).**
- **`signing_status`** → `SigningPhase` — Two-row signing checklist
  with Sign + submission state. **Multi-party signing progress.**

## Section / layout

- **`grouped_section`** → `GroupedSection` + `GroupedSectionAction` —
  Hero icon + title + verified badge + subtitle + bulk-action button +
  caller-rendered body. **Grouping items under a parent entity with a
  bulk action.**

## Domain-specific

Specialised — only reach for these in the matching domain. Don't try to
generalise them into other contexts.

- **`fee_report`** → `FeeReportData` + `SideFeeData` — Two-party trade
  fee breakdown (platform + network + min UTxO + net ADA per side).
- **`route_summary`** → `RouteSummaryData` + `RouteLeg` — Split-routing
  per-leg display with improvement-vs-best-single-pool.
- **`swap_modal`** → `SwapModalAction` — Self-contained DEX swap UI
  (amount + slippage + preview + confirm + processing + success).
- **`tx_cart`** → `TxCartItem` — Pending chain action list with
  per-item counts + providers + batch execute.
- **`tx_estimate`** → `TxEstimateData` — Per-wallet ADA cost breakdown
  during trade negotiation.
- **`trade_flow`** → `TradeFlowData` + `FlowAsset` — Plain give / get / net
  view of a swap; names pass-through amounts so a hardware wallet's inflated
  "send" reads as explained, not surprising.
- **`trait_delta`** → `TraitItem` — Gains (+green) / losses (-red)
  trait chips for trade swaps.
- **`printing_timeline`** → `PrintingTimelineConfig` — Horizontal
  timeline of card/asset printings (MtG / TCG context).

## Infrastructure (not widgets)

- **`buttons`** → `UiButtonExt` — extension trait adding pointer-cursor
  hover. Use on any `egui::Button`.
- **`icons`** → `PhosphorIcon` + `install_phosphor_font` — phosphor
  glyph helper. Call `install_phosphor_font(ctx)` once, then
  `PhosphorIcon::Gear.rich_text(size, colour)`.
- **`theme`** — shared dark-mode colour palette.
- **`utils`** — `truncate_middle`, etc. *(Consider `IdPill` before
  hand-truncating.)*
- **`screenshot`**, **`image_loader`** — runtime infrastructure.

---

## Maintenance

When adding a new widget:

1. Add it to the lib.rs `pub mod` block + `pub use` re-export.
2. Add a Storybook story under
   `shared-crates/ui/_storybook-egui/src/stories/` with the same name +
   register it in `lib.rs::Story` enum (label, category, description,
   dispatch arm).
3. **Add a one-line entry to this file** in the bucket that best fits.

If an existing widget is being *generalised* (e.g. `Chip` came from the
old `status_chip` helpers), note the old call sites in the doc-comment
so future readers find the bridge.

If you find yourself building something inline that looks like it
should be a widget — check this file first, then either reach for what
exists or extract a new one with a story.
