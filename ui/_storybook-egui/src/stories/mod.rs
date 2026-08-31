pub mod about_modal;
pub mod access_gate;
pub mod activity_feed;
pub mod asset_card;
pub mod async_data;
pub mod background;
pub mod bullet_bar;
pub mod buttons;
pub mod cap_band;
pub mod capital_flow;
pub mod card_browser;
pub mod collection_list;
pub mod command_palette;
pub mod coverage_lanes;
pub mod distribution;
pub mod distribution_waterfall;
pub mod error_note;
pub mod event_wiring;
pub mod flip_counter;
pub mod flow_ledger;
pub mod flow_matrix;
pub mod flow_ring;
pub mod flow_stave;
pub mod focus_list;
pub mod formatting;
pub mod gated;
pub mod icon_gallery;
pub mod leaderboard;
pub mod machine;
pub mod marquee;
pub mod mesh_playground;
pub mod metric_card;
pub mod mnemonic_display;
pub mod named_group_list;
pub mod order_list;
pub mod palette_editor;
pub mod pane_nav;
pub mod party_annotator;
pub mod party_badge;
pub mod perspective_text;
pub mod pip_row;
pub mod price_timeline;
pub mod printing_timeline;
pub mod progress_bar;
pub mod radar_chart;
pub mod range_bar;
pub mod rarity_target_editor;
pub mod relationship_editor;
pub mod service_banner;
pub mod seven_segment;
pub mod slot_table;
pub mod sparkline;
pub mod stat_strip;
pub mod supply_bar;
pub mod swap;
pub mod tag_list;
pub mod tcg_card;
pub mod tier_ladder;
pub mod time_spine;
pub mod timestamp;
pub mod token_multiselect;
pub mod trait_filter;
pub mod typeahead_search;
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
pub mod custody_walk;
pub mod data_table;
pub mod exposure_bar;

// Ranked-list dashboards
pub mod leaderboard_table;

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
pub mod trade_flow;
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
pub mod channel_bands;
pub mod chip;
pub mod claim_card;
pub mod id_pill;
pub mod property_list;
pub mod theme_states;
pub mod toast;

// Layout
pub mod grouped_section;
pub mod offer_tile;

// Mint configuration
pub mod mint_checkout;
pub mod phase_card;
pub mod quantity_stepper;

// Wallet
pub mod collection_composition;
pub mod fungibles_row;
pub mod persona_strip;
pub mod user_badge;
pub mod variant_split;
pub mod wallet_identity_header;
pub mod wallet_list;
