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
pub mod utxo_map;
pub mod utxo_shelf;
#[cfg(target_arch = "wasm32")]
pub mod wallet;
#[cfg(target_arch = "wasm32")]
pub mod wallet_button;
pub mod wallet_editor;

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

// Trade desk widgets
pub mod asset_strip;
pub mod coverage_delta_bar;
pub mod fee_report;
pub mod offer_slot;
pub mod signing_status;
pub mod trade_table;
pub mod trait_delta;
pub mod tx_estimate;
pub mod wallet_asset_picker;

pub use animated_counter::AnimatedCounter;
pub use buttons::UiButtonExt;
pub use card_browser::{
    CardBrowserConfig, CardBrowserResponse, CardBrowserState, CardRenderContext,
};
pub use donut_chart::{
    format_value as format_chart_value, legend_row, DistBand, DistributionChart,
};
pub use flip_counter::FlipCounter;
pub use icons::{install_phosphor_font, PhosphorIcon};
pub use image_loader::{iiif_asset_url, AssetImageSize};
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
#[cfg(target_arch = "wasm32")]
pub use file_upload::{FileUploadButton, UploadedFile};
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
pub use utxo_map::{
    utxos_to_map_data, UtxoCell, UtxoMapAction, UtxoMapConfig, UtxoMapData, UtxoMapResponse,
    UtxoMapState,
};
pub use utxo_shelf::{
    classify_utxos, ShelfAction, ShelfConfig, ShelfData, ShelfResponse, ShelfState, ShelfTier,
    ShelfUtxo,
};
#[cfg(target_arch = "wasm32")]
pub use wallet_button::{WalletAction, WalletButton, WalletButtonTheme};
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

// Trade desk re-exports
pub use asset_strip::{AssetStripConfig, AssetStripItem, AssetStripResponse};
pub use coverage_delta_bar::CoverageDeltaConfig;
pub use fee_report::{FeeReportConfig, FeeReportData, SideFeeData};
pub use offer_slot::{OfferSlotAction, OfferSlotConfig, OfferSlotData, OfferSlotResponse};
pub use signing_status::{SigningAction, SigningPhase, SigningStatusConfig, SigningStatusResponse};
pub use trade_table::{
    LockState, PeerState, TradeOffer, TradeTableAction, TradeTableConfig, TradeTableResponse,
    TradeTableState,
};
pub use trait_delta::{TraitDeltaConfig, TraitItem};
pub use tx_estimate::{TxEstimateConfig, TxEstimateData, UtxoCost};
pub use wallet_asset_picker::{
    PickerAsset, PickerPolicyGroup, SelectedAsset, WalletAssetPickerAction,
    WalletAssetPickerConfig, WalletAssetPickerResponse, WalletAssetPickerState,
};
