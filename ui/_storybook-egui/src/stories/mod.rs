pub mod asset_card;
pub mod async_data;
pub mod bullet_bar;
pub mod buttons;
pub mod card_browser;
pub mod collection_list;
pub mod distribution;
pub mod distribution_waterfall;
pub mod error_note;
pub mod flip_counter;
pub mod formatting;
pub mod icon_gallery;
pub mod marquee;
pub mod mesh_playground;
pub mod metric_card;
pub mod mnemonic_display;
pub mod order_list;
pub mod perspective_text;
pub mod pip_row;
pub mod printing_timeline;
pub mod progress_bar;
pub mod radar_chart;
pub mod range_bar;
pub mod seven_segment;
pub mod sparkline;
pub mod supply_bar;
pub mod named_group_list;
pub mod rarity_target_editor;
pub mod relationship_editor;
pub mod swap;
pub mod tag_list;
pub mod tcg_card;
pub mod token_multiselect;
pub mod timestamp;
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
pub mod managed_wallet_utxos;
pub mod signing_status;
pub mod trade_table;
pub mod trait_delta;
pub mod tx_estimate;
pub mod utxo_map;
pub mod wallet_asset_picker;

// TX cart
pub mod tx_cart;

// Primitives — foundational composables (semantic chips, ID displays,
// label/value grids, button groups). Add new foundation widgets to this
// group rather than tacking on a new category at the end of the file.
pub mod button_group;
pub mod chip;
pub mod id_pill;
pub mod property_list;
pub mod toast;

// Layout
pub mod grouped_section;
pub mod offer_tile;

// Mint configuration
pub mod mint_checkout;
pub mod phase_card;
pub mod quantity_stepper;

// Wallet
pub mod fungibles_row;
pub mod persona_strip;
pub mod wallet_identity_header;
pub mod wallet_list;
