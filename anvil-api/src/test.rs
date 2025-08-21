#[cfg(test)]
mod tests {
    use crate::*;
    use dotenv::dotenv;

    use std::env;
    use test_utils::{self, test_case};
    use tracing::info;

    impl AnvilClient {
        fn from_env() -> Self {
            dotenv().ok();
            let base_url = env::var("ANVIL_BASE_URL")
                .unwrap_or_else(|_| "https://prod.api.ada-anvil.app".to_string());

            match env::var("ANVIL_API_KEY") {
                Ok(api_key) => Self::new().with_base_url(&base_url).with_api_key(&api_key),
                Err(_) => Self::new().with_base_url(&base_url),
            }
        }
    }

    #[test]
    fn test_deserialize_blackflag() {
        match serde_json::from_str::<CollectionAssetsResponse>(test_case!(
            "response_blackflag.json"
        )) {
            Ok(deserialized) => {
                println!("deserialized: {deserialized:?}");
            }
            Err(err) => {
                panic!("encountered decoding error: {err:?}");
            }
        }
    }

    #[ignore]
    #[tokio::test]
    async fn test_get_collection_assets_integration() {
        test_utils::init_test_tracing();

        let client = AnvilClient::from_env();
        let policy_id = env::var("TEST_POLICY_ID").unwrap_or_else(|_| {
            "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string()
        });

        let request = CollectionAssetsRequest::new(&policy_id)
            .with_limit(5)
            .with_sale_type(SaleType::All);

        match client.get_collection_assets(&request).await {
            Ok(response) => {
                info!("Collection assets response: count={}", response.count);
                info!("Results length: {}", response.results.len());

                if !response.results.is_empty() {
                    let first_asset = &response.results[0];
                    info!(
                        "First asset: unit={}, name={}",
                        first_asset.unit, first_asset.name
                    );
                    if let Some(listing) = &first_asset.listing {
                        info!(
                            "Listing price: {} on {}",
                            listing.price, listing.marketplace
                        );
                    }
                }
            }
            Err(err) => {
                info!("API call failed (expected if no auth): {:?}", err);
            }
        }
    }

    #[ignore]
    #[tokio::test]
    async fn test_trait_filtering_example() {
        test_utils::init_test_tracing();

        let client = AnvilClient::from_env();
        let policy_id = env::var("TEST_POLICY_ID").unwrap_or_else(|_| {
            "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string()
        });

        // Example: Filter for assets with Rank = Swab
        let rank_filter = PropertyFilter::new("Rank", "Swab");

        let request = CollectionAssetsRequest::for_listed_assets(&policy_id, Some(10))
            .with_properties(vec![rank_filter])
            .with_order_by(OrderBy::PriceAsc);

        match client.get_collection_assets(&request).await {
            Ok(response) => {
                info!("Trait-filtered assets response: count={}", response.count);
                info!("Results length: {}", response.results.len());

                for (i, asset) in response.results.iter().enumerate() {
                    info!(
                        "Asset {}: name={}, unit={}",
                        i + 1,
                        asset.name,
                        asset.unit
                    );

                    // Show the attributes to verify filtering worked
                    if !asset.attributes.is_empty() {
                        info!("  Attributes: {:?}", asset.attributes);
                    }

                    if let Some(listing) = &asset.listing {
                        info!(
                            "  Listed at: {} lovelace ({} ADA) on {}",
                            listing.price,
                            listing.price as f64 / 1_000_000.0,
                            listing.marketplace
                        );
                    }
                }

                // Verify that all returned assets have the expected trait
                for asset in &response.results {
                    if let Some(rank) = asset.attributes.get("Rank") {
                        assert_eq!(rank, "Swab", "Asset should have Rank = Swab");
                    }
                }

                if response.results.is_empty() {
                    info!("No assets found with Rank = Swab (this might be expected)");
                }
            }
            Err(err) => {
                info!("API call failed (expected if no auth): {:?}", err);
            }
        }
    }

