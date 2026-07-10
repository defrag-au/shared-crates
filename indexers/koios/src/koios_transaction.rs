use crate::koios_assets::KoiosAssetList;
use crate::koios_serde::{as_u64, opt_as_u64};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct KoiosTransaction {
    pub tx_hash: String,
    pub block_hash: String,

    #[serde(deserialize_with = "as_u64")]
    pub block_height: u64,

    pub epoch_no: u32,
    pub epoch_slot: u32,

    #[serde(deserialize_with = "as_u64")]
    pub absolute_slot: u64,

    #[serde(deserialize_with = "as_u64")]
    pub tx_timestamp: u64,

    #[serde(deserialize_with = "as_u64")]
    pub tx_block_index: u64,

    #[serde(deserialize_with = "as_u64")]
    pub tx_size: u64,

    pub total_output: String,
    pub fee: String,
    pub treasury_donation: String,
    pub deposit: String,

    #[serde(default, deserialize_with = "opt_as_u64")]
    pub invalid_before: Option<u64>,
    #[serde(default, deserialize_with = "opt_as_u64")]
    pub invalid_after: Option<u64>,

    #[serde(default)]
    pub collateral_inputs: Vec<KoisUtxo>,
    pub collateral_output: Option<KoisUtxo>,
    #[serde(default)]
    pub reference_inputs: Vec<KoisUtxo>,
    #[serde(default)]
    pub inputs: Vec<KoisUtxo>,
    #[serde(default)]
    pub outputs: Vec<KoisUtxo>,
    #[serde(default)]
    pub withdrawals: Vec<KoiosWithdrawal>,
    #[serde(default)]
    pub assets_minted: Vec<KoiosMintedAsset>,
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub certificates: Vec<serde_json::Value>,
    #[serde(default)]
    pub native_scripts: Vec<serde_json::Value>,
    #[serde(default)]
    pub plutus_contracts: Vec<serde_json::Value>,
    #[serde(default)]
    pub voting_procedures: Vec<serde_json::Value>,
    #[serde(default)]
    pub proposal_procedures: Vec<serde_json::Value>,
    #[serde(default)]
    pub scripts: Vec<serde_json::Value>,
    #[serde(default)]
    pub datum: Vec<serde_json::Value>,
    #[serde(default)]
    pub datum_hashes: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KoisUtxo {
    pub value: String,
    pub tx_hash: String,
    pub tx_index: u32,
    #[serde(default)]
    pub asset_list: KoiosAssetList,
    pub datum_hash: Option<String>,
    pub stake_addr: Option<String>,
    pub inline_datum: Option<KoiosInlineDatum>,
    pub payment_addr: KoiosPaymentAddr,
    pub reference_script: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KoiosInlineDatum {
    pub bytes: Option<String>,
    pub value: Option<KoiosPlutusData>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KoiosPlutusData {
    pub constructor: u32,
    pub fields: Vec<KoiosPlutusField>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum KoiosPlutusField {
    Int { int: i64 },
    BytesList { list: Vec<KoiosBytes> },
    BytesListDeprecated { bytes: String },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KoiosBytes {
    pub bytes: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KoiosPaymentAddr {
    pub cred: String,
    pub bech32: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KoiosWithdrawal {
    pub stake_address: String,
    pub amount: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KoiosMintedAsset {
    pub policy_id: String,
    pub asset_name: String,
    pub quantity: String,
}
