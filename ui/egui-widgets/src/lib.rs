#[cfg(target_arch = "wasm32")]
pub use egui_inbox;

pub mod animated_counter;
pub mod asset_card;
pub mod bullet_bar;
pub mod button_group;
pub mod buttons;
pub mod card_browser;
pub mod chip;
pub mod collection_list;
pub mod distribution_waterfall;
pub mod donut_chart;
pub mod error_note;
#[cfg(target_arch = "wasm32")]
pub mod file_upload;
pub mod flip_counter;
pub mod fungibles_row;
pub mod grouped_section;
pub mod icons;
pub mod id_pill;
pub mod image_loader;
pub mod listing_grid;
pub mod marquee;
pub mod metric_card;
pub mod mint_checkout;
pub mod mnemonic_display;
pub mod named_group_list;
pub mod offer_tile;
pub mod order_list;
pub mod palette_editor;
pub mod persona_strip;
pub mod phase_card;
pub mod pip_row;
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
pub mod seven_segment;
pub mod slot_table;
pub mod sparkline;
pub mod supply_bar;
pub mod swap_modal;
pub mod tag_list;
pub mod theme;
pub mod timestamp;
pub mod toast;
pub mod token_multiselect;
pub mod trait_filter;
pub mod utils;
#[cfg(all(target_arch = "wasm32", feature = "cardano"))]
pub mod wallet;
#[cfg(all(target_arch = "wasm32", feature = "cardano"))]
pub mod wallet_button;
#[cfg(feature = "cardano")]
pub mod wallet_editor;
pub mod wallet_identity_header;
pub mod wallet_list;

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
pub mod data_table;
pub mod exposure_bar;

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

pub use animated_counter::AnimatedCounter;
pub use button_group::{ButtonGroup, ButtonGroupButton, ButtonGroupResponse};
pub use buttons::UiButtonExt;
pub use card_browser::{
    CardBrowserConfig, CardBrowserResponse, CardBrowserState, CardRenderContext,
};
pub use chip::{Chip, ChipResponse, ChipVariant};
pub use collection_list::{
    CollectionControl, CollectionControls, CollectionList, CollectionListAction,
    CollectionListLayout, CollectionListResponse, CollectionRow,
};
pub use distribution_waterfall::{DistributionWaterfall, WaterfallMode, WaterfallParty};
pub use donut_chart::{
    format_value as format_chart_value, legend_row, DistBand, DistributionChart,
};
pub use error_note::{pretty_json, summarize_error, ErrorNote, ErrorSummary};
#[cfg(target_arch = "wasm32")]
pub use file_upload::{FileUploadButton, UploadedFile};
pub use flip_counter::FlipCounter;
pub use fungibles_row::{FungiblesRow, FungiblesRowConfig};
pub use icons::{install_phosphor_font, PhosphorIcon};
pub use id_pill::{
    stacked_width_for as id_pill_stacked_width_for, IdPill, IdPillLayout, IdPillResponse,
};
pub use image_loader::{iiif_asset_url, AssetImageSize};
#[cfg(feature = "image-editor")]
pub use image_text_editor::{
    FontChoice, ImageTextEditor, TextEffect, TextOverlay, TextOverlayAnchor,
};
pub use listing_grid::{ListingCard, ListingGrid, ListingGridConfig};
pub use marquee::{Marquee, MarqueeConfig, MarqueeItem};
pub use metric_card::{MetricCard, Trend};
pub use mint_checkout::{
    BundleOffer, CheckoutState, Eligibility, MintCheckout, MintCheckoutAction,
    MintCheckoutResponse, MintCheckoutVm,
};
pub use named_group_list::{NamedGroup, NamedGroupList};
pub use order_list::{
    FulfilmentRow, OrderEventRow, OrderList, OrderListAction, OrderListResponse, OrderRow,
    OrderStatus,
};
pub use palette_editor::{Palette, PaletteEditor, PaletteVariant};
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
pub use relative_time::{relative_label, RelativeTime};
pub use screenshot::ScreenshotButton;
pub use seven_segment::SevenSegmentDisplay;
pub use slot_table::{SlotRow, SlotTable};
pub use sparkline::Sparkline;
pub use supply_bar::SupplyBar;
pub use swap_modal::{
    CultureBuy, SwapModal, SwapModalAction, SwapModalConfig, SwapModalTheme, SwapPreviewData,
    SwapProgress,
};
pub use tag_list::{TagList, TagListResponse};
pub use theme::{rarity_rank_color, FontStrategy};
pub use timestamp::{format_iso8601, Timestamp};
pub use toast::{show_toasts, Toast, ToastKind, ToastQueue, DEFAULT_DURATION_FRAMES};
pub use token_multiselect::{TokenMultiselect, TokenMultiselectResponse};
pub use trait_filter::{FilterEntry, TraitFilterConfig, TraitFilterResponse, TraitFilterState};
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
    truncate_stake, WalletIdentityAction, WalletIdentityConfig, WalletIdentityHeader,
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
pub use split_allocation_bar::{dex_color, AllocationSegment, SplitAllocationBarConfig};

// Loan dashboard re-exports
pub use data_table::{
    DataRowItem, DataRowStatus, DataTableConfig, DataTableResponse, DataTableState,
};
pub use exposure_bar::{ltv_risk_color, ExposureBarConfig, ExposureSegment};

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
    utxos_to_map_data, UtxoCell, UtxoMapAction, UtxoMapConfig, UtxoMapData, UtxoMapResponse,
    UtxoMapState,
};
#[cfg(feature = "cardano")]
pub use utxo_shelf::{
    classify_utxos, ShelfAction, ShelfConfig, ShelfData, ShelfResponse, ShelfState, ShelfTier,
    ShelfUtxo,
};
#[cfg(feature = "cardano")]
pub use wallet_asset_picker::{
    PickerAsset, PickerPolicyGroup, SelectedAsset, WalletAssetPickerAction,
    WalletAssetPickerConfig, WalletAssetPickerResponse, WalletAssetPickerState,
};
