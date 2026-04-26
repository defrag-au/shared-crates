use tx_insights::{AssetSaleKind, TxInsight};

use crate::{TxClassification, TxType};

impl From<TxType> for Option<TxInsight> {
    fn from(value: TxType) -> Self {
        match value {
            TxType::Mint { assets, .. } => Some(TxInsight::Mint {
                assets: assets.into_iter().map(|a| a.into()).collect(),
            }),
            TxType::Sale {
                asset,
                seller,
                buyer,
                ..
            } => Some(TxInsight::Sale {
                asset: asset.clone().into(),
                kind: AssetSaleKind::Standard,
                seller,
                buyer,
                price_lovelace: asset.price_lovelace.unwrap_or_default(),
            }),
            _ => None,
        }
    }
}

impl From<TxClassification> for Vec<TxInsight> {
    fn from(classification: TxClassification) -> Self {
        classification
            .tx_types
            .into_iter()
            .filter_map(|tx_type| tx_type.into())
            .collect()
    }
}
