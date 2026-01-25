//! Asset Intents
//!
//! Intent types for asset operations: tips, transfers, and drops.
//!
//! This crate provides the core types for expressing intentions to move assets:
//!
//! - [`TipIntent`] - Fungible token tips via tipping services (e.g., FarmBot ctip)
//! - [`TransferIntent`] - Direct asset transfers via wallet services (e.g., cnft.dev)
//! - [`Drop`] - A reward/prize that can be either a tip or wallet send
//!
//! # Example
//!
//! ```
//! use asset_intents::{Drop, TipIntent, TransferIntent};
//! use cardano_assets::AssetId;
//!
//! // Create a tip for 100 ADA
//! let tip = TipIntent::new("ADA", 100.0);
//!
//! // Create an NFT transfer
//! let asset_id = AssetId::new_unchecked(
//!     "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string(),
//!     "50697261746531303836".to_string(),
//! );
//! let transfer = TransferIntent::single(asset_id.clone());
//!
//! // Use the Drop enum for reward configurations
//! let drops = vec![
//!     Drop::tip("ADA", 500.0),              // 1st prize: 500 ADA tip
//!     Drop::tip("ADA", 250.0),              // 2nd prize: 250 ADA tip
//!     Drop::wallet_send_single(asset_id),   // 3rd prize: NFT transfer
//! ];
//! ```

mod drop;
mod tip;
mod token_amount;
mod transfer;

pub use drop::Drop;
pub use tip::TipIntent;
pub use token_amount::{format_number, TokenAmount};
pub use transfer::TransferIntent;

// Re-export AssetId for convenience
pub use cardano_assets::AssetId;
