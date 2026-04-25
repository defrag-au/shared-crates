pub mod asset_card;
pub mod async_data;
pub mod buttons;
pub mod card_browser;
pub mod distribution;
pub mod flip_counter;
pub mod formatting;
pub mod icon_gallery;
pub mod marquee;
pub mod mesh_playground;
pub mod metric_card;
pub mod perspective_text;
pub mod pip_row;
pub mod printing_timeline;
pub mod progress_bar;
pub mod radar_chart;
pub mod range_bar;
pub mod seven_segment;
pub mod sparkline;
pub mod swap;
pub mod tcg_card;
pub mod trait_filter;
#[cfg(target_arch = "wasm32")]
pub mod wallet;
pub mod wallet_editor;

// DEX split swap widgets
pub mod amount_input;
pub mod pool_liquidity;
pub mod price_impact_curve;
pub mod route_summary;
pub mod slippage_selector;
pub mod split_allocation_bar;

// Loan dashboard widgets
pub mod data_table;
pub mod exposure_bar;

// Utility widgets
#[cfg(target_arch = "wasm32")]
pub mod file_upload;
pub mod image_text_editor;

// Trade desk widgets
pub mod asset_strip;
pub mod coverage_delta_bar;
pub mod fee_report;
pub mod signing_status;
pub mod trade_table;
pub mod trait_delta;
pub mod tx_estimate;
pub mod utxo_map;
pub mod wallet_asset_picker;

// TX cart
pub mod tx_cart;
