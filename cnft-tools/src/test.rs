#[cfg(test)]
mod tests {
    #![allow(clippy::assertions_on_constants)]

    use crate::{CnftApi, CnftAsset};

    use std::collections::HashMap;
    use test_utils::test_case;
    use tracing::Level;

    #[test]
    fn test_deserialize() {
        match serde_json::from_str::<CnftAsset>(test_case!("asset_in_list_response.json")) {
            Ok(asset) => {
                let test_traits = HashMap::from([
                    ("Eyes".to_string(), vec!["Focus".to_string()]),
                    ("Nose".to_string(), vec!["Button".to_string()]),
                    ("Rank".to_string(), vec!["Quartermaster".to_string()]),
                    ("Skin".to_string(), vec!["Inked".to_string()]),
                    ("Mouth".to_string(), vec!["Gold Beard".to_string()]),
                    ("Weapon".to_string(), vec!["Shark's Hook".to_string()]),
                    ("Clothes".to_string(), vec!["Sapphire Warlord".to_string()]),
                    ("Headwear".to_string(), vec!["Deckhand's Cap".to_string()]),
                    ("Background".to_string(), vec!["Emerald Isle".to_string()]),
                ]);
                assert_eq!(asset.on_sale, Some(false));
                assert_eq!(asset.asset_name, Some("Pirate376".into()));
                assert_eq!(asset.asset_id, "376");
                assert_eq!(asset.name, "Pirate #376");
                assert_eq!(
                    asset.icon_url,
                    Some("QmSfqtMhjqeU6cncYWpMXcoQQVrzxsaap2SgRzmhkvXZC9".to_string())
                );
                assert_eq!(asset.trait_count, Some(9));
                assert_eq!(asset.encoded_name, "506972617465333736");
                assert_eq!(asset.build_type, Some("robot".into()));
                assert_eq!(asset.rarity_rank, 59);
                assert_eq!(
                    asset.owner_stake_key,
                    "stake1u8yccncl049nd25c8wlav3fplue9u34yy5822eru4v8w23g656ct9"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                panic!("failed decoding: {err:?}");
            }
        }
    }

    #[test]
    fn test_deserialize_blackflag() {
        match serde_json::from_str::<Vec<CnftAsset>>(test_case!("blackflag.json")) {
            Ok(assets) => {
                assert_eq!(assets.len(), 2000);

                match assets.iter().find(|a| a.name == "Luffy") {
                    Some(luffy) => {
                        assert_eq!(
                            luffy.traits,
                            HashMap::from([("Rank".to_string(), vec!["Legendary".to_string()]),])
                        )
                    }
                    None => panic!("luffy not found"),
                }

                for asset in assets {
                    if let Some(count) = asset.trait_count {
                        assert_eq!(asset.traits.keys().len() as u32, count);
                    }
                }
            }
            Err(err) => {
                panic!("failed decoding: {err:?}");
            }
        }
    }

    #[test]
    fn test_deserialize_salty_seagulls() {
        match serde_json::from_str::<Vec<CnftAsset>>(test_case!("salty_seagulls.json")) {
            Ok(assets) => {
                println!("asset count = {}", assets.len());
                assert_eq!(assets.len(), 5);

                // Verify basic structure and that we can handle arrays in trait values
                for asset in &assets {
                    if let Some(count) = asset.trait_count {
                        assert_eq!(asset.traits.keys().len() as u32, count);
                    }
                }

                // Verify at least one asset parsed correctly
                assert!(!assets.is_empty());
                let first = &assets[0];
                assert!(!first.traits.is_empty());

                // Verify we correctly parsed the flattened traits from King Daniel Navagio
                assert_eq!(first.name, "King Daniel Navagio");
                assert_eq!(
                    first.traits.get("background"),
                    Some(&vec!["Purple".to_string()])
                );
                assert_eq!(first.traits.get("role"), Some(&vec!["King".to_string()]));
                assert_eq!(
                    first.traits.get("class"),
                    Some(&vec!["Monarch".to_string()])
                );
                assert_eq!(
                    first.traits.get("colony"),
                    Some(&vec!["Navagio".to_string()])
                );
                assert_eq!(first.traits.get("matedPair"), Some(&vec!["No".to_string()]));

                // Verify we have all the expected trait keys
                assert!(first.traits.contains_key("feathers"));
                assert!(first.traits.contains_key("shirt"));
                assert!(first.traits.contains_key("eyes"));
                assert!(first.traits.contains_key("hat"));
                assert!(first.traits.contains_key("beak"));
            }
            Err(err) => {
                panic!("failed decoding: {err:?}");
            }
        }
    }

    #[tokio::test]
    async fn test_encounter() {
        worker_utils::init_tracing(Some(Level::DEBUG));

        match CnftApi::default()
            .get_for_policy("43206de9e07fbd36ce6c109b3d34637727233c58a0b38f1da00a9ccf")
            .await
        {
            Ok(assets) => {
                assert_eq!(assets.len(), 3333);
            }
            Err(err) => panic!("failed to call microversus api: {err:?}"),
        }
    }
}
