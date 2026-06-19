#[cfg(target_arch = "wasm32")]
mod stories;

#[cfg(target_arch = "wasm32")]
pub use app::*;

#[cfg(target_arch = "wasm32")]
mod app {
    use eframe::wasm_bindgen::JsCast as _;
    use wasm_bindgen::prelude::*;

    use super::stories;

    // ========================================================================
    // Story Registry
    // ========================================================================

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Story {
        Formatting,
        Distribution,
        Marquee,
        Buttons,
        ProgressBar,
        BulletBar,
        Sparkline,
        MetricCard,
        SevenSegment,
        FlipCounter,
        AsyncData,
        MeshPlayground,
        PerspectiveText,
        TcgCard,
        PrintingTimeline,
        AssetCard,
        RadarChart,
        RangeBar,
        PipRow,
        CardBrowser,
        IconGallery,
        WalletButton,
        TraitFilter,
        WalletEditor,
        SwapModal,
        TraitDelta,
        CoverageDeltaBar,
        AssetStrip,
        TradeTable,
        SigningStatus,
        FeeReport,
        TxEstimate,
        WalletAssetPicker,
        UtxoMap,
        ManagedWalletUtxos,
        DistributionWaterfall,
        // DEX split swap
        SlippageSelector,
        AmountInput,
        SplitAllocationBar,
        RouteSummary,
        PoolLiquidity,
        PriceImpactCurve,
        // Loan dashboard
        ExposureBar,
        DataTable,
        // Mint dashboard
        SupplyBar,
        OrderList,
        // Utility
        FileUpload,
        // Media
        ImageTextEditor,
        // TX Cart
        TxCart,
        // Grouping
        GroupedSection,
        OfferTile,
        // Wallet
        WalletIdentityHeader,
        PersonaStrip,
        FungiblesRow,
        // Auth / admin
        MnemonicDisplay,
        WalletList,
        CollectionList,
        // Mint configuration
        Chip,
        TagList,
        PropertyList,
        IdPill,
        PhaseCard,
        ButtonGroup,
        Toast,
        Timestamp,
        ErrorNote,
        QuantityStepper,
        MintCheckout,
    }

    impl Story {
        fn all() -> &'static [Self] {
            &[
                // Primitives — foundational composables. Add new
                // foundation widgets (Chip / IdPill / PropertyList shape)
                // to this group, not at the end of the list.
                Self::Formatting,
                Self::Distribution,
                Self::Marquee,
                Self::Buttons,
                Self::Chip,
                Self::TagList,
                Self::IdPill,
                Self::PropertyList,
                Self::ButtonGroup,
                Self::Toast,
                Self::Timestamp,
                Self::ErrorNote,
                Self::ProgressBar,
                Self::BulletBar,
                Self::Sparkline,
                Self::MetricCard,
                Self::SevenSegment,
                Self::FlipCounter,
                Self::AsyncData,
                Self::MeshPlayground,
                Self::PerspectiveText,
                Self::TcgCard,
                Self::PrintingTimeline,
                Self::AssetCard,
                Self::RadarChart,
                Self::RangeBar,
                Self::PipRow,
                Self::CardBrowser,
                Self::IconGallery,
                Self::WalletButton,
                Self::WalletEditor,
                Self::TraitFilter,
                Self::SwapModal,
                Self::TraitDelta,
                Self::CoverageDeltaBar,
                Self::AssetStrip,
                Self::TradeTable,
                Self::SigningStatus,
                Self::FeeReport,
                Self::TxEstimate,
                Self::WalletAssetPicker,
                Self::UtxoMap,
                Self::ManagedWalletUtxos,
                Self::DistributionWaterfall,
                // DEX split swap
                Self::SlippageSelector,
                Self::AmountInput,
                Self::SplitAllocationBar,
                Self::RouteSummary,
                Self::PoolLiquidity,
                Self::PriceImpactCurve,
                // Loan dashboard
                Self::ExposureBar,
                Self::DataTable,
                // Mint dashboard
                Self::SupplyBar,
                Self::OrderList,
                // Utility
                Self::FileUpload,
                // Media
                Self::ImageTextEditor,
                // TX Cart
                Self::TxCart,
                // Grouping
                Self::GroupedSection,
                Self::OfferTile,
                // Wallet
                Self::WalletIdentityHeader,
                Self::PersonaStrip,
                Self::FungiblesRow,
                Self::MnemonicDisplay,
                Self::WalletList,
                Self::CollectionList,
                // Mint configuration
                Self::PhaseCard,
                Self::QuantityStepper,
                Self::MintCheckout,
            ]
        }

