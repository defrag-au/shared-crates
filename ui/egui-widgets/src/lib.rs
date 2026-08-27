#[cfg(target_arch = "wasm32")]
pub use egui_inbox;

pub mod access_gate;
pub mod activity_feed;
pub mod activity_lanes;
pub mod animated_counter;
pub mod arrival_field;
pub mod asset_card;
pub mod background;
pub mod bullet_bar;
pub mod button_group;
pub mod buttons;
pub mod capital_flow;
pub mod card_browser;
pub mod channel_bands;
pub mod chip;
pub mod claim_card;
pub mod collection_composition;
pub mod collection_list;
pub mod command_palette;
pub mod detail_split;
pub mod distribution_waterfall;
pub mod donut_chart;
pub mod error_note;
pub mod event_wiring;
#[cfg(target_arch = "wasm32")]
pub mod file_upload;
pub mod flip_counter;
pub mod flow_ledger;
pub mod flow_matrix;
pub mod flow_ring;
pub mod flow_stave;
pub mod focus_list;
pub mod fonts;
pub mod fungibles_row;
pub mod gated;
pub mod grouped_section;
pub mod holder_field;
pub mod holder_formation;
pub mod icons;
pub mod id_pill;
pub mod image_loader;
pub mod leaderboard;
pub mod listing_grid;
pub mod machine;
pub mod marquee;
pub mod metric_card;
pub mod mint_arrivals;
pub mod mint_checkout;
pub mod mnemonic_display;
pub mod motion;
pub mod named_group_list;
pub mod offer_tile;
pub mod order_list;
pub mod palette_editor;
pub mod party_annotator;
pub mod party_badge;
pub mod party_finder;
pub mod persona_strip;
pub mod phase_card;
pub mod pip_row;
pub mod price_timeline;
pub mod printing_timeline;
pub mod progress_bar;
pub mod property_list;
pub mod quantity_stepper;
pub mod radar_chart;
pub mod range_bar;
pub mod rarity_target_editor;
pub mod relationship_editor;
pub mod relative_time;
pub mod screenshot;
pub mod selection;
pub mod seven_segment;
pub mod slot_table;
pub mod sparkline;
pub mod stat_strip;
pub mod supply_bar;
pub mod swap_modal;
pub mod tag_list;
pub mod theme;
pub mod time_spine;
pub mod timestamp;
pub mod toast;
pub mod token_multiselect;
pub mod trade_flow;
pub mod trait_filter;
pub mod typeahead_search;
pub mod user_badge;
pub mod utils;
pub mod variant_split;
#[cfg(all(target_arch = "wasm32", feature = "cardano"))]
pub mod wallet;
#[cfg(all(target_arch = "wasm32", feature = "cardano"))]
pub mod wallet_button;
#[cfg(feature = "cardano")]
pub mod wallet_editor;
pub mod wallet_identity_header;
pub mod wallet_list;
#[cfg(all(target_arch = "wasm32", feature = "cardano"))]
pub mod wallet_mock;

// Image text editor (feature-gated)
#[cfg(feature = "image-editor")]
pub mod image_text_editor;

// DEX split swap widgets
pub mod amount_input;
pub mod pool_liquidity_indicator;
pub mod price_impact_curve;
pub mod route_summary;
pub mod slippage_selector;
pub mod split_allocation_bar;

// Loan dashboard widgets
pub mod custody_walk;
pub mod data_table;
pub mod exposure_bar;

// Ranked-list dashboards (holders, leaderboards, top traders)
pub mod leaderboard_table;

// Generic coverage bar (no cardano deps — usable everywhere, e.g. rarity tuning).
pub mod coverage_delta_bar;

