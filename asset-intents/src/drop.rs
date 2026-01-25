//! Drop type combining different intent types for rewards/prizes

use cardano_assets::AssetId;
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// A reward drop - can be delivered via different mechanisms
///
/// Used for raffle prizes, achievement rewards, giveaways, etc.
///
/// Serializes with flattened fields:
/// - `{"type": "tip", "token": "ADA", "amount": 100.0}`
/// - `{"type": "wallet_send", "asset_id": {...}, "amount": 1}`
///
/// # Example
///
/// ```
/// use asset_intents::Drop;
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
/// let nft_drop = Drop::wallet_send(asset_id, 1);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum Drop {
    /// Fungible token tip via tipping service (e.g., FarmBot ctip)
    Tip {
        /// Token identifier (e.g., "ADA", "CARN", "GOLD")
        token: String,
        /// Amount to tip
        amount: f64,
    },
    /// Direct asset transfer via wallet service (e.g., cnft.dev)
    WalletSend {
        /// The asset to transfer
        asset_id: AssetId,
        /// Quantity to transfer
        amount: u64,
    },
}

impl Drop {
    /// Create a new tip drop
    pub fn tip(token: impl Into<String>, amount: f64) -> Self {
        Drop::Tip {
            token: token.into(),
            amount,
        }
    }

    /// Create a new wallet send drop
    pub fn wallet_send(asset_id: AssetId, amount: u64) -> Self {
        Drop::WalletSend { asset_id, amount }
    }

    /// Create a wallet send drop for a single NFT
    pub fn wallet_send_single(asset_id: AssetId) -> Self {
        Drop::WalletSend {
            asset_id,
            amount: 1,
        }
    }

    /// Get a human-readable description of the drop
    pub fn description(&self) -> String {
        match self {
            Drop::Tip { token, amount } => format!("{} {}", amount, token),
            Drop::WalletSend { asset_id, amount } => {
                format!("{} x {}", amount, asset_id.delimited(":"))
            }
        }
    }

    /// Returns true if this is a tip drop
    pub fn is_tip(&self) -> bool {
        matches!(self, Drop::Tip { .. })
    }

    /// Returns true if this is a wallet send drop
    pub fn is_wallet_send(&self) -> bool {
        matches!(self, Drop::WalletSend { .. })
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
        assert!(!drop.is_wallet_send());
        assert_eq!(drop.description(), "100 ADA");
    }

    #[test]
    fn test_wallet_send_drop() {
        let drop = Drop::wallet_send(test_asset_id(), 1);
        assert!(drop.is_wallet_send());
        assert!(!drop.is_tip());
    }

    #[test]
    fn test_serialization() {
        let tip = Drop::tip("CARN", 50.0);
        let json = serde_json::to_string(&tip).unwrap();
        assert!(json.contains("\"type\":\"tip\""));
        assert!(json.contains("\"token\":\"CARN\""));
        assert!(json.contains("\"amount\":50.0"));

        let transfer = Drop::wallet_send_single(test_asset_id());
        let json = serde_json::to_string(&transfer).unwrap();
        assert!(json.contains("\"type\":\"wallet_send\""));
        assert!(json.contains("\"amount\":1"));
    }

    #[test]
    fn test_deserialization() {
        let json = r#"{"type":"tip","token":"ADA","amount":100.0}"#;
        let drop: Drop = serde_json::from_str(json).unwrap();
        assert!(drop.is_tip());
        if let Drop::Tip { token, amount } = drop {
            assert_eq!(token, "ADA");
            assert_eq!(amount, 100.0);
        }
    }
}
