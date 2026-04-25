use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxAsset {
    pub id: String, // concatenated policy_id + encoded asset name
    #[serde(default = "default_single", with = "wasm_safe_serde::u64_required")]
    pub qty: u64,
}

// TODO: implement the various policy_id extraction functions
// TODO: look at using this asset in the various structs in types.rs

fn default_single() -> u64 {
    1
}