// Cardano-specific widgets (feature-gated)
#[cfg(feature = "cardano")]
pub mod asset_strip;
#[cfg(feature = "cardano")]
pub mod fee_report;
#[cfg(feature = "cardano")]
pub mod managed_wallet_utxos;
#[cfg(feature = "cardano")]
pub mod offer_slot;
#[cfg(feature = "cardano")]
pub mod signing_status;
#[cfg(feature = "cardano")]
pub mod trade_table;
#[cfg(feature = "cardano")]
pub mod trait_delta;
pub mod tx_cart;
#[cfg(feature = "cardano")]
pub mod tx_estimate;
#[cfg(feature = "cardano")]
pub mod utxo_map;
#[cfg(feature = "cardano")]
pub mod utxo_shelf;
#[cfg(feature = "cardano")]
pub mod wallet_asset_picker;

pub use activity_feed::{
    ActivityAsset, ActivityEntry, ActivityFeed, ActivityFeedResponse, ActivityTag,
};
pub use activity_lanes::{ActivityLanes, ActivityLanesResponse, Lane};
pub use animated_counter::AnimatedCounter;
pub use arrival_field::{ArrivalField, ArrivalFieldResponse};
pub use button_group::{ButtonGroup, ButtonGroupButton, ButtonGroupResponse};
pub use buttons::UiButtonExt;
pub use capital_flow::{
    Band, CapitalFlow, CapitalFlowResponse, CapitalState, FlowEvent, bands as capital_bands,
    cumulative_at, legend as capital_legend, state_at,
};
pub use card_browser::{
    CardBrowserConfig, CardBrowserResponse, CardBrowserState, CardRenderContext,
};
pub use channel_bands::{
    CHANNEL_PALETTE, ChannelBands, ChannelBandsResponse, ChannelSeries, OTHER_COLOR, OTHER_LABEL,
    assign_colors, fold_to_other, period_total,
};
pub use chip::{Chip, ChipResponse, ChipVariant};
pub use claim_card::{
    ClaimCard, ClaimSupport, FalsifierStatus, unsourced_assertions, weakest_basis,
};
pub use collection_list::{
    CollectionControl, CollectionControls, CollectionList, CollectionListAction,
    CollectionListLayout, CollectionListResponse, CollectionRow,
};
pub use command_palette::{CommandPalette, PaletteAction, PaletteState};
pub use custody_walk::{
    CustodyStrength, CustodyWalk, CustodyWalkResponse, WalkNode, WalkNodeKind, WalkSummary,
    summarize as summarize_walk,
};
pub use distribution_waterfall::{DistributionWaterfall, WaterfallMode, WaterfallParty};
pub use donut_chart::{
    DistBand, DistributionChart, format_value as format_chart_value, legend_row,
};
pub use error_note::{ErrorNote, ErrorSummary, pretty_json, summarize_error};
pub use event_wiring::{ActionCardVm, EventNodeVm, EventWiring, EventWiringResponse};
#[cfg(target_arch = "wasm32")]
pub use file_upload::{FileUploadButton, UploadedFile};
pub use flip_counter::FlipCounter;
pub use flow_ledger::{
    FlowLedger, FlowLedgerResponse, FlowRow, LedgerTotals, totals as flow_totals,
};
pub use flow_matrix::{FlowMatrix, FlowMatrixResponse, MatrixFlow};
pub use flow_ring::{FlowRing, FlowRingResponse, RingFlow, RingNode, ring_tint};
pub use flow_stave::{
    FlowStave, FlowStaveResponse, Reconciliation, StaveEvent, StaveLane, StaveOrigin,
};
pub use fungibles_row::{FungiblesRow, FungiblesRowConfig};
pub use holder_field::{AssetMove, HolderField, HolderFieldResponse};
pub use holder_formation::{
    Acquisition, Distribution, HolderFormation, distribution_at, distribution_series, holdings_at,
};
pub use icons::{PhosphorIcon, install_phosphor_font};
pub use id_pill::{
    IdPill, IdPillLayout, IdPillResponse, stacked_width_for as id_pill_stacked_width_for,
};
pub use image_loader::{AssetImageSize, iiif_asset_url};
#[cfg(feature = "image-editor")]
pub use image_text_editor::{
    FontChoice, ImageTextEditor, TextEffect, TextOverlay, TextOverlayAnchor,
};
pub use listing_grid::{ListingCard, ListingGrid, ListingGridConfig};
pub use machine::Machine;
pub use marquee::{Marquee, MarqueeConfig, MarqueeItem};
pub use metric_card::{MetricCard, Trend};
pub use mint_arrivals::{Arrival, MintArrivals, peak_pile, pile_offset, piles_at};
pub use mint_checkout::{
    BundleOffer, CheckoutState, Eligibility, MintCheckout, MintCheckoutAction,
    MintCheckoutResponse, MintCheckoutVm,
};
pub use motion::{
    Easing, forget as tween_forget, tween, tween_bool, tween_color, tween_from, tween_pos,
    tween_pos_from, tween_vec,
};
pub use named_group_list::{NamedGroup, NamedGroupList};
pub use order_list::{
    FulfilmentRow, OrderEventRow, OrderList, OrderListAction, OrderListResponse, OrderRow,
    OrderStatus,
};
pub use palette_editor::{Palette, PaletteEditor, PaletteVariant};
pub use party_annotator::{
    AnnotationDraft, PartyAnnotator, PartyAnnotatorResponse, PartyClass, basis_color,
};
pub use party_badge::{PartyBadge, PartyBasis};
pub use party_finder::{
    AliasIndex, MatchTier, PartyFinder, PartyFinderResponse, PartyFinderState, WalletIdentity,
};
pub use persona_strip::{PersonaStrip, PersonaStripConfig};
pub use phase_card::{GateChip, PhaseCard, PhaseCardAction, PhaseCardResponse, PhaseCardRow};
pub use pip_row::{
    HoverInfo, HoveredBin, HoveredPip, Pip, PipRowConfig, PipRowData, PipRowResponse,
};
pub use progress_bar::ProgressBar;
pub use property_list::{PropertyLabelAlign, PropertyList};
pub use quantity_stepper::{QuantityStepper, QuantityStepperResponse};
pub use radar_chart::{RadarChartConfig, RadarPoint};
pub use range_bar::{RangeBarConfig, RangePoint};
pub use rarity_target_editor::{RarityRow, RarityTargetEditor};
pub use relationship_editor::{RelationshipEditor, RelationshipEditorResponse};
pub use relative_time::{RelativeTime, relative_label};
pub use screenshot::ScreenshotButton;
pub use selection::{DIM as SELECTION_DIM, Selection};
pub use seven_segment::SevenSegmentDisplay;
pub use slot_table::{SlotRow, SlotTable};
pub use sparkline::{SparkHoverStyle, Sparkline};
pub use stat_strip::{StatRange, StatStrip, StatWindow};
pub use supply_bar::SupplyBar;
pub use swap_modal::{
    CultureBuy, SwapModal, SwapModalAction, SwapModalConfig, SwapModalTheme, SwapPreviewData,
    SwapProgress,
};
pub use tag_list::{TagList, TagListResponse};
pub use theme::{FontStrategy, rarity_rank_color};
pub use time_spine::{
    MarkKind, SpineState, TimeScale, TimeSpine, TimeSpineResponse, TimeView, civil_from_unix,
    compact_tick_label, format_date, next_tick_step_secs, paint_ticks,
};
pub use timestamp::{Timestamp, format_iso8601};
pub use toast::{DEFAULT_DURATION_FRAMES, Toast, ToastKind, ToastQueue, show_toasts};
pub use token_multiselect::{TokenMultiselect, TokenMultiselectResponse};
pub use trade_flow::{FlowAsset, TradeFlowConfig, TradeFlowData};
pub use trait_filter::{FilterEntry, TraitFilterConfig, TraitFilterResponse, TraitFilterState};
pub use typeahead_search::{TypeaheadOption, TypeaheadResponse, TypeaheadSearch, filter_options};
pub use utils::{
    format_ada, format_duration, format_lovelace, format_number, format_percent, section_heading,
    stat_card, truncate_hex,
};
#[cfg(all(target_arch = "wasm32", feature = "cardano"))]
pub use wallet_button::{WalletAction, WalletButton, WalletButtonTheme};
#[cfg(feature = "cardano")]
pub use wallet_editor::{
    WalletEditorAction, WalletEditorConfig, WalletEditorEntry, WalletEditorResponse,
    WalletEditorState, WalletEntryStatus,
};
pub use wallet_identity_header::{
    WalletIdentityAction, WalletIdentityConfig, WalletIdentityHeader, truncate_stake,
};
pub use wallet_list::{
    WalletList, WalletListAction, WalletListLayout, WalletListResponse, WalletListRole,
    WalletListRow, WalletPoolBadge, WalletPoolBadgeHealth,
};

