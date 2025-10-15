#![allow(unused)]

use orchestrator::AssetRefresh;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;

pub trait MatchesPolicy {
    fn get_actions(&self, policy_id: &str) -> Vec<AssetRefresh>;
}

#[derive(Deserialize, Debug, Clone)]
pub struct DemeterMintHookRequest {
    pub context: DemeterWebhookContext,
    pub mint: DemeterMint,
    pub fingerprint: Option<String>,
    pub variant: String, // TODO: can be used for routing the webhook
    pub timestamp: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DemeterWebhookContext {
    block_hash: String,
    block_number: u64,
    slot: u64,
    timestamp: u64,
    tx_idx: u32,
    tx_hash: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DemeterMint {
    policy: String,
    asset: String,
    quantity: f64,
}

impl MatchesPolicy for DemeterMintHookRequest {
    fn get_actions(&self, policy_id: &str) -> Vec<AssetRefresh> {
        if self.mint.policy.eq(policy_id) {
            vec![AssetRefresh::Mint {
                policy_id: String::from(policy_id),
                asset_id: self.mint.asset.clone(),
                mint_timestamp: self.context.timestamp,
            }]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::assertions_on_constants)]
    use assets::Asset;

    use super::*;

    macro_rules! test_case {
        ($fname:expr) => {{
            let filename = concat!(env!("CARGO_MANIFEST_DIR"), "/resources/test/", $fname);
            let mut file = File::open(filename).unwrap();
            let mut buff = String::new();
            file.read_to_string(&mut buff).unwrap();

            println!("buff: {}", &buff.to_string());
            &buff.to_string()
        }};
    }

    const BLACKFLAG_POLICY_ID: &str = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

    #[test]
    fn test_deserialize() {
        match serde_json::from_str::<DemeterMintHookRequest>(test_case!("demeter_mint.json")) {
            Ok(result) => assert!(true, "decoded successfully"),
            Err(err) => {
                println!("encountered decoding error: {:?}", err);
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_extract_matching_assets() {
        let request =
            serde_json::from_str::<DemeterMintHookRequest>(test_case!("demeter_mint.json"))
                .unwrap();
        let matching = request.get_actions(BLACKFLAG_POLICY_ID);

        assert_eq!(
            matching,
            Vec::from([AssetRefresh::Mint {
                policy_id: String::from(BLACKFLAG_POLICY_ID),
                asset_id: String::from("506972617465313432"),
                mint_timestamp: 1736746787,
            }])
        );
    }
}
