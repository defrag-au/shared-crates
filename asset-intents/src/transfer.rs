//! Transfer intent for direct asset transfers

use cardano_assets::AssetId;
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Intent to transfer assets directly to a wallet
///
/// Used for NFT transfers, token sends, etc. via services like cnft.dev
///
/// # Example
///
/// ```
/// use asset_intents::TransferIntent;
/// use cardano_assets::AssetId;
///
/// let asset_id = AssetId::new_unchecked(
///     "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string(),
///     "50697261746531303836".to_string(),
/// );
///
/// // Transfer 1 NFT
/// let transfer = TransferIntent::new(asset_id.clone(), 1);
///
/// // Transfer 100 fungible tokens
/// let transfer = TransferIntent::new(asset_id, 100);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TransferIntent {
    /// The asset to transfer
    pub asset_id: AssetId,
    /// Quantity to transfer
    pub amount: u64,
}

impl TransferIntent {
    /// Create a new transfer intent
    pub fn new(asset_id: AssetId, amount: u64) -> Self {
        Self { asset_id, amount }
    }

    /// Create a transfer intent for a single NFT
    pub fn single(asset_id: AssetId) -> Self {
        Self::new(asset_id, 1)
    }

    /// Get a human-readable description (e.g., "1 x policy:asset")
    pub fn description(&self) -> String {
        format!("{} x {}", self.amount, self.asset_id.delimited(":"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_asset_id() -> AssetId {
        AssetId::new_unchecked(
            "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string(),
            "50697261746531303836".to_string(),
        )
    }

    #[test]
    fn test_transfer_intent_creation() {
        let transfer = TransferIntent::new(test_asset_id(), 5);
        assert_eq!(transfer.amount, 5);
    }

    #[test]
    fn test_single_nft_transfer() {
        let transfer = TransferIntent::single(test_asset_id());
        assert_eq!(transfer.amount, 1);
    }

    #[test]
    fn test_transfer_description() {
        let transfer = TransferIntent::new(test_asset_id(), 3);
        assert!(transfer.description().contains("3 x"));
    }
}
