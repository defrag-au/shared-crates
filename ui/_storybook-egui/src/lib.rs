// `#[macro_use]` rather than a `use`: `macro_rules!` macros are textually
// scoped, so the module has to be declared before the `stories!` invocation in
// `mod app` below.
//
// NOT cfg'd to wasm, deliberately. The app itself is wasm-only, which means a
// native `cargo test` cannot reach the real registry at all — so the macro's own
// tests are the only ones that run on the host. Gating the macro too would leave
// it with no coverage anywhere.
#[macro_use]
mod registry;

#[cfg(target_arch = "wasm32")]
mod stories;

#[cfg(target_arch = "wasm32")]
pub use app::*;

#[cfg(target_arch = "wasm32")]
mod app {
    use eframe::wasm_bindgen::JsCast as _;
    use wasm_bindgen::prelude::*;

    use super::stories;
    use egui_widgets::drawer::{Drawer, DrawerSide};
    use egui_widgets::viewport::{Breakpoint, PanelMode};

    // ========================================================================
    // Story Registry
    // ========================================================================

    // The `enum`, the sidebar ordering, the group headings and the render
    // dispatch, from one declaration per story. See `crate::registry` for what
    // was broken before and why this shape.
    //
    // `label()` and `description()` stay hand-written below: they are exhaustive
    // matches, so the compiler already catches an omission, and moving 129 prose
    // strings would risk pairing one with the wrong story for no safety gain.
    stories! {
        enum Story for StorybookApp;

        group "Primitives" {
            Formatting => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::formatting::show(ui);
            Distribution => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::distribution::show(ui, &mut a.distribution_chart);
            Marquee => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::marquee::show(ui, &mut a.marquee, &mut a.marquee_messages);
            Buttons => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::buttons::show(ui);
            ThemeStates => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::theme_states::show(ui);
            BackgroundToasts => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::background::show(ui);
            Skeleton => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::skeleton::show(ui);
            SliderGroup => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::slider_group::show(ui, &mut a.slider_group_state);
            Chip => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::chip::show(ui);
            PartyBadge => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::party_badge::show(ui);
            FlowLedger => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::flow_ledger::show(ui);
            ActivityFeed => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::activity_feed::show(ui);
            TxCard => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::tx_card::show(ui, &mut a.tx_card_state);
            ImageStack => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::image_stack::show(ui, &mut a.image_stack_state);
            ChannelBands => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::channel_bands::show(ui);
            CustodyWalk => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::custody_walk::show(ui);
            ClaimCard => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::claim_card::show(ui, &mut a.claim_card_state);
            CapitalFlow => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::capital_flow::show(ui, &mut a.capital_flow_state);
            CapBand => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::cap_band::show(ui, &mut a.cap_band_state);
            TimeSpine => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::time_spine::show(ui, &mut a.time_spine_state);
            TimeSpineDensity => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::time_spine_density::show(ui, &mut a.time_spine_density_state);
            CoverageLanes => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::coverage_lanes::show(ui, &mut a.coverage_lanes_state);
            FlowMatrix => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::flow_matrix::show(ui, &mut a.flow_matrix_state);
            FlowRing => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::flow_ring::show(ui, &mut a.flow_ring_state);
            FlowStave => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::flow_stave::show(ui, &mut a.flow_stave_state);
            PartyAnnotator => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::party_annotator::show(ui, &mut a.party_annotator_state);
            TagList => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::tag_list::show(ui, &mut a.tag_list_state);
            TokenMultiselect => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::token_multiselect::show(ui, &mut a.token_multiselect_state);
            TypeaheadSearch => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::typeahead_search::show(ui);
            RelationshipEditor => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::relationship_editor::show(ui, &mut a.relationship_editor_state);
            CommandPalette => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::command_palette::show(ui, &mut a.command_palette_state);
            EventWiring => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::event_wiring::show(ui, &mut a.event_wiring_state);
            WiringEditor => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::wiring_editor::show(ui, &mut a.wiring_editor_state);
            ConversationHistory => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::conversation_history::show(ui, &mut a.conversation_history_state);
            AgentConfig => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::agent_config::show(ui, &mut a.agent_config_state);
            Select => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::select::show(ui, &mut a.select_state);
            UiMachine => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::machine::show(ui, &mut a.machine_state);
            NamedGroupList => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::named_group_list::show(ui, &mut a.named_group_list_state);
            RarityTargetEditor => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::rarity_target_editor::show(ui, &mut a.rarity_target_editor_state);
            EffectEditor => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::effect_editor::show(ui, &mut a.effect_editor_state);
            Knob => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::knob::show(ui, &mut a.knob_state);
            SlotTable => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::slot_table::show(ui, &mut a.slot_table_state);
            IdPill => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::id_pill::show(ui);
            PropertyList => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::property_list::show(ui);
            ButtonGroup => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::button_group::show(ui, &mut a.button_group_state);
            PaneNav => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::pane_nav::show(ui, &mut a.pane_nav_state);
            OptionGroup => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::option_group::show(ui, &mut a.option_group_state);
            Toast => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::toast::show(ui, &mut a.toast_state);
            Timestamp => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::timestamp::show(ui);
            ErrorNote => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::error_note::show(ui);
            Gated => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::gated::show(ui);
            AccessGate => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::access_gate::show(ui);
            Viewport => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::viewport::show(ui);
            Drawer => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::drawer::show(ui);
            Disclosure => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::disclosure::show(ui, &mut a.disclosure_state);
            UserBadge => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::user_badge::show(ui);
            TierLadder => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::tier_ladder::show(ui);
            AboutModal => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::about_modal::show(ui);
            ServiceBanner => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::service_banner::show(ui);
        }

        group "Data Visualization" {
            ProgressBar => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::progress_bar::show(ui, &mut a.progress_bar_state);
            BulletBar => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::bullet_bar::show(ui, &mut a.bullet_bar_state);
            Sparkline => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::sparkline::show(ui, &mut a.sparkline_state);
            MetricCard => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::metric_card::show(ui);
            PerfStrip => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::perf_strip::show(ui, &mut a.perf_strip_state);
            TokenHistory => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::token_history::show(ui);
            TokenKinetic => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::token_kinetic::show(ui);
            TokenParticles => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::token_particles::show(ui);
            StatStrip => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::stat_strip::show(ui);
            SevenSegment => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::seven_segment::show(ui, &mut a.seven_segment_state);
            FlipCounter => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::flip_counter::show(ui, &mut a.flip_counter_state);
            AsyncData => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::async_data::show(ui, &mut a.async_data_state);
            MeshPlayground => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::mesh_playground::show(ui, &mut a.mesh_playground_state);
            PerspectiveText => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::perspective_text::show(ui, &mut a.perspective_text_state);
            TcgCard => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::tcg_card::show(ui, &mut a.tcg_card_state);
            PrintingTimeline => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::printing_timeline::show(ui, &mut a.printing_timeline_state);
            AssetCard => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::asset_card::show(ui, &mut a.asset_card_state);
            RadarChart => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::radar_chart::show(ui, &mut a.radar_chart_state);
            RangeBar => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::range_bar::show(ui, &mut a.range_bar_state);
            PipRow => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::pip_row::show(ui, &mut a.pip_row_state);
            PriceTimeline => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::price_timeline::show(ui, &mut a.price_timeline_state);
            Leaderboard => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::leaderboard::show(ui, &mut a.leaderboard_state);
            FocusList => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::focus_list::show(ui, &mut a.focus_list_state);
            CardBrowser => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::card_browser::show(ui, &mut a.card_browser_state);
            IconGallery => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::icon_gallery::show(ui, &mut a.icon_gallery_state);
            TraitFilter => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::trait_filter::show(ui, &mut a.trait_filter_state);
        }

        // Was TWO groups both named "Wallet", separated in the old ordering by the
        // Trade Desk run — so the sidebar rendered the heading twice. Merged, with
        // each former block's order preserved.
        group "Wallet" {
            WalletButton => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::wallet::show(ui, &mut a.wallet_btn, &mut a.wallet_connector);
            WalletEditor => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::wallet_editor::show(ui, &mut a.wallet_editor_state);
            TxFlight => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::tx_flight::show(ui, &mut a.tx_flight_state);
            StakeSession => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::stake_session::show(ui, &mut a.stake_session_state);
            ListingComposer => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::listing_composer::show(ui, &mut a.listing_composer_state);
            UtxoShelf => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::utxo_shelf::show(ui, &mut a.utxo_shelf_state, &mut a.wallet_btn, &mut a.wallet_connector);
            UtxoMap => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::utxo_map::show(ui, &mut a.utxo_map_state);
            ManagedWalletUtxos => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::managed_wallet_utxos::show(ui, &mut a.managed_wallet_utxos_state);
            DistributionWaterfall => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::distribution_waterfall::show(ui, &mut a.distribution_waterfall_state);
            WalletIdentityHeader => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::wallet_identity_header::show(ui, &mut a.wallet_identity_header_state);
            PersonaStrip => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::persona_strip::show(ui);
            FungiblesRow => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::fungibles_row::show(ui);
        }

        group "Swap" {
            // Wants a `Context` as well as the `Ui`; cloned first because
            // `ui.ctx()` would hold a borrow across the call.
            SwapModal => |a: &mut StorybookApp, ui: &mut egui::Ui| {
                let ctx = ui.ctx().clone();
                stories::swap::show(&ctx, ui, &mut a.swap_modal, &mut a.swap_progress)
            };
        }

        group "Trade Desk" {
            TraitDelta => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::trait_delta::show(ui);
            CoverageDeltaBar => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::coverage_delta_bar::show(ui);
            AssetStrip => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::asset_strip::show(ui, &mut a.asset_strip_state);
            TradeTable => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::trade_table::show(ui, &mut a.trade_table_state);
            SigningStatus => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::signing_status::show(ui, &mut a.signing_status_state);
            FeeReport => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::fee_report::show(ui, &mut a.fee_report_state);
            TxEstimate => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::tx_estimate::show(ui, &mut a.tx_estimate_state);
            TradeFlow => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::trade_flow::show(ui, &mut a.trade_flow_state);
            WalletAssetPicker => |a: &mut StorybookApp, ui: &mut egui::Ui| {
                let ctx = ui.ctx().clone();
                stories::wallet_asset_picker::show(&ctx, ui, &mut a.wallet_asset_picker_state)
            };
        }

        group "DEX Split Swap" {
            SlippageSelector => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::slippage_selector::show(ui, &mut a.slippage_selector_state);
            AmountInput => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::amount_input::show(ui, &mut a.amount_input_state);
            SplitAllocationBar => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::split_allocation_bar::show(ui);
            RouteSummary => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::route_summary::show(ui);
            PoolLiquidity => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::pool_liquidity::show(ui);
            PriceImpactCurve => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::price_impact_curve::show(ui);
        }

        group "Composed Routes" {
            RouteQuote => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::route_quote::show(ui);
            PoolInspector => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::pool_inspector::show(ui);
            TxWatch => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::tx_watch::show(ui);
        }

        group "Collection CSP" {
            VariantSplit => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::variant_split::show(ui);
            CollectionComposition => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::collection_composition::show(ui);
        }

        group "Loan Dashboard" {
            ExposureBar => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::exposure_bar::show(ui);
            DataTable => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::data_table::show(ui, &mut a.data_table_state);
        }

        group "Ranked Lists" {
            LeaderboardTable => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::leaderboard_table::show(ui);
        }

        group "Mint Dashboard" {
            SupplyBar => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::supply_bar::show(ui);
            OrderList => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::order_list::show(ui, &mut a.order_list_state);
        }

        group "Utility" {
            FileUpload => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::file_upload::show(ui, &mut a.file_upload_state);
        }

        group "Media" {
            ImageTextEditor => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::image_text_editor::show(ui, &mut a.image_text_editor_state);
        }

        // `ListingGrid` reported "TX Cart" while sitting in the Data-Visualization
        // run of the old ordering, so it appeared under the wrong heading.
        group "TX Cart" {
            ListingGrid => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::listing_grid::show(ui, &mut a.listing_grid_state);
            TxCart => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::tx_cart::show(ui, &mut a.tx_cart_state);
        }

        group "Layout" {
            GroupedSection => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::grouped_section::show(ui);
            OfferTile => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::offer_tile::show(ui);
            CornerAction => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::corner_action::show(ui);
        }

        group "Auth / Admin" {
            MnemonicDisplay => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::mnemonic_display::show(ui, &mut a.mnemonic_display_state);
            WalletList => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::wallet_list::show(ui, &mut a.wallet_list_state);
            CollectionList => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::collection_list::show(ui, &mut a.collection_list_state);
        }

        group "Mint Configuration" {
            PhaseCard => |_a: &mut StorybookApp, ui: &mut egui::Ui| stories::phase_card::show(ui);
            QuantityStepper => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::quantity_stepper::show(ui, &mut a.quantity_stepper_state);
            MintCheckout => |a: &mut StorybookApp, ui: &mut egui::Ui| stories::mint_checkout::show(ui, &mut a.mint_checkout_state);
        }
    }

