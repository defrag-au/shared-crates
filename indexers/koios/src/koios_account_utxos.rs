use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct TxRecord {
    pub tx_hash: String,
    pub tx_index: u32,
    pub address: String,
    pub value: String,
    pub stake_address: String,
    pub payment_cred: String,
    pub epoch_no: u32,
    pub block_height: u64,
    pub block_time: u64,
    pub datum_hash: Option<String>,
    pub inline_datum: Option<InlineDatum>,
    pub reference_script: Option<String>,
    pub asset_list: Vec<Asset>,
    pub is_spent: bool,
}

#[derive(Debug, Deserialize)]
pub struct InlineDatum {
    pub bytes: String,
    pub value: DatumValue,
}

#[derive(Debug, Deserialize)]
pub struct DatumValue {
    pub fields: Vec<DatumField>,
    pub constructor: u8,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum DatumField {
    Int { int: u64 },
    List { list: Vec<DatumBytes> },
}

#[derive(Debug, Deserialize)]
pub struct DatumBytes {
    pub bytes: String,
}

#[derive(Debug, Deserialize)]
pub struct Asset {
    pub decimals: u8,
    pub quantity: String,
    pub policy_id: String,
    pub asset_name: String,
    pub fingerprint: String,
}
