//! Transaction insights types for classification and notification sharing
//! between cnft-dev workers and augminted bots workers.

use std::collections::HashMap;

pub use serde::{Deserialize, Serialize};
pub use wasm_safe_serde;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnalysedTx {
    pub hash: String,
    pub insights: Vec<TxInsight>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TxInsight {
    Mint {
        assets: Vec<TxAsset>,
    },
    OfferCreate {
        policy_id: String,
        seller: String,
        offer_type: TxOfferType,
        #[serde(with = "wasm_safe_serde::u64_required")]
        price_lovelace: u64,
    },
    Listing {
        asset: TxAsset,
        action: ListingAction,
        seller: String,
        #[serde(with = "wasm_safe_serde::u64_required")]
        price_lovelace: u64,
    },
    Sale {
        asset: TxAsset,
        kind: AssetSaleKind,
        seller: String,
        buyer: String,
        #[serde(with = "wasm_safe_serde::u64_required")]
        price_lovelace: u64,
    },
    DexTrade {
        asset: TxAsset,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxAsset {
    pub id: String, // policy_id + asset_hex concatenated
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub qty: u64,
    #[serde(default)]
    pub traits: Option<HashMap<String, Vec<String>>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TxOfferType {
    Collection,
    Asset { asset_hex: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ListingAction {
    Create,
    Update,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum AssetSaleKind {
    Standard,
    AcceptOffer,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_large_price_serialization() {
        let large_price = 15_000_000_000_000_000_u64; // > MAX_SAFE_JS_INTEGER (9007199254740991)

        let insight = TxInsight::Sale {
            asset: TxAsset {
                id: "policy123.asset456".to_string(),
                qty: 1,
                traits: None,
            },
            kind: AssetSaleKind::Standard,
            seller: "addr1seller".to_string(),
            buyer: "addr1buyer".to_string(),
            price_lovelace: large_price,
        };

        let json = serde_json::to_string(&insight).expect("Should serialize");

        // Large price should be serialized as string
        assert!(json.contains(&format!("\"{}\"", large_price)));

        // Should deserialize back correctly
        let deserialized: TxInsight = serde_json::from_str(&json).expect("Should deserialize");
        if let TxInsight::Sale { price_lovelace, .. } = deserialized {
            assert_eq!(price_lovelace, large_price);
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_analyzed_tx_serialization() {
        let tx = AnalysedTx {
            hash: "tx123".to_string(),
            insights: vec![TxInsight::Mint {
                assets: vec![TxAsset {
                    id: "policy.asset".to_string(),
                    qty: 1000,
                    traits: Some(HashMap::from([(
                        "color".to_string(),
                        vec!["red".to_string(), "blue".to_string()],
                    )])),
                }],
            }],
        };

        let json = serde_json::to_string(&tx).expect("Should serialize");
        let _deserialized: AnalysedTx = serde_json::from_str(&json).expect("Should deserialize");
    }
}
