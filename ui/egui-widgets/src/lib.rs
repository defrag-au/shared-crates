pub use egui_inbox;

pub mod animated_counter;
pub mod asset_card;
pub mod buttons;
pub mod card_browser;
pub mod donut_chart;
#[cfg(target_arch = "wasm32")]
pub mod file_upload;
pub mod flip_counter;
pub mod icons;
pub mod image_loader;
pub mod listing_grid;
pub mod marquee;
pub mod metric_card;
pub mod pip_row;
pub mod printing_timeline;
pub mod progress_bar;
pub mod radar_chart;
pub mod range_bar;
pub mod screenshot;
pub mod seven_segment;
pub mod sparkline;
pub mod swap_modal;
pub mod theme;
pub mod trait_filter;
pub mod utils;
#[cfg(all(target_arch = "wasm32", feature = "cardano"))]
pub mod wallet;
#[cfg(all(target_arch = "wasm32", feature = "cardano"))]
pub mod wallet_button;
#[cfg(feature = "cardano")]
pub mod wallet_editor;

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

// Cardano-specific widgets (feature-gated)
#[cfg(feature = "cardano")]
pub mod asset_strip;
#[cfg(feature = "cardano")]
pub mod coverage_delta_bar;
#[cfg(feature = "cardano")]
pub mod fee_report;
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
pub use buttons::UiButtonExt;
pub use card_browser::{
    CardBrowserConfig, CardBrowserResponse, CardBrowserState, CardRenderContext,
};
pub use donut_chart::{
    format_value as format_chart_value, legend_row, DistBand, DistributionChart,
};
#[cfg(target_arch = "wasm32")]
pub use file_upload::{FileUploadButton, UploadedFile};
pub use flip_counter::FlipCounter;
pub use icons::{install_phosphor_font, PhosphorIcon};
pub use image_loader::{iiif_asset_url, AssetImageSize};
#[cfg(feature = "image-editor")]
pub use image_text_editor::{
    FontChoice, ImageTextEditor, TextEffect, TextOverlay, TextOverlayAnchor,
};
pub use listing_grid::{ListingCard, ListingGrid, ListingGridConfig};
pub use marquee::{Marquee, MarqueeConfig, MarqueeItem};
pub use metric_card::{MetricCard, Trend};
pub use pip_row::{
    HoverInfo, HoveredBin, HoveredPip, Pip, PipRowConfig, PipRowData, PipRowResponse,
};
pub use progress_bar::ProgressBar;
pub use radar_chart::{RadarChartConfig, RadarPoint};
pub use range_bar::{RangeBarConfig, RangePoint};
pub use screenshot::ScreenshotButton;
pub use seven_segment::SevenSegmentDisplay;
pub use sparkline::Sparkline;
pub use swap_modal::{
    CultureBuy, SwapModal, SwapModalAction, SwapModalConfig, SwapModalTheme, SwapPreviewData,
    SwapProgress,
};
pub use theme::{rarity_rank_color, FontStrategy};
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
#[cfg(feature = "cardano")]
pub use coverage_delta_bar::CoverageDeltaConfig;
#[cfg(feature = "cardano")]
pub use fee_report::{FeeReportConfig, FeeReportData, SideFeeData};
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
