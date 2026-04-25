use serde::{Deserialize, Serialize};

use crate::TxAsset;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScreenedTx {
    pub hash: String,
    pub signals: Vec<ScreenedTxSignal>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ScreenedTxSignal {
    Mint { assets: Vec<TxAsset> },
    Marketplace { addresses: Vec<String> },
    Dex { addresses: Vec<String> },
    MonitoredPolicy { policy_id: String },
}