        fn label(&self) -> &'static str {
            match self {
                Self::Formatting => "Formatting",
                Self::Distribution => "Distribution",
                Self::Marquee => "Marquee",
                Self::Buttons => "Buttons",
                Self::ProgressBar => "Progress Bar",
                Self::BulletBar => "Bullet Bar",
                Self::Sparkline => "Sparkline",
                Self::MetricCard => "Metric Card",
                Self::SevenSegment => "Seven Segment",
                Self::FlipCounter => "Flip Counter",
                Self::AsyncData => "Async Data",
                Self::MeshPlayground => "Mesh Playground",
                Self::PerspectiveText => "Perspective Text",
                Self::TcgCard => "TCG Card",
                Self::PrintingTimeline => "Printing Timeline",
                Self::AssetCard => "Asset Card",
                Self::RadarChart => "Radar Chart",
                Self::RangeBar => "Range Bar",
                Self::PipRow => "Pip Row",
                Self::CardBrowser => "Card Browser",
                Self::IconGallery => "Icon Gallery",
                Self::WalletButton => "Wallet Button",
                Self::TraitFilter => "Trait Filter",
                Self::WalletEditor => "Wallet Editor",
                Self::SwapModal => "Swap Modal",
                Self::TraitDelta => "Trait Delta",
                Self::CoverageDeltaBar => "Coverage Delta Bar",
                Self::AssetStrip => "Asset Strip",
                Self::TradeTable => "Trade Table",
                Self::SigningStatus => "Signing Status",
                Self::FeeReport => "Fee Report",
                Self::TxEstimate => "TX Estimate",
                Self::WalletAssetPicker => "Wallet Asset Picker",
                Self::UtxoMap => "UTxO Shelf",
                Self::ManagedWalletUtxos => "Managed Wallet UTxOs",
                Self::DistributionWaterfall => "Distribution Waterfall",
                Self::SlippageSelector => "Slippage Selector",
                Self::AmountInput => "Amount Input",
                Self::SplitAllocationBar => "Split Allocation Bar",
                Self::RouteSummary => "Route Summary",
                Self::PoolLiquidity => "Pool Liquidity",
                Self::PriceImpactCurve => "Price Impact Curve",
                Self::ExposureBar => "Exposure Bar",
                Self::DataTable => "Data Table",
                Self::SupplyBar => "Supply Bar",
                Self::OrderList => "Order List",
                Self::FileUpload => "File Upload",
                Self::ImageTextEditor => "Image Text Editor",
                Self::TxCart => "TX Cart",
                Self::GroupedSection => "Grouped Section",
                Self::OfferTile => "Offer Tile",
                Self::WalletIdentityHeader => "Wallet Identity Header",
                Self::PersonaStrip => "Persona Strip",
                Self::FungiblesRow => "Fungibles Row",
                Self::MnemonicDisplay => "Mnemonic Display",
                Self::WalletList => "Wallet List",
                Self::CollectionList => "Collection List",
                Self::Chip => "Chip",
                Self::TagList => "Tag List",
                Self::PropertyList => "Property List",
                Self::IdPill => "ID Pill",
                Self::Timestamp => "Timestamp",
                Self::ErrorNote => "Error Note",
                Self::PhaseCard => "Phase Card",
                Self::ButtonGroup => "Button Group",
                Self::Toast => "Toast",
                Self::QuantityStepper => "Quantity Stepper",
                Self::MintCheckout => "Mint Checkout",
            }
        }

        fn category(&self) -> &'static str {
            match self {
                Self::Formatting
                | Self::Distribution
                | Self::Marquee
                | Self::Buttons
                | Self::Chip
                | Self::TagList
                | Self::Timestamp
                | Self::ErrorNote
                | Self::IdPill
                | Self::PropertyList
                | Self::ButtonGroup
                | Self::Toast => "Primitives",
                Self::ProgressBar
                | Self::BulletBar
                | Self::Sparkline
                | Self::MetricCard
                | Self::SevenSegment
                | Self::FlipCounter
                | Self::AsyncData
                | Self::MeshPlayground
                | Self::PerspectiveText
                | Self::TcgCard
                | Self::PrintingTimeline
                | Self::AssetCard
                | Self::RadarChart
                | Self::RangeBar
                | Self::PipRow
                | Self::CardBrowser
                | Self::IconGallery
                | Self::TraitFilter => "Data Visualization",
                Self::WalletButton | Self::WalletEditor => "Wallet",
                Self::SwapModal => "Swap",
                Self::TraitDelta
                | Self::CoverageDeltaBar
                | Self::AssetStrip
                | Self::TradeTable
                | Self::SigningStatus
                | Self::FeeReport
                | Self::TxEstimate
                | Self::WalletAssetPicker => "Trade Desk",
                Self::UtxoMap
                | Self::ManagedWalletUtxos
                | Self::DistributionWaterfall
                | Self::WalletIdentityHeader
                | Self::PersonaStrip
                | Self::FungiblesRow => "Wallet",
                Self::SlippageSelector
                | Self::AmountInput
                | Self::SplitAllocationBar
                | Self::RouteSummary
                | Self::PoolLiquidity
                | Self::PriceImpactCurve => "DEX Split Swap",
                Self::ExposureBar | Self::DataTable => "Loan Dashboard",
                Self::SupplyBar | Self::OrderList => "Mint Dashboard",
                Self::FileUpload => "Utility",
                Self::ImageTextEditor => "Media",
                Self::TxCart => "TX Cart",
                Self::GroupedSection | Self::OfferTile => "Layout",
                Self::MnemonicDisplay | Self::WalletList | Self::CollectionList => "Auth / Admin",
                Self::PhaseCard | Self::QuantityStepper | Self::MintCheckout => {
                    "Mint Configuration"
                }
            }
        }

        fn description(&self) -> &'static str {
            match self {
                Self::Formatting => "Shared formatters: ADA, lovelace, percent, number, duration, hex truncation",
                Self::Timestamp => "Consistent ISO-8601 timestamp atom — fixed monospace size, optional badge, full + relative on hover",
                Self::ErrorNote => "Distils Debug-wrapped / escaped-JSON error blobs to the human reason + HTTP status, with a show-raw toggle",
                Self::Distribution => "Concentric orbital rings supply distribution chart",
                Self::Marquee => "Scrolling ticker with delta-time animation and static centering",
                Self::Buttons => "UiButtonExt trait \u{2014} pointer cursor on hover for buttons",
                Self::ProgressBar => "Determinate and countdown progress bars with custom colors",
                Self::BulletBar => "Value fill with a target marker (bullet graph) — actual vs target",
                Self::Sparkline => {
                    "Inline line chart with fill gradient, mean line, and hover inspection"
                }
                Self::MetricCard => {
                    "Dashboard stat card with trend indicators and embedded sparklines"
                }
                Self::SevenSegment => "Retro LED-style 7-segment display with animated counter",
                Self::FlipCounter => "Split-flap airport board style counter with flip animations",
                Self::AsyncData => "egui_inbox driving widgets from simulated API polling",
                Self::MeshPlayground => {
                    "Raw Mesh API: quads, gradients, trapezoids, rotation, strips"
                }
                Self::PerspectiveText => {
                    "Galley mesh vertex transforms: scale, wave, perspective flip"
                }
                Self::TcgCard => {
                    "Trading card rendering with perspective tilt, holographic effects, and card flip"
                }
                Self::PrintingTimeline => {
                    "Horizontal timeline showing a card's printing history across sets with rarity evolution"
                }
                Self::AssetCard => {
                    "Asset card widget: square, hex, rounded square — with holographic foil, stats, and 3D tilt"
                }
                Self::RadarChart => {
                    "Spider/radar chart for N-dimensional normalized data with bezier curves"
                }
                Self::RangeBar => {
                    "Horizontal range bar with labeled tick marks, gradient fill, and auto-staggered labels"
                }
                Self::PipRow => {
                    "Label + horizontal pip bar for distributions, market depth, and ranked data"
                }
                Self::CardBrowser => {
                    "Master-detail card grid with selection, detail panel, and caller-driven rendering"
                }
                Self::IconGallery => {
                    "Phosphor icon font gallery with size/color controls and contextual examples"
                }
                Self::TraitFilter => {
                    "Compound-key prefix trie tag filter with dual category/value indexing"
                }
                Self::WalletButton => "CIP-30 wallet connection button with state management",
                Self::WalletEditor => {
                    "Wallet bundle editor with input, status indicators, and add/remove actions"
                }
                Self::SwapModal => "DEX swap modal with preview, culture buys, and progress states",
                Self::TraitDelta => {
                    "Trait gain/loss chips showing which traits change hands in a trade"
                }
                Self::CoverageDeltaBar => {
                    "Before/after coverage bar with delta indicator for trade impact"
                }
                Self::TradeTable => {
                    "Two-column trade offer layout with asset cards, add/remove controls"
                }
                Self::SigningStatus => {
                    "Concurrent signing checklist with Sign/Cancel actions and progress states"
                }
                Self::FeeReport => {
                    "Per-side fee breakdown with Black Flag holder waiver display"
                }
                Self::TxEstimate => {
                    "Per-wallet transaction estimate with platform fee, network fee, min UTxO, and net ADA"
                }
                Self::WalletAssetPicker => {
                    "Modal asset browser with accordion policy groups and card grid selection"
                }
                Self::AssetStrip => {
                    "Horizontally stacked asset thumbnails with progressive overlap and click-to-remove"
                }
                Self::UtxoMap => {
                    "UTxO health shelving unit: classify UTxOs into Collateral, Liquid, Clean, Cluttered, Bloated, Dust tiers"
                }
                Self::ManagedWalletUtxos => {
                    "Role-aware UTxO breakdown for a custodial wallet: spendable ADA vs flagged asset-bearing (minted-to-self / stray) UTxOs"
                }
                Self::DistributionWaterfall => {
                    "How a buyer's payment flows to each party (gross → fees → distributable → split), across Projected / Live / Final modes"
                }
                Self::SlippageSelector => {
                    "Preset slippage buttons + custom input mode with high/low warnings"
                }
                Self::AmountInput => {
                    "ADA amount input with preset buttons, optional MAX, and validation warnings"
                }
                Self::SplitAllocationBar => {
                    "Segmented horizontal bar showing ADA allocation across DEXes with tooltips and legend"
                }
                Self::RouteSummary => {
                    "Split routing result: per-leg breakdown, totals, blended price, and improvement vs single pool"
                }
                Self::PoolLiquidity => {
                    "Per-pool depth bars, TVL, spot price, price impact (green/yellow/red), and allocation fraction"
                }
                Self::PriceImpactCurve => {
                    "AMM price impact curves per pool — visualizes why split routing minimizes slippage"
                }
                Self::ExposureBar => {
                    "Stacked horizontal bar showing total ADA exposure by collateral token, colored by LTV risk"
                }
                Self::DataTable => {
                    "Dense row-based table with column headers, LTV micro-bars, selection, and detail panel"
                }
                Self::SupplyBar => {
                    "Two-band mint supply bar: minted (fulfilled) + ordered backlog, with oversubscription handling"
                }
                Self::OrderList => {
                    "Mint-orders dashboard — per-status filter chips, search, relative dates (absolute on hover), \
                     quiet refund chips, and an expandable per-order event history"
                }
                Self::FileUpload => {
                    "Browser file picker button — reads selected files into memory with name, MIME type, and bytes"
                }
                Self::ImageTextEditor => {
                    "Drag-to-position text overlays on images with font size, color, and outline controls. Flattens to final composite."
                }
                Self::TxCart => {
                    "Batched transaction cart with per-item status, phase state machine, and sequential signing flow"
                }
                Self::GroupedSection => {
                    "Group header (hero icon + title + verified badge + bulk-action button) with caller-rendered body"
                }
                Self::OfferTile => {
                    "Picker tile with state machine (Active / InCart / Spent), image-or-placeholder content, and corner badge"
                }
                Self::WalletIdentityHeader => {
                    "Big handle or shortened stake address with copy button — top-of-page wallet identity strip"
                }
                Self::PersonaStrip => {
                    "Italic one-liner persona summary with optional tag chips — wallet/collection persona view"
                }
                Self::FungiblesRow => {
                    "Compact row for a fungible token holding (name, ticker chip, quantity, optional ADA value)"
                }
                Self::MnemonicDisplay => {
                    "BIP-39 mnemonic shown once during provisioning / Art. 20 export — numbered grid, copy CTA, confirmation gate"
                }
                Self::WalletList => {
                    "Per-client wallet roster — Primary at top, Collections grouped, Custom folded below, with archive actions"
                }
                Self::CollectionList => {
                    "Per-client collections list — title, status/standard/network chips, supply progress, policy_id copy, Test mint / Seed stubs actions"
                }
                Self::Chip => {
                    "Small filled-tag label with semantic variants (Success / Warning / Danger / Tag / Info / Muted) + optional × remove affordance"
                }
                Self::TagList => {
                    "Wrapping row of removable chips with an optional clear-all button — for active filters / selected facets"
                }
                Self::PropertyList => {
                    "Compact label/value grid for read-only key data — phase summaries, wallet readouts, payment audit"
                }
                Self::IdPill => {
                    "Truncated identifier with copy button — policy_id, wallet addresses, deposit addresses, tx hashes"
                }
                Self::PhaseCard => {
                    "Read-only mint phase card — header (name + status + priority + Edit/Delete), Price/Window/Per-wallet properties, gate chips with × remove + Add gate"
                }
                Self::ButtonGroup => {
                    "Row of related action buttons — text + optional Phosphor icons + tooltips + disabled state, with horizontal_wrapped layout"
                }
                Self::Toast => {
                    "Transient overlay messages with frame-countdown auto-dismiss — Success/Error/Warning/Info, host-owned ToastQueue, bottom-right stack"
                }
                Self::QuantityStepper => {
                    "Compact −/[n]/+ quantity control with min/max clamping — caller owns the value, returns clamped value + changed flag, − disables at min and + at max"
                }
                Self::MintCheckout => {
                    "Buyer mint offer + CTA — phase/eligibility chips, QuantityStepper, price-each/total, fixed-price bundle cards, purchase summary, Mint button, working/submitted/error states; VM-driven, returns QtyChanged/Mint/SelectBundle"
                }
            }
        }
    }

    // ========================================================================
    // Theme
    // ========================================================================

    const BG_SIDEBAR: egui::Color32 = egui::Color32::from_rgb(20, 20, 40);
    pub const BG_MAIN: egui::Color32 = egui::Color32::from_rgb(26, 26, 46);
    pub const TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(100, 100, 130);
    const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(220, 220, 235);
    pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(68, 255, 68);
    const BG_SELECTED: egui::Color32 = egui::Color32::from_rgb(40, 40, 60);

    fn configure_style(ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        style.visuals.dark_mode = true;
        style.visuals.panel_fill = BG_MAIN;
        style.visuals.window_fill = BG_MAIN;
        style.visuals.override_text_color = Some(TEXT_PRIMARY);
        ctx.set_style(style);
    }

    // ========================================================================
    // App
    // ========================================================================

    struct StorybookApp {
        current_story: Story,
        // Per-story state
        distribution_chart: egui_widgets::DistributionChart,
        marquee: egui_widgets::Marquee,
        marquee_messages: Vec<egui_widgets::MarqueeItem>,
        progress_bar_state: stories::progress_bar::ProgressBarState,
        bullet_bar_state: stories::bullet_bar::BulletBarState,
        tag_list_state: stories::tag_list::TagListState,
        sparkline_state: stories::sparkline::SparklineState,
        seven_segment_state: stories::seven_segment::SevenSegmentState,
        flip_counter_state: stories::flip_counter::FlipCounterState,
        async_data_state: stories::async_data::AsyncDataState,
        mesh_playground_state: stories::mesh_playground::MeshPlaygroundState,
        perspective_text_state: stories::perspective_text::PerspectiveTextState,
        tcg_card_state: stories::tcg_card::TcgCardState,
        printing_timeline_state: stories::printing_timeline::PrintingTimelineDemo,
        asset_card_state: stories::asset_card::AssetCardState,
        radar_chart_state: stories::radar_chart::RadarChartState,
        range_bar_state: stories::range_bar::RangeBarState,
        pip_row_state: stories::pip_row::PipRowState,
        card_browser_state: stories::card_browser::CardBrowserStoryState,
        icon_gallery_state: stories::icon_gallery::IconGalleryState,
        trait_filter_state: stories::trait_filter::TraitFilterStoryState,
        wallet_editor_state: stories::wallet_editor::WalletEditorStoryState,
        wallet_btn: egui_widgets::WalletButton,
        wallet_connector: egui_widgets::wallet::WalletConnector,
        swap_modal: egui_widgets::SwapModal,
        swap_progress: egui_widgets::SwapProgress,
        // Trade desk
        asset_strip_state: stories::asset_strip::AssetStripStoryState,
        fee_report_state: stories::fee_report::FeeReportStoryState,
        tx_estimate_state: stories::tx_estimate::TxEstimateStoryState,
        signing_status_state: stories::signing_status::SigningStatusStoryState,
        trade_table_state: stories::trade_table::TradeTableStoryState,
        wallet_asset_picker_state: stories::wallet_asset_picker::WalletAssetPickerStoryState,
        utxo_map_state: stories::utxo_map::UtxoMapStoryState,
        managed_wallet_utxos_state: stories::managed_wallet_utxos::ManagedWalletUtxosStoryState,
        distribution_waterfall_state:
            stories::distribution_waterfall::DistributionWaterfallStoryState,
        // DEX split swap
        slippage_selector_state: stories::slippage_selector::SlippageSelectorStoryState,
        amount_input_state: stories::amount_input::AmountInputStoryState,
        // Loan dashboard
        data_table_state: stories::data_table::DataTableStoryState,
        // Utility
        file_upload_state: stories::file_upload::FileUploadState,
        image_text_editor_state: stories::image_text_editor::ImageTextEditorState,
        // Mint dashboard
        order_list_state: stories::order_list::OrderListState,
        // Mint configuration
        quantity_stepper_state: stories::quantity_stepper::QuantityStepperStoryState,
        mint_checkout_state: stories::mint_checkout::MintCheckoutStoryState,
        // TX Cart
        tx_cart_state: stories::tx_cart::TxCartStoryState,
        // Wallet
        wallet_identity_header_state:
            stories::wallet_identity_header::WalletIdentityHeaderStoryState,
        // Auth / admin
        mnemonic_display_state: stories::mnemonic_display::MnemonicDisplayState,
        wallet_list_state: stories::wallet_list::WalletListState,
        collection_list_state: stories::collection_list::CollectionListState,
        // Primitives
        button_group_state: stories::button_group::ButtonGroupState,
        toast_state: stories::toast::ToastState,
    }

    impl StorybookApp {
        fn new(cc: &eframe::CreationContext<'_>) -> Self {
            configure_style(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            cc.egui_ctx.add_image_loader(std::sync::Arc::new(
                egui_widgets::image_loader::browser::BrowserImageLoader::default(),
            ));
            egui_widgets::install_phosphor_font(&cc.egui_ctx);

            Self {
                current_story: Story::Distribution,
                distribution_chart: egui_widgets::DistributionChart::new(),
                marquee: egui_widgets::Marquee::default(),
                marquee_messages: vec![egui_widgets::MarqueeItem {
                    text: "Welcome to the egui Widgets Storybook".into(),
                    color: ACCENT,
                }],
                progress_bar_state: stories::progress_bar::ProgressBarState::default(),
                bullet_bar_state: stories::bullet_bar::BulletBarState::default(),
                tag_list_state: stories::tag_list::TagListState::default(),
                sparkline_state: stories::sparkline::SparklineState::default(),
                seven_segment_state: stories::seven_segment::SevenSegmentState::default(),
                flip_counter_state: stories::flip_counter::FlipCounterState::default(),
                async_data_state: stories::async_data::AsyncDataState::default(),
                mesh_playground_state: stories::mesh_playground::MeshPlaygroundState::default(),
                perspective_text_state: stories::perspective_text::PerspectiveTextState::default(),
                tcg_card_state: stories::tcg_card::TcgCardState::default(),
                printing_timeline_state: stories::printing_timeline::PrintingTimelineDemo::default(
                ),
                asset_card_state: stories::asset_card::AssetCardState::default(),
                radar_chart_state: stories::radar_chart::RadarChartState::default(),
                range_bar_state: stories::range_bar::RangeBarState::default(),
                pip_row_state: stories::pip_row::PipRowState::default(),
                card_browser_state: stories::card_browser::CardBrowserStoryState::default(),
                icon_gallery_state: stories::icon_gallery::IconGalleryState::default(),
                trait_filter_state: stories::trait_filter::TraitFilterStoryState::default(),
                wallet_editor_state: stories::wallet_editor::WalletEditorStoryState::default(),
                wallet_btn: egui_widgets::WalletButton::new(),
                wallet_connector: egui_widgets::wallet::WalletConnector::new(),
                swap_modal: egui_widgets::SwapModal::new(egui_widgets::SwapModalConfig {
                    token_name: "TestToken".into(),
                    token_ticker: Some("TST".into()),
                    culture_buys: vec![
                        egui_widgets::CultureBuy {
                            ada_amount: 51,
                            label: "Area 51".into(),
                        },
                        egui_widgets::CultureBuy {
                            ada_amount: 69,
                            label: "Nice".into(),
                        },
                        egui_widgets::CultureBuy {
                            ada_amount: 420,
                            label: "Blaze".into(),
                        },
                    ],
                    theme: egui_widgets::SwapModalTheme::default(),
                }),
                swap_progress: egui_widgets::SwapProgress::Idle,
                asset_strip_state: stories::asset_strip::AssetStripStoryState::default(),
                fee_report_state: stories::fee_report::FeeReportStoryState::default(),
                tx_estimate_state: stories::tx_estimate::TxEstimateStoryState::default(),
                signing_status_state: stories::signing_status::SigningStatusStoryState::default(),
                trade_table_state: stories::trade_table::TradeTableStoryState::default(),
                wallet_asset_picker_state:
                    stories::wallet_asset_picker::WalletAssetPickerStoryState::default(),
                utxo_map_state: stories::utxo_map::UtxoMapStoryState::default(),
                managed_wallet_utxos_state:
                    stories::managed_wallet_utxos::ManagedWalletUtxosStoryState::default(),
                distribution_waterfall_state:
                    stories::distribution_waterfall::DistributionWaterfallStoryState::default(),
                slippage_selector_state:
                    stories::slippage_selector::SlippageSelectorStoryState::default(),
                amount_input_state: stories::amount_input::AmountInputStoryState::default(),
                data_table_state: stories::data_table::DataTableStoryState::default(),
                file_upload_state: stories::file_upload::FileUploadState::default(),
                order_list_state: stories::order_list::OrderListState::default(),
                quantity_stepper_state:
                    stories::quantity_stepper::QuantityStepperStoryState::default(),
                mint_checkout_state: stories::mint_checkout::MintCheckoutStoryState::default(),
                image_text_editor_state: stories::image_text_editor::ImageTextEditorState::default(
                ),
                tx_cart_state: stories::tx_cart::TxCartStoryState::default(),
                wallet_identity_header_state:
                    stories::wallet_identity_header::WalletIdentityHeaderStoryState::default(),
                mnemonic_display_state: stories::mnemonic_display::MnemonicDisplayState::default(),
                wallet_list_state: stories::wallet_list::WalletListState::default(),
                collection_list_state: stories::collection_list::CollectionListState::default(),
                button_group_state: stories::button_group::ButtonGroupState::default(),
                toast_state: stories::toast::ToastState::default(),
            }
        }

        fn draw_sidebar(&mut self, ui: &mut egui::Ui) {
            let mut current_category = "";
            for story in Story::all() {
                if story.category() != current_category {
                    current_category = story.category();
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(current_category)
                            .color(TEXT_MUTED)
                            .small()
                            .strong(),
                    );
                }
                let is_selected = self.current_story == *story;
                let text = if is_selected {
                    egui::RichText::new(story.label()).color(ACCENT).strong()
                } else {
                    egui::RichText::new(story.label()).color(TEXT_PRIMARY)
                };
                let fill = if is_selected {
                    BG_SELECTED
                } else {
                    egui::Color32::TRANSPARENT
                };
                if ui
                    .add(
                        egui::Button::new(text)
                            .fill(fill)
                            .frame(false)
                            .min_size(egui::vec2(ui.available_width(), 24.0)),
                    )
                    .clicked()
                {
                    self.current_story = *story;
                }
            }
        }
    }

    impl eframe::App for StorybookApp {
        // eframe 0.34 made `ui` the required App method (was `update` in 0.33);
        // panels nest via `show_inside(ui, …)` instead of `show(ctx, …)`.
        fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            let ctx = ui.ctx().clone();
            egui::SidePanel::left("stories")
                .default_width(180.0)
                .resizable(false)
                .frame(egui::Frame::side_top_panel(&ctx.style()).fill(BG_SIDEBAR))
                .show_inside(ui, |ui| {
                    ui.add_space(8.0);
                    ui.heading(egui::RichText::new("egui Widgets").color(ACCENT));
                    ui.separator();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.draw_sidebar(ui);
                    });
                });

            egui::CentralPanel::default()
                .frame(egui::Frame::central_panel(&ctx.style()).fill(BG_MAIN))
                .show_inside(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.heading(self.current_story.label());
                        ui.label(
                            egui::RichText::new(self.current_story.description()).color(TEXT_MUTED),
                        );
                        ui.separator();
                        ui.add_space(8.0);

                        match self.current_story {
                            Story::Formatting => stories::formatting::show(ui),
                            Story::Timestamp => stories::timestamp::show(ui),
                            Story::ErrorNote => stories::error_note::show(ui),
                            Story::Distribution => {
                                stories::distribution::show(ui, &mut self.distribution_chart)
                            }
                            Story::Marquee => stories::marquee::show(
                                ui,
                                &mut self.marquee,
                                &mut self.marquee_messages,
                            ),
                            Story::Buttons => stories::buttons::show(ui),
                            Story::ProgressBar => {
                                stories::progress_bar::show(ui, &mut self.progress_bar_state)
                            }
                            Story::BulletBar => {
                                stories::bullet_bar::show(ui, &mut self.bullet_bar_state)
                            }
                            Story::Sparkline => {
                                stories::sparkline::show(ui, &mut self.sparkline_state)
                            }
                            Story::MetricCard => stories::metric_card::show(ui),
                            Story::SevenSegment => {
                                stories::seven_segment::show(ui, &mut self.seven_segment_state)
                            }
                            Story::FlipCounter => {
                                stories::flip_counter::show(ui, &mut self.flip_counter_state)
                            }
                            Story::AsyncData => {
                                stories::async_data::show(ui, &mut self.async_data_state)
                            }
                            Story::MeshPlayground => {
                                stories::mesh_playground::show(ui, &mut self.mesh_playground_state)
                            }
                            Story::PerspectiveText => stories::perspective_text::show(
                                ui,
                                &mut self.perspective_text_state,
                            ),
                            Story::TcgCard => stories::tcg_card::show(ui, &mut self.tcg_card_state),
                            Story::PrintingTimeline => stories::printing_timeline::show(
                                ui,
                                &mut self.printing_timeline_state,
                            ),
                            Story::AssetCard => {
                                stories::asset_card::show(ui, &mut self.asset_card_state)
                            }
                            Story::RadarChart => {
                                stories::radar_chart::show(ui, &mut self.radar_chart_state)
                            }
                            Story::RangeBar => {
                                stories::range_bar::show(ui, &mut self.range_bar_state)
                            }
                            Story::PipRow => stories::pip_row::show(ui, &mut self.pip_row_state),
                            Story::CardBrowser => {
                                stories::card_browser::show(ui, &mut self.card_browser_state)
                            }
                            Story::IconGallery => {
                                stories::icon_gallery::show(ui, &mut self.icon_gallery_state)
                            }
                            Story::TraitFilter => {
                                stories::trait_filter::show(ui, &mut self.trait_filter_state)
                            }
                            Story::WalletEditor => {
                                stories::wallet_editor::show(ui, &mut self.wallet_editor_state)
                            }
                            Story::WalletButton => stories::wallet::show(
                                ui,
                                &mut self.wallet_btn,
                                &mut self.wallet_connector,
                            ),
                            Story::SwapModal => stories::swap::show(
                                &ctx,
                                ui,
                                &mut self.swap_modal,
                                &mut self.swap_progress,
                            ),
                            Story::TraitDelta => stories::trait_delta::show(ui),
                            Story::CoverageDeltaBar => stories::coverage_delta_bar::show(ui),
                            Story::TradeTable => {
                                stories::trade_table::show(ui, &mut self.trade_table_state)
                            }
                            Story::SigningStatus => {
                                stories::signing_status::show(ui, &mut self.signing_status_state)
                            }
                            Story::FeeReport => {
                                stories::fee_report::show(ui, &mut self.fee_report_state)
                            }
                            Story::TxEstimate => {
                                stories::tx_estimate::show(ui, &mut self.tx_estimate_state)
                            }
                            Story::WalletAssetPicker => stories::wallet_asset_picker::show(
                                &ctx,
                                ui,
                                &mut self.wallet_asset_picker_state,
                            ),
                            Story::AssetStrip => {
                                stories::asset_strip::show(ui, &mut self.asset_strip_state)
                            }
                            Story::UtxoMap => stories::utxo_map::show(
                                ui,
                                &mut self.utxo_map_state,
                                &mut self.wallet_btn,
                                &mut self.wallet_connector,
                            ),
                            Story::ManagedWalletUtxos => stories::managed_wallet_utxos::show(
                                ui,
                                &mut self.managed_wallet_utxos_state,
                            ),
                            Story::DistributionWaterfall => stories::distribution_waterfall::show(
                                ui,
                                &mut self.distribution_waterfall_state,
                            ),
                            // DEX split swap
                            Story::SlippageSelector => stories::slippage_selector::show(
                                ui,
                                &mut self.slippage_selector_state,
                            ),
                            Story::AmountInput => {
                                stories::amount_input::show(ui, &mut self.amount_input_state)
                            }
                            Story::SplitAllocationBar => stories::split_allocation_bar::show(ui),
                            Story::RouteSummary => stories::route_summary::show(ui),
                            Story::PoolLiquidity => stories::pool_liquidity::show(ui),
                            Story::PriceImpactCurve => stories::price_impact_curve::show(ui),
                            // Loan dashboard
                            Story::ExposureBar => stories::exposure_bar::show(ui),
                            Story::SupplyBar => stories::supply_bar::show(ui),
                            Story::OrderList => {
                                stories::order_list::show(ui, &mut self.order_list_state)
                            }
                            Story::DataTable => {
                                stories::data_table::show(ui, &mut self.data_table_state)
                            }
                            Story::FileUpload => {
                                stories::file_upload::show(ui, &mut self.file_upload_state)
                            }
                            Story::ImageTextEditor => stories::image_text_editor::show(
                                ui,
                                &mut self.image_text_editor_state,
                            ),
                            Story::GroupedSection => stories::grouped_section::show(ui),
                            Story::OfferTile => stories::offer_tile::show(ui),
                            Story::TxCart => stories::tx_cart::show(ui, &mut self.tx_cart_state),
                            Story::WalletIdentityHeader => stories::wallet_identity_header::show(
                                ui,
                                &mut self.wallet_identity_header_state,
                            ),
                            Story::PersonaStrip => stories::persona_strip::show(ui),
                            Story::FungiblesRow => stories::fungibles_row::show(ui),
                            Story::MnemonicDisplay => stories::mnemonic_display::show(
                                ui,
                                &mut self.mnemonic_display_state,
                            ),
                            Story::WalletList => {
                                stories::wallet_list::show(ui, &mut self.wallet_list_state)
                            }
                            Story::CollectionList => {
                                stories::collection_list::show(ui, &mut self.collection_list_state)
                            }
                            Story::Chip => stories::chip::show(ui),
                            Story::TagList => {
                                stories::tag_list::show(ui, &mut self.tag_list_state)
                            }
                            Story::PropertyList => stories::property_list::show(ui),
                            Story::IdPill => stories::id_pill::show(ui),
                            Story::PhaseCard => stories::phase_card::show(ui),
                            Story::QuantityStepper => stories::quantity_stepper::show(
                                ui,
                                &mut self.quantity_stepper_state,
                            ),
                            Story::MintCheckout => {
                                stories::mint_checkout::show(ui, &mut self.mint_checkout_state)
                            }
                            Story::ButtonGroup => {
                                stories::button_group::show(ui, &mut self.button_group_state)
                            }
                            Story::Toast => stories::toast::show(ui, &mut self.toast_state),
                        }
                    });
                });
        }
    }

    // ========================================================================
    // Entry Point
    // ========================================================================

    #[wasm_bindgen(start)]
    pub fn main() {
        console_error_panic_hook::set_once();
        eframe::WebLogger::init(log::LevelFilter::Debug).ok();

        let web_options = eframe::WebOptions::default();

        wasm_bindgen_futures::spawn_local(async {
            let document = web_sys::window()
                .expect("no window")
                .document()
                .expect("no document");
            let canvas = document
                .get_element_by_id("egui_canvas")
                .expect("no egui_canvas element")
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .expect("not a canvas");

            eframe::WebRunner::new()
                .start(
                    canvas,
                    web_options,
                    Box::new(|cc| Ok(Box::new(StorybookApp::new(cc)))),
                )
                .await
                .expect("failed to start eframe");
        });
    }
}