    impl Story {
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
                Self::PerfStrip => "Perf Strip",
                Self::TokenHistory => "Token History",
                Self::TokenKinetic => "Token Kinetic",
                Self::TokenParticles => "Token Particles",
                Self::StatStrip => "Stat Strip",
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
                Self::PriceTimeline => "Price Timeline",
                Self::Leaderboard => "Leaderboard",
                Self::ListingGrid => "Listing Grid",
                Self::FocusList => "Focus List",
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
                Self::TxFlight => "Tx Flight",
                Self::StakeSession => "Stake Session",
                Self::ListingComposer => "Listing Composer",
                Self::FeeReport => "Fee Report",
                Self::TxEstimate => "TX Estimate",
                Self::TradeFlow => "Trade Flow",
                Self::WalletAssetPicker => "Wallet Asset Picker",
                Self::UtxoShelf => "UTxO Shelf",
                Self::UtxoMap => "UTxO Map",
                Self::ManagedWalletUtxos => "Managed Wallet UTxOs",
                Self::DistributionWaterfall => "Distribution Waterfall",
                Self::SlippageSelector => "Slippage Selector",
                Self::AmountInput => "Amount Input",
                Self::SplitAllocationBar => "Split Allocation Bar",
                Self::RouteSummary => "Route Summary",
                Self::PoolLiquidity => "Pool Liquidity",
                Self::PriceImpactCurve => "Price Impact Curve",
                Self::RouteQuote => "Route Quote",
                Self::PoolInspector => "Pool Inspector",
                Self::TxWatch => "Tx Watch",
                Self::VariantSplit => "Variant Split",
                Self::CollectionComposition => "Collection Composition",
                Self::ExposureBar => "Exposure Bar",
                Self::DataTable => "Data Table",
                Self::LeaderboardTable => "Leaderboard Table",
                Self::SupplyBar => "Supply Bar",
                Self::OrderList => "Order List",
                Self::FileUpload => "File Upload",
                Self::ImageTextEditor => "Image Text Editor",
                Self::TxCart => "TX Cart",
                Self::GroupedSection => "Grouped Section",
                Self::OfferTile => "Offer Tile",
                Self::CornerAction => "Corner Action",
                Self::WalletIdentityHeader => "Wallet Identity Header",
                Self::PersonaStrip => "Persona Strip",
                Self::FungiblesRow => "Fungibles Row",
                Self::MnemonicDisplay => "Mnemonic Display",
                Self::WalletList => "Wallet List",
                Self::CollectionList => "Collection List",
                Self::ThemeStates => "Theme States",
                Self::BackgroundToasts => "Background Toasts",
                Self::Skeleton => "Skeleton",
                Self::SliderGroup => "Slider Group",
                Self::Chip => "Chip",
                Self::PartyBadge => "Party Badge",
                Self::FlowLedger => "Flow Ledger",
                Self::ActivityFeed => "Activity Feed",
                Self::TxCard => "Tx Card",
                Self::ImageStack => "Image Stack",
                Self::ChannelBands => "Channel Bands",
                Self::CustodyWalk => "Custody Walk",
                Self::ClaimCard => "Claim Card",
                Self::CapitalFlow => "Capital Flow",
                Self::CapBand => "Cap Band",
                Self::TimeSpine => "Time Spine",
                Self::TimeSpineDensity => "Time Spine Density",
                Self::CoverageLanes => "Coverage Lanes",
                Self::FlowMatrix => "Flow Matrix",
                Self::FlowRing => "Flow Ring",
                Self::FlowStave => "Flow Stave",
                Self::PartyAnnotator => "Party Annotator",
                Self::TagList => "Tag List",
                Self::TokenMultiselect => "Token Multiselect",
                Self::TypeaheadSearch => "Typeahead Search",
                Self::RelationshipEditor => "Relationship Editor",
                Self::CommandPalette => "Command Palette",
                Self::EventWiring => "Event Wiring",
                Self::WiringEditor => "Wiring Editor",
                Self::ConversationHistory => "Conversation History",
                Self::AgentConfig => "Agent Config",
                Self::Select => "Select",
                Self::UiMachine => "Machine",
                Self::NamedGroupList => "Named Group List",
                Self::RarityTargetEditor => "Rarity Target Editor",
                Self::EffectEditor => "Effect Editor",
                Self::Knob => "Knob (prototype)",
                Self::SlotTable => "Slot Table",
                Self::PropertyList => "Property List",
                Self::IdPill => "ID Pill",
                Self::Timestamp => "Timestamp",
                Self::ErrorNote => "Error Note",
                Self::Gated => "Gated",
                Self::AccessGate => "Access Gate",
                Self::Viewport => "Breakpoint",
                Self::Drawer => "Drawer",
                Self::Disclosure => "Disclosure",
                Self::UserBadge => "User Badge",
                Self::TierLadder => "Tier Ladder",
                Self::AboutModal => "About Modal",
                Self::ServiceBanner => "Service Banner",
                Self::PhaseCard => "Phase Card",
                Self::ButtonGroup => "Button Group",
                Self::PaneNav => "Pane Nav",
                Self::OptionGroup => "Option Group",
                Self::Toast => "Toast",
                Self::QuantityStepper => "Quantity Stepper",
                Self::MintCheckout => "Mint Checkout",
            }
        }

        fn description(&self) -> &'static str {
            match self {
                Self::Formatting => {
                    "Shared formatters: ADA, lovelace, percent, number, duration, hex truncation"
                }
                Self::Timestamp => {
                    "Consistent ISO-8601 timestamp atom — fixed monospace size, optional badge, full + relative on hover"
                }
                Self::ErrorNote => {
                    "Distils Debug-wrapped / escaped-JSON error blobs to the human reason + HTTP status, with a show-raw toggle"
                }
                Self::Gated => {
                    "Entitlement-gated rendering — locked card/chip affordances driven by the shared authorizations Feature registry"
                }
                Self::AccessGate => {
                    "App-level access screen: sign-in prompt + requirements (join links) for gated tools"
                }
                Self::Viewport => {
                    "Compact / Medium / Wide — the breakpoint every responsive layout decision reads from"
                }
                Self::Drawer => {
                    "Edge-anchored slide-over with a scrim — the narrow-layout stand-in for a side panel"
                }
                Self::Disclosure => {
                    "Detail that opens under the row it explains — eased, tied by a rule, anchored so the list does not shove"
                }
                Self::UserBadge => "Logged-in-as pill (avatar + name) with a sign-out popup",
                Self::TierLadder => {
                    "The access ladder as a modal — what each rung gives, every route to it, and where you stand"
                }
                Self::AboutModal => {
                    "What a product is, what state it is in, and what to expect — the BETA badge's modal"
                }
                Self::ServiceBanner => {
                    "A persistent strip saying the backend is not whole — takes space rather than covering content"
                }
                Self::Distribution => "Concentric orbital rings supply distribution chart",
                Self::Marquee => "Scrolling ticker with delta-time animation and static centering",
                Self::Buttons => "UiButtonExt trait \u{2014} pointer cursor on hover for buttons",
                Self::ProgressBar => "Determinate and countdown progress bars with custom colors",
                Self::BulletBar => {
                    "Value fill with an OPTIONAL target marker (bullet graph) — actual vs \
                     target, or no target at all when none was ever set"
                }
                Self::Sparkline => {
                    "Inline line chart with fill gradient, mean line, and hover inspection"
                }
                Self::MetricCard => {
                    "Dashboard stat card with trend indicators and embedded sparklines"
                }
                Self::PerfStrip => {
                    "Live HUD, vertical or horizontal — frame build cost, fps, memory, work in flight"
                }
                Self::TokenParticles => {
                    "Supply as a conserved particle field, playing through warped time"
                }
                Self::TokenKinetic => {
                    "Kinetic variants — conserved mass, event-warped time, reservoirs"
                }
                Self::TokenHistory => {
                    "Candidate forms for a token's history, on real WRT data — exploration surface"
                }
                Self::StatStrip => {
                    "Row of windowed stat cards — one metric across 24h/7d/30d, with an empty-window note"
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
                Self::PriceTimeline => {
                    "Time-axis price scatter with reference lines/bands, log y, and hover inspection"
                }
                Self::Leaderboard => {
                    "Ranked standings with medals, a share bar, and supporting stats"
                }
                Self::ListingGrid => {
                    "A price is not a promise you can buy it. Real residual jpg.store listings: MarsBirds at 19-25 ADA with resolvable datums, beside a Clay Nation listing whose hash-datum preimage exists nowhere on chain or in any indexer. Before buyability was a state the grid drew those identically — the reader picked the cheapest, clicked, and got a node error about datums seconds later. Blocked cards stay in the book (hide them and the floor you quote is wrong) but are knocked back and say why, in the same corner the eye learned to find the add-to-cart +. The three reasons are an enum, not a bool: 'no datum' is permanent, 'unsupported' is a registry gap we can close, 'bundle' means buyable-but-not-alone — collapsing them to 'unavailable' has you chasing the wrong fix. Note the cheapest card in the grid is one of the blocked ones"
                }
                Self::FocusList => {
                    "Fixed-geometry master-detail list for tooltips: sliding highlight + detail pane"
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
                Self::TxFlight => {
                    "One server-built, wallet-signed transaction as a build / sign / submit checklist"
                }
                Self::ListingComposer => {
                    "Price a batch of listings by what the buyer pays — fee and seller payout per row, from the contract's own arithmetic"
                }
                Self::StakeSession => {
                    "Connect a wallet, sign in to a worker by stake key, stay signed in — the whole strip"
                }
                Self::FeeReport => "Per-side fee breakdown with Black Flag holder waiver display",
                Self::TxEstimate => {
                    "Per-wallet transaction estimate with platform fee, network fee, min UTxO, and net ADA"
                }
                Self::TradeFlow => {
                    "Give / get / net view of a swap; flags UTxO rebalancing so a hardware wallet's inflated 'send' reads as an explained mechanic"
                }
                Self::WalletAssetPicker => {
                    "Modal asset browser with accordion policy groups and card grid selection"
                }
                Self::AssetStrip => {
                    "Horizontally stacked asset thumbnails with progressive overlap and click-to-remove"
                }
                Self::UtxoShelf => {
                    "UTxO health shelving unit: classify UTxOs into Collateral, Liquid, Clean, Cluttered, Bloated, Dust tiers"
                }
                Self::UtxoMap => {
                    "Voronoi terrain map of a wallet: one cell per (utxo, policy), land is locked ADA and water is free ADA — territories are the policies"
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
                Self::RouteQuote => {
                    "Sequential multi-venue route: per-leg fees in mixed assets, where the transaction boundaries fall, and what a partial failure leaves you holding"
                }
                Self::PoolInspector => {
                    "A pool's real state — the curve reserve against the accrued fees sitting beside it, and whether the datum's invariant holds"
                }
                Self::TxWatch => {
                    "Several transactions on their way to chain: sign, submit, confirm, per transaction, with the active stage breathing so a wait for a block does not read as a hung screen"
                }
                Self::VariantSplit => {
                    "Derived variant distribution for a variant_flow source — share weighted by downstream asset capacity, with the uniform baseline for contrast"
                }
                Self::CollectionComposition => {
                    "Promotable infographic of how a collection generates: z-ordered layer stack with presence/options/variant badges + variant_flow connectors, under a stats band"
                }
                Self::ExposureBar => {
                    "Stacked horizontal bar showing total ADA exposure by collateral token, colored by LTV risk"
                }
                Self::DataTable => {
                    "Dense row-based table with column headers, LTV micro-bars, selection, and detail panel"
                }
                Self::LeaderboardTable => {
                    "Dense, virtual-scrolled ranked table — rank, identity (accent for handles), semantic badge, pre-formatted value, and share. For holders / leaderboards / top traders"
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
                Self::CornerAction => {
                    "Icon button pinned to a corner of a thumbnail — claims the click so the card beneath doesn't select"
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
                Self::BackgroundToasts => {
                    "Declare which background jobs are running; the toasts follow. Owns the settle delay (quick work finishes silently), the quiet dismissal (finishing is not news) and the repaint scheduling that makes the delay mean something on an idle surface"
                }
                Self::ThemeStates => {
                    "TEMPLATE for contrast bugs — interaction states (selected / hovered / active / disabled) drawn on every surface, plus the translucent selection wash. Resting-state stories cannot show these; mirrored numerically by tests/contrast.rs"
                }
                Self::Skeleton => {
                    "Placeholders for content that is not on screen, and a statement of WHY — Loading pulses because 'wait' is the right instruction, Withheld is static and recedes because waiting produces nothing. The reason is positional so a call site cannot draw one without saying which. Rows or a block; carries no data, so the same shapes appear whether three items are behind the gate or three thousand"
                }
                Self::SliderGroup => {
                    "A bank of labelled faders on one spine — a mixing desk rotated a quarter turn. Replaces stacked `Slider::text()`, whose labels do not line up, whose boxed DragValue outweighs the control it belongs to, and whose rail stays 100px however wide the pane is. Shown beside the version it replaces"
                }
                Self::Chip => {
                    "Small filled-tag label with semantic variants (Success / Warning / Danger / Tag / Info / Muted) + optional × remove affordance"
                }
                Self::PartyBadge => {
                    "A counterparty plus HOW FIRMLY its identity is known — observed / asserted / derived, shape-coded. Basis is a positional arg, so a call site can't render a party without stating it; an unsourced assertion renders as a warning"
                }
                Self::FlowLedger => {
                    "A wallet's movements in time order — net amounts only, running balance, per-row channel colour, round trips muted, and a reconciliation footer that says DOES NOT RECONCILE rather than showing a plausible total"
                }
                Self::ActivityFeed => {
                    "The account view of the same history: day-grouped cards, each naming its venue, its counterparty and THE ASSETS THAT MOVED — because \"+2 items\" hides whether a wallet got two junk airdrops or two of the collection it trades"
                }
                Self::ImageStack => {
                    "The tuning bench for the fanned pile of mounted prints that makes a lot of many READ as a lot of many. Every proportion is a slider — mount, spacing, lift, tilt spread, shadow offset/spread/alpha — and the count runs to the hard cap of five, because the difference between 'prints dropped on a desk' and 'some overlapping squares' is a few percent in two of them, and no amount of reading the code tells you which way to go. The art is a rotated mesh, not an egui::Image: Image::corner_radius silently cancels Image::rotate, which shipped once as upright pictures inside tilted mounts. The shadow is faked: epaint blurs rectangles but not rotated polygons, so fourteen concentric quads on an eased alpha ramp stand in for a blur. Includes a backdrop toggle — the server-rendered card sits on #0b0b10 where paper white pops far harder than it does on the app's own BG_SECONDARY, which may be most of why the rendered version looked stronger"
                }
                Self::TxCard => {
                    "One transaction as a VERDICT rather than a field list. The row this replaces led with the WALLET NET — bookkeeping — and buried the settlement price as the smallest text on the row next to a raw db slug; four chips did the work of one clause. Same ranking as the social card: the price leads, the lot is named by its shared stem (never after one member), the party clause is a sentence, the net goes last and grey. Viewpoint is an enum with the parties INSIDE it, so a policy feed — which has no \"us\" — literally cannot be given a verb of ownership. Two absences are kept apart: below-floor pulses and offers to walk deeper, ambiguous states itself and offers nothing, because deepening cannot fix it"
                }
                Self::CapitalFlow => {
                    "\"They raised X — watch where it went.\" Cumulative destination bands over a real time axis with a draggable playhead and play button; a labelled raise line the stack is free to CROSS, because deployment beyond the raise is a finding rather than an error to clamp"
                }
                Self::CapBand => {
                    "A headline valuation against what it would ACTUALLY fetch. The yellow line is what everybody quotes; the teal is the float sold into the curve — at peak ~150k notional against ~20k realisable, about 13%. A chart showing only the headline makes an argument it cannot support. For the first third the band is a LINE, not a band: low == high because every holder was identified and there was no uncertainty to draw. That flat stretch is also four collinear points per quad — drawn as a polygon the tessellator cannot derive a normal and throws diagonal rays across the panel, which shipped once and was caught only by looking"
                }
                Self::TimeSpine => {
                    "ONE time axis for many faces: a playhead that REVEALS, a brush that FILTERS, play/pause — and a shared selection so hovering a holder's pile lights it up everywhere. Dots fly in and settle (keyed tweens; object constancy) while playing; a scrubbed frame settles instantly so a still is readable. The falsifier for 'is egui why this feels flat?'"
                }
                Self::TimeSpineDensity => {
                    "The same three years drawn twice. One mark per transaction saturates past a few per pixel — the mint, the spikes and the quiet tail all paint the same solid bar and only the dead stretch is legible, so the lane shows when NOTHING happened. The density form keeps the ruler, playhead and in/out hues and changes only the lane's claim: a waveform from the midline (a neutral mark's shape, made about a count), root-scaled up to a robust knee and log-compressed above it, so a weeks-long frenzy keeps its shape instead of clipping into slabs or flattening three years into a hairline, floored so one-versus-none survives, with mints and burns — the events that ARE discrete — kept as marks over it. Hover a column for its count"
                }
                Self::CoverageLanes => {
                    "Three answers, not two: observed producing, observed idle, and NOBODY LOOKED. Day 4's midday orange stretch and day 7's grey column cover comparable spans and make completely different claims — one is a watched fleet sitting idle, the other is a broken poller. Fold them together and every ingest outage becomes recorded downtime. The ground state is unobserved and knowledge paints over it, so a caller cannot assert \"idle\" by forgetting to mention it; uptime divides by OBSERVED time and travels with the share of the window nobody watched. miner-06 dies on day 5 and never returns — you find it by lane shape, not by reading rows"
                }
                Self::FlowMatrix => {
                    "Who paid whom across MANY wallets at once — the face for when you do NOT yet know where to look. A matrix rather than a node-link graph, because the finding that cracks a multi-wallet case is two wallets paying the SAME counterparty, which is a column here and four edges lost in a hairball there. One unit at a time (raw Cardano quantities are not comparable), diverging out/in, log magnitude, and an unresolved payer gets its own column instead of being dropped"
                }
                Self::FlowRing => {
                    "Value moving between parties, LIVE, on the shared spine. Parties keep fixed seats on concentric rings (inner = the project's own wallets, outer = who they dealt with) and value crosses the middle as particles — ONE DOT PER QUANTUM, so a large payment is a longer train rather than a thicker line. Particle position is a pure function of the playhead, so scrubbing shows value genuinely mid-flight and a still frame is reproducible. Hover for a wallet's inventory at that exact moment; switch nodes off to cut density without moving anything that stays"
                }
                Self::FlowStave => {
                    "One wallet's money story as a SEQUENCE CHART — the narrative face the transfers table cannot be. The focal wallet holds the centre lane, counterparties fan out by ring class, and time runs downward with LOG-COMPRESSED gaps: a five-minute fund→mint→forward cascade stays a visible cluster while an idle week stays a bounded gap, with the true clock in the gutter. Direction is an arrow (blue toward the focal lane, orange away), a mint is a diamond — created, not received — an unresolved payer arrives from the chart's edge, and every arrow carries its own unit label so ADA, tokens and asset counts keep their identity on one chart"
                }
                Self::PartyAnnotator => {
                    "Turn an anonymous wallet into a named thing, ON THE RECORD. Entity is WHO is behind it (several wallets share one — that is what makes roll-up possible); label is what to call this one wallet. The basis is the point: a human filling in a form is ASSERTING, so that is the default rather than 'observed', and an assertion with no source is marked UNSOURCED in place instead of being blocked — blocking just pushes the guess into the label field where nothing can flag it"
                }
                Self::ClaimCard => {
                    "A claim, what would refute it, and whether anyone tried. Capture is free — the falsifier gates PROMOTION, not creation: provisional claims get a dashed edge and are never citable, refuted ones are KEPT struck-through with what killed them, and the header counts unsourced assertions the claim rests on"
                }
                Self::CustodyWalk => {
                    "Indented UTxO provenance tree for one traced sum — change legs continue past the payee rather than naming it as the source, depth/budget bounds render as real leaves, and the header reads PROVEN (UTxO) vs INFERRED (account chain) and PARTIAL when anything is untraced"
                }
                Self::ChannelBands => {
                    "Stacked composition over a discrete time axis — where money came from per period, with a same-unit reference line. Validated 5-hue palette, colours assigned by identity so filtering never repaints, overflow folds to Other rather than inventing hues"
                }
                Self::TagList => {
                    "Wrapping row of removable chips with an optional clear-all button — for active filters / selected facets"
                }
                Self::TokenMultiselect => {
                    "Pick a subset from a known option set — selected as removable chips + an 'add' menu of the rest (group/member/required-slot pickers)"
                }
                Self::TypeaheadSearch => {
                    "Search box with a keyboard-navigable result dropdown — up/down/enter + click, icon + verified/rug badges. Server-ranked or `filter_options`-filtered. Used by the holder-map token-search landing"
                }
                Self::RelationshipEditor => {
                    "Directed source → target edges over an option set — variant_flow / dependencies / slot-locks (and the wires in the node-graph view)"
                }
                Self::CommandPalette => {
                    "Modal ⌘K launcher — autofocused fuzzy search over the app's commands, enter dispatches, escape dismisses. Wraps TypeaheadSearch"
                }
                Self::EventWiring => {
                    "One event node wired to its action cards — pattern chips fire it, the '+ action' port opens the palette. The gateway admin's IFTTT editor"
                }
                Self::WiringEditor => {
                    "The composed binding editor both gateway surfaces mount: VM mapping, action config, add/remove drain. Toggle the host to check it survives the portal's scrolled column"
                }
                Self::ConversationHistory => {
                    "What was said, what the bot worked out, what it answered — misses with their reason, failed tool steps, dropped traces, and the billed token shape"
                }
                Self::AgentConfig => {
                    "Bring-your-own-key agent setup: provider presets, a write-only credential shown only as masked metadata, and per-role daily token budgets"
                }
                Self::Select => {
                    "react-select-shaped single select: one bordered control (value · clear · separator · chevron), floating filtered menu, keyboard nav, and values whose option no longer exists rendered flagged rather than blank"
                }
                Self::UiMachine => {
                    "Plain-enum UI state with entry-frame detection + frame-TTL auto-revert — replaces the dirty/pending/flash boolean trio with one matchable state"
                }
                Self::NamedGroupList => {
                    "Named groups with member multiselects + an optional flag — exclusive groups / bundled sets / linked traits"
                }
                Self::RarityTargetEditor => {
                    "Labelled 0–100% target sliders with a running-total-vs-budget cue — per-trait None% and per-value rarity targets"
                }
                Self::EffectEditor => {
                    "A named recolouring over a set of slots + its weighted tones — the [[effect]] config, where a variant may be a material"
                }
                Self::Knob => {
                    "PROTOTYPE — a rotary control in four faces, benched against the DragValue and the fader it would replace"
                }
                Self::SlotTable => {
                    "Slot list with enable / required toggles + z-order — disabled_traits, defaults.required, z_index_overrides"
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
                Self::PaneNav => {
                    "Shell nav with persistent selection — locked entries show their reason rather than vanishing, hides itself at one destination, wraps inside a constrained column"
                }
                Self::OptionGroup => {
                    "Related choices as ONE control — a single border with hairline separators, in stacked or inline flow, full or icon-only. Picker or selector."
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

    // ── Chrome ───────────────────────────────────────────────────────────────
    //
    // The storybook's OWN surface: sidebar, story title, the control bar. These
    // are **literals on purpose** — the chrome must NOT follow the theme being
    // reviewed.
    //
    // Three of them used to alias `theme::BG_PRIMARY` / `TEXT_MUTED` /
    // `TEXT_PRIMARY`, which meant switching theme re-skinned the tool along with
    // the widgets and you could no longer tell which you were looking at. Same
    // family of mistake as the `override_text_color` bug this module already
    // carries a warning about: the storybook must not become a surface no user
    // sees, and it must not disguise itself as one either.
    //
    // The story pane is the device; the chrome is the bezel.
    const CHROME_BG_SIDEBAR: egui::Color32 = egui::Color32::from_rgb(20, 20, 40);
    const CHROME_BG_MAIN: egui::Color32 = egui::Color32::from_rgb(26, 27, 38);
    const CHROME_TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(139, 149, 196);
    const CHROME_TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(192, 202, 245);
    const CHROME_ACCENT: egui::Color32 = egui::Color32::from_rgb(68, 255, 68);
    const CHROME_BG_SELECTED: egui::Color32 = egui::Color32::from_rgb(40, 40, 60);

    const TEXT_PRIMARY: egui::Color32 = CHROME_TEXT_PRIMARY;

    // ── Story scaffolding ────────────────────────────────────────────────────
    //
    // The prose a story writes AROUND the widget it demonstrates: section
    // headings, captions, the "what to check" lists. Inside the bezel, so these
    // follow the theme under review — unlike the `CHROME_*` constants above,
    // which must not.
    //
    // These are functions taking `ui`, not constants, and that is the whole
    // point. They were `const ACCENT/TEXT_MUTED/BG_MAIN`, and `ACCENT` was
    // `#44ff44` — a green that exists in no theme, so every story heading in the
    // storybook was showing the reader a colour no app can render, while sitting
    // directly above a widget that had just been migrated to tokens. The
    // switcher changed the widget and not one word of the text describing it.
    //
    // A constant cannot read a theme. That is not an inconvenience to work
    // around with a cached palette or a per-frame global — it is the type system
    // stating the actual constraint, which is that a colour is a function of the
    // active theme and therefore needs something to ask.

    /// The story's own accent — headings, the selected item in a preset row.
    pub fn accent(ui: &egui::Ui) -> egui::Color32 {
        egui_widgets::theme::ThemeExt::tokens(ui).color.accent
    }

    /// Captions, hints, the unselected half of a toggle row.
    pub fn muted(ui: &egui::Ui) -> egui::Color32 {
        egui_widgets::theme::ThemeExt::tokens(ui).color.text_muted
    }

    /// Body text a story writes in its own voice.
    pub fn ink(ui: &egui::Ui) -> egui::Color32 {
        egui_widgets::theme::ThemeExt::tokens(ui).color.text_primary
    }

    /// The ground a story paints its own panels on.
    pub fn bg(ui: &egui::Ui) -> egui::Color32 {
        egui_widgets::theme::ThemeExt::tokens(ui).color.bg_primary
    }

    /// A tier between [`ink`] and [`muted`] — secondary labels, units, the
    /// quieter half of a two-part value.
    pub fn secondary(ui: &egui::Ui) -> egui::Color32 {
        egui_widgets::theme::ThemeExt::tokens(ui)
            .color
            .text_secondary
    }

    /// A raised panel or a selected row, against [`bg`].
    pub fn highlight(ui: &egui::Ui) -> egui::Color32 {
        egui_widgets::theme::ThemeExt::tokens(ui).color.bg_highlight
    }

    /// Any other token, by name.
    ///
    /// The six shorthands above are the story *scaffolding* vocabulary — the
    /// colours a story uses to write about a widget. This is for the rest: the
    /// demo data a story invents, where the colour is standing in for a status
    /// or a series rather than for prose. `tok(ui, Token::Success)` says which,
    /// and unlike the `theme::SUCCESS` constant it replaced, it follows the
    /// theme under review.
    ///
    /// Deliberately not six more shorthands. A story reaching past the
    /// scaffolding should have to name the token it wants, because that is the
    /// moment to ask whether the story is demonstrating the widget or just
    /// decorating itself.
    pub fn tok(ui: &egui::Ui, token: egui_widgets::theme::Token) -> egui::Color32 {
        token.get(&egui_widgets::theme::ThemeExt::tokens(ui).color)
    }

    /// A section heading inside a story.
    ///
    /// Exists because `ui.label(RichText::new(t).color(ACCENT).strong())` was
    /// written out ~400 times. Saying `heading(ui, t)` instead is shorter, and
    /// more importantly it gives the storybook ONE place to decide what a
    /// heading looks like — which is what made it possible to notice that every
    /// one of them was the wrong colour.
    pub fn heading(ui: &mut egui::Ui, text: impl Into<String>) {
        let c = accent(ui);
        ui.label(egui::RichText::new(text.into()).color(c).strong());
    }

    /// A small muted caption under a heading or beside a control.
    pub fn caption(ui: &mut egui::Ui, text: impl Into<String>) {
        let c = muted(ui);
        ui.label(egui::RichText::new(text.into()).color(c).small());
    }

    /// How long a story's control column gets before it stops growing.
    ///
    /// A fader with a 1200px throw is not more useful than one with 460 — past
    /// a point the extra travel buys no precision and costs the reader a long
    /// mouse journey. So the storybook takes a view on its OWN control column,
    /// which is a different thing from a widget taking a view on its callers:
    /// this is one app's layout decision, stated once.
    const CONTROLS_W: f32 = 460.0;

    /// Run `add` inside a story's control column.
    ///
    /// `fit_width` clamps to the CONTAINER, so a narrow pane or a side-by-side
    /// theme comparison still gets a working bank rather than one that overflows.
    pub fn controls<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        use egui_widgets::viewport::LayoutExt as _;
        ui.scope(|ui| {
            ui.set_max_width(ui.fit_width(CONTROLS_W));
            add(ui)
        })
        .inner
    }

    // ========================================================================
    // Review controls
    // ========================================================================

    /// What the reader has asked to review the story *under*: a theme, optionally
    /// a second theme beside it, a density, a motion mode and a forced breakpoint.
    ///
    /// # Why this lives in the URL
    ///
    /// Every one of these is a query parameter, and the pickers write back to the
    /// address bar. That is not a convenience — `tools/cdp-shot.mjs` takes a URL
    /// and a viewport, so putting the whole review state in the URL makes the
    /// screenshot matrix addressable **with no changes to the harness at all**.
    /// Pickers are for humans; parameters are for the matrix; the pickers keep
    /// them in step so a reader can paste what they are looking at to someone
    /// else.
    /// `Clone` but not `Copy`: [`egui_widgets::theme::Theme`] is deliberately not
    /// `Copy`, because the later axes (series palettes) will not be.
    #[derive(Clone)]
    struct ReviewControls {
        theme: egui_widgets::theme::Theme,
        /// `Some` renders the story twice, side by side — see [`Self::draw_story`].
        compare: Option<egui_widgets::theme::Theme>,
        density: egui_widgets::theme::Density,
        motion: egui_widgets::theme::MotionMode,
        /// `None` measures the real viewport, as an app would.
        breakpoint: Option<egui_widgets::viewport::Breakpoint>,
    }

    impl Default for ReviewControls {
        fn default() -> Self {
            let base = egui_widgets::theme::Theme::tokyo_night();
            Self {
                compare: None,
                density: base.density,
                motion: base.motion.mode,
                breakpoint: None,
                theme: base,
            }
        }
    }

    impl ReviewControls {
        /// Read the controls out of `?theme=…&vs=…&density=…&motion=…&bp=…`.
        fn from_query(query: &str) -> Self {
            let mut out = Self::default();
            let param = |key: &str| -> Option<String> {
                query
                    .trim_start_matches('?')
                    .split('&')
                    .filter_map(|kv| kv.split_once('='))
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| v.replace('+', " ").replace("%20", " "))
            };

            if let Some(name) = param("theme") {
                if let Some(t) = egui_widgets::theme::Theme::by_name(&name) {
                    out.density = t.density;
                    out.motion = t.motion.mode;
                    out.theme = t;
                }
            }
            if let Some(name) = param("vs") {
                out.compare = egui_widgets::theme::Theme::by_name(&name);
            }
            if let Some(name) = param("density") {
                if let Some(d) = egui_widgets::theme::Density::ALL
                    .iter()
                    .find(|d| d.label().eq_ignore_ascii_case(&name))
                {
                    out.density = *d;
                }
            }
            if let Some(name) = param("motion") {
                out.motion = match name.to_ascii_lowercase().as_str() {
                    "none" => egui_widgets::theme::MotionMode::None,
                    "reduced" => egui_widgets::theme::MotionMode::Reduced,
                    _ => egui_widgets::theme::MotionMode::Full,
                };
            }
            if let Some(name) = param("bp") {
                out.breakpoint = egui_widgets::viewport::Breakpoint::by_name(&name);
            }
            out
        }

        /// The theme as selected, with the density and motion overrides folded in.
        fn resolved(&self) -> egui_widgets::theme::Theme {
            self.theme
                .clone()
                .with_density(self.density)
                .with_motion(self.motion)
        }

        /// Install the selection for this frame.
        fn apply(&self, ctx: &egui::Context) {
            egui_widgets::theme::install_theme(ctx, self.resolved());
            egui_widgets::viewport::override_breakpoint(ctx, self.breakpoint);
        }

        /// Render the story under the selected theme — twice, side by side, when a
        /// comparison theme is set.
        ///
        /// The A/B half is the reason `theme::scoped` exists: a palette regression
        /// is obvious beside its control and nearly invisible when you have to flip
        /// between two screenshots to find it.
        fn draw_story(&self, story: Story, app: &mut StorybookApp, ui: &mut egui::Ui) {
            let Some(other) = self.compare.clone() else {
                story.draw(app, ui);
                return;
            };

            let half = (ui.available_width() - 24.0) * 0.5;
            let mine = self.resolved();
            ui.horizontal_top(|ui| {
                for (theme, side) in [(mine, "A"), (other, "B")] {
                    ui.allocate_ui(egui::vec2(half, ui.available_height()), |ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{side} · {}", theme.name))
                                    .color(CHROME_TEXT_MUTED)
                                    .small()
                                    .strong(),
                            );
                            // The per-side theme has to reach BOTH `ui.tokens()`
                            // and egui's own `Style`, which is what `scoped` does.
                            egui_widgets::theme::scoped(ui, &theme, |ui| {
                                story.draw(app, ui);
                            });
                        });
                    });
                    ui.separator();
                }
            });
        }

        /// The picker row. Writes every change back to the URL so the matrix and
        /// the reader address the same thing.
        fn controls(&mut self, ui: &mut egui::Ui) {
            let before = self.clone();
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("review")
                        .color(CHROME_TEXT_MUTED)
                        .small()
                        .strong(),
                );

                for preset in egui_widgets::theme::Theme::PRESETS {
                    let t = preset();
                    let on = self.theme.name == t.name;
                    if chrome_toggle(ui, t.name, on).clicked() {
                        self.density = t.density;
                        self.motion = t.motion.mode;
                        self.theme = t;
                    }
                }

                ui.separator();
                ui.label(egui::RichText::new("vs").color(CHROME_TEXT_MUTED).small());
                if chrome_toggle(ui, "off", self.compare.is_none()).clicked() {
                    self.compare = None;
                }
                for preset in egui_widgets::theme::Theme::PRESETS {
                    let t = preset();
                    let on = self.compare.as_ref().is_some_and(|c| c.name == t.name);
                    if chrome_toggle(ui, t.name, on).clicked() {
                        self.compare = Some(t);
                    }
                }
            });

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("density")
                        .color(CHROME_TEXT_MUTED)
                        .small()
                        .strong(),
                );
                for d in egui_widgets::theme::Density::ALL {
                    if chrome_toggle(ui, d.label(), self.density == *d).clicked() {
                        self.density = *d;
                    }
                }

                ui.separator();
                ui.label(
                    egui::RichText::new("motion")
                        .color(CHROME_TEXT_MUTED)
                        .small()
                        .strong(),
                );
                for m in egui_widgets::theme::MotionMode::ALL {
                    let name = match m {
                        egui_widgets::theme::MotionMode::Full => "full",
                        egui_widgets::theme::MotionMode::Reduced => "reduced",
                        egui_widgets::theme::MotionMode::None => "none",
                    };
                    if chrome_toggle(ui, name, self.motion == *m).clicked() {
                        self.motion = *m;
                    }
                }

                ui.separator();
                ui.label(
                    egui::RichText::new("breakpoint")
                        .color(CHROME_TEXT_MUTED)
                        .small()
                        .strong(),
                );
                if chrome_toggle(ui, "measured", self.breakpoint.is_none()).clicked() {
                    self.breakpoint = None;
                }
                for bp in egui_widgets::viewport::Breakpoint::ALL {
                    let on = self.breakpoint == Some(bp);
                    if chrome_toggle(ui, bp.label(), on).clicked() {
                        self.breakpoint = Some(bp);
                    }
                }
            });

            if !self.same_as(&before) {
                set_location_query(&self.to_query());
            }
        }

        fn same_as(&self, other: &Self) -> bool {
            self.theme.name == other.theme.name
                && self.compare.as_ref().map(|c| c.name) == other.compare.as_ref().map(|c| c.name)
                && self.density == other.density
                && self.motion == other.motion
                && self.breakpoint == other.breakpoint
        }

        /// The inverse of [`Self::from_query`] — what the address bar should say.
        fn to_query(&self) -> String {
            let mut parts = vec![format!("theme={}", self.theme.name.replace(' ', "+"))];
            if let Some(c) = self.compare.as_ref() {
                parts.push(format!("vs={}", c.name.replace(' ', "+")));
            }
            parts.push(format!("density={}", self.density.label()));
            parts.push(format!(
                "motion={}",
                match self.motion {
                    egui_widgets::theme::MotionMode::Full => "full",
                    egui_widgets::theme::MotionMode::Reduced => "reduced",
                    egui_widgets::theme::MotionMode::None => "none",
                }
            ));
            if let Some(bp) = self.breakpoint {
                parts.push(format!("bp={}", bp.label()));
            }
            parts.join("&")
        }
    }

    /// A chrome-coloured toggle. Not `egui_widgets`' own button: the control bar
    /// must stay legible whatever the theme under review does.
    fn chrome_toggle(ui: &mut egui::Ui, label: &str, on: bool) -> egui::Response {
        let text = egui::RichText::new(label).size(10.0).color(if on {
            CHROME_ACCENT
        } else {
            CHROME_TEXT_MUTED
        });
        let fill = if on {
            CHROME_BG_SELECTED
        } else {
            egui::Color32::TRANSPARENT
        };
        ui.add(egui::Button::new(text).fill(fill).small())
    }

    fn configure_style(ctx: &egui::Context) {
        // Use the shipped theme, not a private one. The old private style set
        // `override_text_color`, so every default label screenshotted at
        // 12.6:1 while the same label in a real app rendered near 4:1 —
        // storybook reviews were reviewing a surface no user sees.
        egui_widgets::theme::configure_style(
            ctx,
            egui_widgets::theme::FontStrategy::proportional(),
        );
    }

    // ========================================================================
    // Deep linking
    // ========================================================================

    /// The story named by `#/<slug>` in the address bar, if any.
    ///
    /// Deep links make a story reviewable without a click-through, which is
    /// what lets a headless browser screenshot one directly — and lets a
    /// reviewer be pointed at an exact widget rather than "it's under
    /// Primitives".
    #[cfg(target_arch = "wasm32")]
    fn story_from_location() -> Option<Story> {
        let hash = web_sys::window()?.location().hash().ok()?;
        Story::from_slug(&hash)
    }

    /// Native builds have no address bar; `STORYBOOK_STORY` stands in, so the
    /// same deep link works from a shell.
    #[cfg(not(target_arch = "wasm32"))]
    fn story_from_location() -> Option<Story> {
        Story::from_slug(&std::env::var("STORYBOOK_STORY").ok()?)
    }

    #[cfg(target_arch = "wasm32")]
    fn set_location_hash(slug: &str) {
        if let Some(w) = web_sys::window() {
            let _ = w.location().set_hash(slug);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn set_location_hash(_slug: &str) {}

    /// The review state as it arrived — `?theme=…&vs=…&density=…&motion=…&bp=…`.
    #[cfg(target_arch = "wasm32")]
    fn review_from_location() -> ReviewControls {
        let query = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .unwrap_or_default();
        ReviewControls::from_query(&query)
    }

    /// Native builds have no address bar; `STORYBOOK_REVIEW` stands in, so the
    /// same selection works from a shell — `STORYBOOK_REVIEW='theme=ember&vs=iris'`.
    #[cfg(not(target_arch = "wasm32"))]
    fn review_from_location() -> ReviewControls {
        ReviewControls::from_query(&std::env::var("STORYBOOK_REVIEW").unwrap_or_default())
    }

    /// Write the review selection back to the address bar, preserving `#/story`
    /// and `nav=0`.
    ///
    /// `replace_state` rather than assigning `location.search`, which would
    /// reload the page and throw away the wasm app mid-frame.
    #[cfg(target_arch = "wasm32")]
    fn set_location_query(query: &str) {
        let Some(w) = web_sys::window() else { return };
        let keep_nav = w
            .location()
            .search()
            .ok()
            .is_some_and(|s| s.contains("nav=0"));
        let hash = w.location().hash().unwrap_or_default();
        let url = format!("?{query}{}{hash}", if keep_nav { "&nav=0" } else { "" });
        if let Ok(history) = w.history() {
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&url));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn set_location_query(_query: &str) {}

    /// `?nav=0` drops the story list so the story gets the WHOLE viewport.
    ///
    /// This exists for narrow-width review. The sidebar is a fixed 180px, so a
    /// 390px phone viewport left the story 200px — every card overflowed, and
    /// the overflow was the storybook's chrome rather than anything the widget
    /// did. Screenshotting a wider viewport to compensate is not the same test:
    /// what a widget does at 390 is the thing being checked.
    #[cfg(target_arch = "wasm32")]
    fn nav_hidden() -> bool {
        web_sys::window()
            .and_then(|w| w.location().search().ok())
            .is_some_and(|s| s.contains("nav=0"))
    }

    /// Native builds have no address bar; the env var stands in, as with
    /// `STORYBOOK_STORY`.
    #[cfg(not(target_arch = "wasm32"))]
    fn nav_hidden() -> bool {
        std::env::var("STORYBOOK_NAV").is_ok_and(|v| v == "0")
    }

    // ========================================================================
    // App
    // ========================================================================

    /// `pub` only to match the visibility of the macro-generated
    /// [`Story::draw`], which takes `&mut StorybookApp`. A `pub fn` whose
    /// parameter type is private is a `private_interfaces` warning, and the
    /// honest resolution is that the app type IS part of that signature.
    /// Nothing outside the crate constructs one — `mod app` is `cfg`-gated to
    /// wasm and re-exported wholesale.
    /// Prefix marking a palette entry the SHELL owns, so a story command and a
    /// navigation entry can share one flat list without a collision. A widget
    /// id could in principle start with this; the prefix is ugly enough that
    /// none will.
    const GOTO: &str = "storybook:goto:";

    pub struct StorybookApp {
        current_story: Story,
        /// `?nav=0` — see [`nav_hidden`].
        nav_hidden: bool,
        /// Whether the Compact-layout nav drawer is open. Only consulted when
        /// the chrome is narrow enough to have one.
        nav_open: bool,
        /// Theme / density / motion / breakpoint under review — see
        /// [`ReviewControls`].
        review: ReviewControls,
        /// ⌘K over every story plus whatever the story on screen offers. The
        /// storybook is the first real consumer of `egui_widgets::commands`,
        /// which is the point: a mechanism for widgets to advertise themselves
        /// is only worth having if the app that shows every widget uses it.
        palette: egui_widgets::command_palette::PaletteState,
        // Per-story state
        distribution_chart: egui_widgets::DistributionChart,
        marquee: egui_widgets::Marquee,
        marquee_messages: Vec<egui_widgets::MarqueeItem>,
        progress_bar_state: stories::progress_bar::ProgressBarState,
        disclosure_state: stories::disclosure::State,
        slider_group_state: stories::slider_group::SliderGroupStoryState,
        bullet_bar_state: stories::bullet_bar::BulletBarState,
        tag_list_state: stories::tag_list::TagListState,
        token_multiselect_state: stories::token_multiselect::TokenMultiselectState,
        relationship_editor_state: stories::relationship_editor::RelationshipEditorState,
        command_palette_state: stories::command_palette::CommandPaletteState,
        event_wiring_state: stories::event_wiring::EventWiringState,
        wiring_editor_state: stories::wiring_editor::WiringEditorStory,
        conversation_history_state: stories::conversation_history::ConversationHistoryStory,
        agent_config_state: stories::agent_config::AgentConfigStory,
        select_state: stories::select::SelectStory,
        machine_state: stories::machine::MachineState,
        named_group_list_state: stories::named_group_list::NamedGroupListState,
        rarity_target_editor_state: stories::rarity_target_editor::RarityTargetEditorState,
        effect_editor_state: stories::effect_editor::EffectEditorState,
        knob_state: stories::knob::KnobState,
        slot_table_state: stories::slot_table::SlotTableState,
        sparkline_state: stories::sparkline::SparklineState,
        perf_strip_state: stories::perf_strip::PerfStripStory,
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
        price_timeline_state: stories::price_timeline::PriceTimelineState,
        leaderboard_state: stories::leaderboard::LeaderboardState,
        listing_grid_state: stories::listing_grid::ListingGridState,
        focus_list_state: stories::focus_list::FocusListState,
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
        trade_flow_state: stories::trade_flow::TradeFlowStoryState,
        signing_status_state: stories::signing_status::SigningStatusStoryState,
        tx_flight_state: stories::tx_flight::TxFlightStoryState,
        stake_session_state: stories::stake_session::StakeSessionStoryState,
        listing_composer_state: stories::listing_composer::ListingComposerStoryState,
        trade_table_state: stories::trade_table::TradeTableStoryState,
        wallet_asset_picker_state: stories::wallet_asset_picker::WalletAssetPickerStoryState,
        utxo_shelf_state: stories::utxo_shelf::UtxoShelfStoryState,
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
        pane_nav_state: stories::pane_nav::PaneNavState,
        option_group_state: stories::option_group::OptionGroupStoryState,
        toast_state: stories::toast::ToastState,
        claim_card_state: stories::claim_card::ClaimCardState,
        capital_flow_state: stories::capital_flow::CapitalFlowState,
        cap_band_state: stories::cap_band::CapBandState,
        tx_card_state: stories::tx_card::TxCardState,
        image_stack_state: stories::image_stack::ImageStackState,
        time_spine_state: stories::time_spine::TimeSpineState,
        time_spine_density_state: stories::time_spine_density::TimeSpineDensityState,
        coverage_lanes_state: stories::coverage_lanes::CoverageLanesState,
        flow_matrix_state: stories::flow_matrix::FlowMatrixState,
        flow_ring_state: stories::flow_ring::FlowRingState,
        flow_stave_state: stories::flow_stave::FlowStaveState,
        party_annotator_state: stories::party_annotator::PartyAnnotatorState,
    }

    impl StorybookApp {
        fn new(cc: &eframe::CreationContext<'_>) -> Self {
            configure_style(&cc.egui_ctx);
            // `install_assets` and NOT `install`: the storybook is the case that
            // escape hatch exists for. `ReviewControls::apply` installs a theme
            // every pass from what the reader picked, so binding one here would
            // be overwritten before the first widget drew.
            egui_widgets::install_assets(&cc.egui_ctx);
            // Inter as the primary proportional face (from the font bucket), in front of
            // the bundled default + DejaVu fallback. Async fetch; swaps in once it lands.
            egui_widgets::fonts::load_remote_font(
                &cc.egui_ctx,
                egui_widgets::fonts::r2::INTER_REGULAR,
                egui::FontFamily::Proportional,
                egui::epaint::text::FontPriority::Highest,
            );
            egui_widgets::fonts::load_remote_font(
                &cc.egui_ctx,
                egui_widgets::fonts::r2::INTER_BOLD,
                egui::FontFamily::Proportional,
                egui::epaint::text::FontPriority::Highest,
            );

            Self {
                current_story: story_from_location().unwrap_or(Story::Distribution),
                // Read ONCE at startup: with the nav gone there is no way to
                // change stories, so this is a per-load mode, not a toggle.
                nav_hidden: nav_hidden(),
                nav_open: false,
                review: review_from_location(),
                palette: Default::default(),
                distribution_chart: egui_widgets::DistributionChart::new(),
                marquee: egui_widgets::Marquee::default(),
                marquee_messages: vec![egui_widgets::MarqueeItem {
                    text: "Welcome to the egui Widgets Storybook".into(),
                    color: CHROME_ACCENT,
                }],
                progress_bar_state: stories::progress_bar::ProgressBarState::default(),
                disclosure_state: stories::disclosure::State::default(),
                slider_group_state: stories::slider_group::SliderGroupStoryState::default(),
                bullet_bar_state: stories::bullet_bar::BulletBarState::default(),
                tag_list_state: stories::tag_list::TagListState::default(),
                token_multiselect_state: stories::token_multiselect::TokenMultiselectState::default(
                ),
                relationship_editor_state:
                    stories::relationship_editor::RelationshipEditorState::default(),
                command_palette_state: stories::command_palette::CommandPaletteState::default(),
                event_wiring_state: stories::event_wiring::EventWiringState::default(),
                wiring_editor_state: stories::wiring_editor::WiringEditorStory::default(),
                conversation_history_state:
                    stories::conversation_history::ConversationHistoryStory::default(),
                agent_config_state: stories::agent_config::AgentConfigStory::default(),
                select_state: stories::select::SelectStory::default(),
                machine_state: stories::machine::MachineState::default(),
                named_group_list_state: stories::named_group_list::NamedGroupListState::default(),
                rarity_target_editor_state:
                    stories::rarity_target_editor::RarityTargetEditorState::default(),
                effect_editor_state: stories::effect_editor::EffectEditorState::default(),
                knob_state: stories::knob::KnobState::default(),
                slot_table_state: stories::slot_table::SlotTableState::default(),
                sparkline_state: stories::sparkline::SparklineState::default(),
                perf_strip_state: stories::perf_strip::PerfStripStory::default(),
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
                price_timeline_state: stories::price_timeline::PriceTimelineState::default(),
                leaderboard_state: stories::leaderboard::LeaderboardState::default(),
                listing_grid_state: stories::listing_grid::ListingGridState::default(),
                focus_list_state: stories::focus_list::FocusListState::default(),
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
                trade_flow_state: stories::trade_flow::TradeFlowStoryState::default(),
                signing_status_state: stories::signing_status::SigningStatusStoryState::default(),
                tx_flight_state: stories::tx_flight::TxFlightStoryState::default(),
                stake_session_state: stories::stake_session::StakeSessionStoryState::default(),
                listing_composer_state:
                    stories::listing_composer::ListingComposerStoryState::default(),
                trade_table_state: stories::trade_table::TradeTableStoryState::default(),
                wallet_asset_picker_state:
                    stories::wallet_asset_picker::WalletAssetPickerStoryState::default(),
                utxo_shelf_state: stories::utxo_shelf::UtxoShelfStoryState::default(),
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
                pane_nav_state: stories::pane_nav::PaneNavState::default(),
                option_group_state: stories::option_group::OptionGroupStoryState::default(),
                toast_state: stories::toast::ToastState::default(),
                claim_card_state: stories::claim_card::ClaimCardState::default(),
                capital_flow_state: stories::capital_flow::CapitalFlowState::default(),
                cap_band_state: stories::cap_band::CapBandState::default(),
                tx_card_state: stories::tx_card::TxCardState::default(),
                image_stack_state: stories::image_stack::ImageStackState::default(),
                time_spine_state: stories::time_spine::TimeSpineState::default(),
                time_spine_density_state:
                    stories::time_spine_density::TimeSpineDensityState::default(),
                coverage_lanes_state: stories::coverage_lanes::CoverageLanesState::default(),
                flow_matrix_state: stories::flow_matrix::FlowMatrixState::default(),
                flow_ring_state: stories::flow_ring::FlowRingState::default(),
                flow_stave_state: stories::flow_stave::FlowStaveState::default(),
                party_annotator_state: stories::party_annotator::PartyAnnotatorState::default(),
            }
        }

        /// ⌘K: every story, plus whatever the story on screen is offering.
        ///
        /// The two sources are merged rather than one replacing the other,
        /// which is the arrangement a real app wants too: navigation belongs to
        /// the shell and can never be offered by a widget, while "Add wallet"
        /// belongs to the roster and the shell should not have to know it
        /// exists. Neither list is complete on its own.
        fn draw_palette(&mut self, ui: &mut egui::Ui) {
            use egui_widgets::typeahead_search::TypeaheadOption;

            let mut options: Vec<TypeaheadOption> = Story::all()
                .iter()
                .filter(|s| **s != self.current_story)
                .map(|s| {
                    TypeaheadOption::new(format!("{GOTO}{}", s.label()), s.label())
                        .subtitle(s.category())
                })
                .collect();
            // What the story currently on screen can do. Nothing here knows
            // what that is — the widgets said so themselves.
            options.extend(egui_widgets::commands::offered_options(ui.ctx()));

            match egui_widgets::command_palette::CommandPalette::new("storybook", &options)
                .placeholder("Go to a story, or run something on this one…")
                .show(ui, &mut self.palette)
            {
                egui_widgets::command_palette::PaletteAction::Invoke(id) => {
                    match id.strip_prefix(GOTO) {
                        Some(label) => {
                            if let Some(s) = Story::all().iter().find(|s| s.label() == label) {
                                self.current_story = *s;
                            }
                        }
                        // Not ours. Hand it back to the registry and whichever
                        // widget offered it will claim it — the shell never needs
                        // to learn what the command does.
                        None => egui_widgets::commands::invoke(ui.ctx(), id),
                    }
                }
                egui_widgets::command_palette::PaletteAction::None => {}
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
                            .color(CHROME_TEXT_MUTED)
                            .small()
                            .strong(),
                    );
                }
                let is_selected = self.current_story == *story;
                let text = if is_selected {
                    egui::RichText::new(story.label())
                        .color(CHROME_ACCENT)
                        .strong()
                } else {
                    egui::RichText::new(story.label()).color(TEXT_PRIMARY)
                };
                let fill = if is_selected {
                    CHROME_BG_SELECTED
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
                    set_location_hash(&story.slug());
                }
            }
        }
    }

    impl eframe::App for StorybookApp {
        // eframe 0.34 made `ui` the required App method (was `update` in 0.33);
        // panels nest via `show(ui, …)` instead of the old `show(ctx, …)`.
        fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            // The perf_strip story shows the storybook's OWN cost, so the frame
            // clock has to be wired up here rather than inside the story — a
            // story that measured only itself would report a fraction of the
            // frame and read as far cheaper than the app really is.
            let _frame_scope = egui_widgets::perf_strip::FrameScope::begin();
            let ctx = ui.ctx().clone();

            // THE CHROME'S OWN BREAKPOINT, measured — never `Breakpoint::from_ctx`,
            // which honours the review controls' override. Those exist so a
            // reader at a desk can ask "what does this story do at Compact";
            // if the chrome read them too, choosing Compact would fold the nav
            // away on a 27" monitor and there would be no way back to the
            // control that did it.
            let story_before = self.current_story;
            // `content_rect`, not `screen_rect`: the latter includes the notch
            // and the home indicator, which is exactly the difference that
            // matters on the device this is for.
            let chrome_bp = Breakpoint::from_width(ctx.content_rect().width());
            let narrow = chrome_bp.panel_mode() == PanelMode::Drawer;
            // The storybook has been shipping a layout engine it did not use on
            // itself: 180pt of permanent sidebar is 46% of a phone.
            egui_widgets::viewport::apply_touch_sizing(&ctx, chrome_bp);

            if !self.nav_hidden && !narrow {
                egui::Panel::left("stories")
                    .default_size(180.0)
                    .resizable(false)
                    .frame(egui::Frame::side_top_panel(&ctx.global_style()).fill(CHROME_BG_SIDEBAR))
                    .show(ui, |ui| {
                        ui.add_space(8.0);
                        ui.heading(egui::RichText::new("egui Widgets").color(CHROME_ACCENT));
                        ui.separator();
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            self.draw_sidebar(ui);
                        });
                    });
            }

            // The review controls are applied BEFORE the story pane draws, so the
            // story renders under the theme the reader selected this frame rather
            // than lagging one behind.
            self.review.apply(&ctx);

            egui::CentralPanel::default()
                .frame(egui::Frame::central_panel(&ctx.global_style()).fill(CHROME_BG_MAIN))
                .show(ui, |ui| {
                    // Chrome, deliberately NOT under the theme being reviewed —
                    // see `CHROME_*`.
                    ui.horizontal(|ui| {
                        if !self.nav_hidden && narrow {
                            // The nav has to be reachable, and a hamburger is
                            // the one affordance every phone reader already
                            // knows. `Drawer` is the crate's own answer for a
                            // side panel at Compact, so the storybook uses it
                            // rather than growing a second one.
                            let open = ui
                                .button(egui::RichText::new("☰").color(CHROME_ACCENT))
                                .on_hover_text("Stories")
                                .clicked();
                            if open {
                                self.nav_open = true;
                            }
                        }
                        ui.heading(
                            egui::RichText::new(self.current_story.label())
                                .color(CHROME_TEXT_PRIMARY),
                        );
                    });
                    // At Compact the description is several lines of chrome
                    // above the thing being reviewed, on the screen with the
                    // least room for it.
                    if !narrow {
                        ui.label(
                            egui::RichText::new(self.current_story.description())
                                .color(CHROME_TEXT_MUTED),
                        );
                    }
                    if !self.nav_hidden {
                        self.review.controls(ui);
                    }
                    ui.separator();
                    ui.add_space(8.0);

                    // Drag-to-scroll stays ON. A phone has no other way to move
                    // a long story, and the gesture conflict it creates with the
                    // controls inside is now handled where it belongs — see
                    // `egui_widgets::touch`, which makes a control transparent
                    // until it is grabbed and then takes the drag explicitly.
                    // Clearing `drag` here would fix the controls by breaking
                    // the page.
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let story = self.current_story;
                        let review = self.review.clone();
                        review.draw_story(story, self, ui);
                    });

                    // AFTER the story, so the offers it made this pass are
                    // already in the registry. It would work drawn before —
                    // `commands` keeps an offer live for one pass precisely
                    // because a palette usually sits above what it lists — but
                    // there is no reason to spend that grace when the order is
                    // ours to choose.
                    self.draw_palette(ui);

                    if narrow {
                        let mut open = self.nav_open;
                        Drawer::new("storybook-nav")
                            .side(DrawerSide::Left)
                            .width(260.0)
                            .show(ui, &mut open, |ui| {
                                ui.heading(
                                    egui::RichText::new("egui Widgets").color(CHROME_ACCENT),
                                );
                                ui.separator();
                                egui::ScrollArea::vertical().show(ui, |ui| {
                                    self.draw_sidebar(ui);
                                });
                            });
                        // Picking a story should close the drawer; `draw_sidebar`
                        // does not know it is in one, so the change is detected
                        // here instead of teaching it.
                        if self.nav_open && self.current_story != story_before {
                            open = false;
                        }
                        self.nav_open = open;
                    }
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
