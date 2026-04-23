use tx_insights::TxAsset;

use crate::PricedAsset;

impl From<PricedAsset> for TxAsset {
    fn from(value: PricedAsset) -> Self {
        value.asset.into()
    }
}
