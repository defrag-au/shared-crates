//! Cardano Marketplace Abstraction
//! 
//! This crate provides a normalized abstraction layer over Cardano NFT marketplaces,
//! currently wrapping the Anvil API to provide bandwidth-efficient operations with
//! deduplicated collection metadata.
//! 
//! ## Features
//! 
//! - Get collection details without redundant asset data
//! - Retrieve floor price information with marketplace distribution
//! - Filter assets by traits for targeted floor price queries
//! - Normalized type system for cross-marketplace compatibility
//! 
//! ## Example
//! 
//! ```rust,no_run
//! use cardano_marketplace::{MarketplaceClient, TraitFilter};
//! 
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = MarketplaceClient::from_env()?;
//!     let policy_id = "d5e6bf0500378d4f0da4e8dde6becec7621cd8cbf5cbb9b87013d4cc";
//!     
//!     // Get collection details
//!     let collection = client.get_collection_details(policy_id).await?;
//!     println!("Collection: {}", collection.name);
//!     
//!     // Get floor price
//!     let floor = client.get_floor_price(policy_id).await?;
//!     println!("Floor price: {} ADA", floor.price as f64 / 1_000_000.0);
//!     
//!     // Get filtered floor price
//!     let trait_filter = TraitFilter::new()
//!         .add_single_trait("Background".to_string(), "Blue".to_string());
//!     let filtered_floor = client.get_floor_price_filtered(policy_id, &trait_filter).await?;
//!     println!("Filtered floor price: {} ADA", filtered_floor.price as f64 / 1_000_000.0);
//!     
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod types;

pub use client::MarketplaceClient;
pub use types::*;