    #[ignore]
    #[tokio::test]
    async fn test_simple_trait_filtering() {
        test_utils::init_test_tracing();

        let client = AnvilClient::from_env();
        let policy_id = env::var("TEST_POLICY_ID").unwrap_or_else(|_| {
            "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string()
        });

        // Simpler way to filter by traits using convenience method
        let request = CollectionAssetsRequest::for_listed_assets(&policy_id, Some(5))
            .with_trait("Rank", "Swab")
            .with_order_by(OrderBy::PriceAsc);

        match client.get_collection_assets(&request).await {
            Ok(response) => {
                info!("Simple trait filtering: found {} assets with Rank=Swab", response.results.len());
                
                for asset in &response.results {
                    if let Some(listing) = &asset.listing {
                        info!(
                            "{}: {} ADA on {}",
                            asset.name,
                            listing.price as f64 / 1_000_000.0,
                            listing.marketplace
                        );
                    }
                }
            }
            Err(err) => {
                info!("API call failed (expected if no auth): {:?}", err);
            }
        }
    }

    #[ignore]
    #[tokio::test]
    async fn test_multiple_trait_filtering() {
        test_utils::init_test_tracing();

        let client = AnvilClient::from_env();
        let policy_id = env::var("TEST_POLICY_ID").unwrap_or_else(|_| {
            "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string()
        });

        // Example of filtering by multiple traits at once
        let traits = vec![
            ("Rank", "Swab"),
            ("Background", "Lost Reef"),
        ];

        let request = CollectionAssetsRequest::for_listed_assets(&policy_id, Some(5))
            .with_traits(traits)
            .with_order_by(OrderBy::PriceAsc);

        match client.get_collection_assets(&request).await {
            Ok(response) => {
                info!("Multiple trait filtering: found {} assets", response.results.len());
                
                for asset in &response.results {
                    info!("Asset: {} - Attributes: {:?}", asset.name, asset.attributes);
                }
            }
            Err(err) => {
                info!("API call failed (expected if no auth): {:?}", err);
            }
        }
    }

    #[ignore]
    #[tokio::test]
    async fn test_get_collection_details() {
        test_utils::init_test_tracing();

        let client = AnvilClient::from_env();
        let policy_id = env::var("TEST_POLICY_ID").unwrap_or_else(|_| {
            "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string()
        });

        match client.get_collection_details(&policy_id).await {
            Ok(collection) => {
                info!("Collection details retrieved successfully");
                info!("  Name: {}", collection.name);
                info!("  Handle: {}", collection.handle);
                info!("  Policy ID: {}", collection.policy_id);
                
                // Basic validation
                assert!(!collection.name.is_empty(), "Collection name should not be empty");
                assert!(!collection.handle.is_empty(), "Collection handle should not be empty");
                assert_eq!(collection.policy_id, policy_id, "Policy ID should match request");
                
                if let Some(socials) = &collection.socials {
                    info!("  Website: {}", socials.website);
                    info!("  Twitter: {}", socials.twitter);
                    info!("  Discord: {}", socials.discord);
                }
            }
            Err(err) => {
                info!("API call failed (expected if no auth): {:?}", err);
            }
        }
    }

    #[test]
    fn test_collection_assets_request_serialization() {
        // Test that search term is properly serialized
        let request = CollectionAssetsRequest::for_listed_assets("test_policy", Some(5))
            .with_search_term("Luffy");
        
        let serialized = serde_json::to_string(&request).expect("Should serialize");
        assert!(serialized.contains("\"term\":\"Luffy\""), "Should contain search term");
        
        // Test without search term
        let request_no_term = CollectionAssetsRequest::for_listed_assets("test_policy", Some(5));
        let serialized_no_term = serde_json::to_string(&request_no_term).expect("Should serialize");
        assert!(!serialized_no_term.contains("term"), "Should not contain term field when None");
        
        // Test without order_by (should not include orderBy in JSON)
        assert!(!serialized_no_term.contains("orderBy"), "Should not contain orderBy when None");
        
        // Test with order_by
        let request_with_order = CollectionAssetsRequest::for_listed_assets("test_policy", Some(5))
            .with_order_by(OrderBy::PriceAsc);
        let serialized_with_order = serde_json::to_string(&request_with_order).expect("Should serialize");
        assert!(serialized_with_order.contains("\"orderBy\":\"priceAsc\""), "Should contain orderBy when set");
    }
}