// DEX split swap re-exports
pub use amount_input::{
    AmountInputAction, AmountInputConfig, AmountInputResponse, AmountInputState,
};
pub use pool_liquidity_indicator::{PoolInfo, PoolLiquidityConfig};
pub use price_impact_curve::{ImpactCurvePool, PriceImpactCurveConfig};
pub use route_summary::{RouteLeg, RouteSummaryConfig, RouteSummaryData};
pub use slippage_selector::{
    SlippagePreset, SlippageSelectorAction, SlippageSelectorConfig, SlippageSelectorState,
};
pub use split_allocation_bar::{AllocationSegment, SplitAllocationBarConfig, dex_color};

// Loan dashboard re-exports
pub use data_table::{
    DataRowItem, DataRowStatus, DataTableConfig, DataTableResponse, DataTableState,
};
pub use exposure_bar::{ExposureBarConfig, ExposureSegment, ltv_risk_color};
pub use leaderboard_table::{LeaderboardRow, LeaderboardTable};

// Cardano-specific re-exports
#[cfg(feature = "cardano")]
pub use asset_strip::{AssetStripConfig, AssetStripItem, AssetStripResponse};
pub use bullet_bar::BulletBar;
pub use coverage_delta_bar::CoverageDeltaConfig;
#[cfg(feature = "cardano")]
pub use fee_report::{FeeReportConfig, FeeReportData, SideFeeData};
#[cfg(feature = "cardano")]
pub use managed_wallet_utxos::{ManagedWalletUtxos, UtxoBreakdown, WalletShape};
#[cfg(feature = "cardano")]
pub use offer_slot::{OfferSlotAction, OfferSlotConfig, OfferSlotData, OfferSlotResponse};
#[cfg(feature = "cardano")]
pub use signing_status::{SigningAction, SigningPhase, SigningStatusConfig, SigningStatusResponse};
#[cfg(feature = "cardano")]
pub use trade_table::{
    LockState, PeerState, TradeOffer, TradeTableAction, TradeTableConfig, TradeTableResponse,
    TradeTableState,
};
#[cfg(feature = "cardano")]
pub use trait_delta::{TraitDeltaConfig, TraitItem};
#[cfg(feature = "cardano")]
pub use tx_estimate::{TxEstimateConfig, TxEstimateData, UtxoCost};
#[cfg(feature = "cardano")]
pub use utxo_map::{
    UtxoCell, UtxoMapAction, UtxoMapConfig, UtxoMapData, UtxoMapResponse, UtxoMapState,
    utxos_to_map_data,
};
#[cfg(feature = "cardano")]
pub use utxo_shelf::{
    ShelfAction, ShelfConfig, ShelfData, ShelfResponse, ShelfState, ShelfTier, ShelfUtxo,
    classify_utxos,
};
#[cfg(feature = "cardano")]
pub use wallet_asset_picker::{
    PickerAsset, PickerPolicyGroup, SelectedAsset, WalletAssetPickerAction,
    WalletAssetPickerConfig, WalletAssetPickerResponse, WalletAssetPickerState,
};
