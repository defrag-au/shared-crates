//! Drop type combining different intent types for rewards/prizes

use crate::{TipIntent, TransferIntent};
use cardano_assets::AssetId;
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// A reward drop - can be delivered via different mechanisms
///
/// Used for raffle prizes, achievement rewards, giveaways, etc.
///
/// # Example
///
/// ```
/// use asset_intents::{Drop, TipIntent, TransferIntent};
/// use cardano_assets::AssetId;
///
/// // A tip drop
/// let tip_drop = Drop::tip("ADA", 100.0);
///
/// // An NFT transfer drop
/// let asset_id = AssetId::new_unchecked(
///     "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string(),
///     "50697261746531303836".to_string(),
/// );
/// let nft_drop = Drop::transfer(asset_id, 1);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum Drop {
    /// Fungible token tip via tipping service (e.g., FarmBot ctip)
    Tip(TipIntent),
    /// Direct asset transfer via wallet service (e.g., cnft.dev)
    Transfer(TransferIntent),
}

impl Drop {
    /// Create a new tip drop
    pub fn tip(token: impl Into<String>, amount: f64) -> Self {
        Drop::Tip(TipIntent::new(token, amount))
    }

    /// Create a new transfer drop
    pub fn transfer(asset_id: AssetId, amount: u64) -> Self {
        Drop::Transfer(TransferIntent::new(asset_id, amount))
    }

    /// Create a transfer drop for a single NFT
    pub fn transfer_single(asset_id: AssetId) -> Self {
        Drop::Transfer(TransferIntent::single(asset_id))
    }

    /// Get a human-readable description of the drop
    pub fn description(&self) -> String {
        match self {
            Drop::Tip(tip) => tip.description(),
            Drop::Transfer(transfer) => transfer.description(),
        }
    }

    /// Returns true if this is a tip drop
    pub fn is_tip(&self) -> bool {
        matches!(self, Drop::Tip(_))
    }

    /// Returns true if this is a transfer drop
    pub fn is_transfer(&self) -> bool {
        matches!(self, Drop::Transfer(_))
    }

    /// Get the tip intent if this is a tip drop
    pub fn as_tip(&self) -> Option<&TipIntent> {
        match self {
            Drop::Tip(tip) => Some(tip),
            _ => None,
        }
    }

    /// Get the transfer intent if this is a transfer drop
    pub fn as_transfer(&self) -> Option<&TransferIntent> {
        match self {
            Drop::Transfer(transfer) => Some(transfer),
            _ => None,
        }
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
    fn test_tip_drop() {
        let drop = Drop::tip("ADA", 100.0);
        assert!(drop.is_tip());
        assert!(!drop.is_transfer());
        assert_eq!(drop.description(), "100 ADA");
    }

    #[test]
    fn test_transfer_drop() {
        let drop = Drop::transfer(test_asset_id(), 1);
        assert!(drop.is_transfer());
        assert!(!drop.is_tip());
    }

    #[test]
    fn test_serialization() {
        let tip = Drop::tip("CARN", 50.0);
        let json = serde_json::to_string(&tip).unwrap();
        assert!(json.contains("\"type\":\"tip\""));

        let transfer = Drop::transfer_single(test_asset_id());
        let json = serde_json::to_string(&transfer).unwrap();
        assert!(json.contains("\"type\":\"transfer\""));
    }

    #[test]
    fn test_deserialization() {
        let json = r#"{"type":"tip","token":"ADA","amount":100.0}"#;
        let drop: Drop = serde_json::from_str(json).unwrap();
        assert!(drop.is_tip());
        assert_eq!(drop.as_tip().unwrap().amount, 100.0);
    }
}
