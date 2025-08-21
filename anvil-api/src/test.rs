#[cfg(test)]
mod tests {
    use crate::*;
    use dotenv::dotenv;
    use serde_json;
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
        let request = CollectionAssetsRequest {
            policy_id: env::var("TEST_POLICY_ID").unwrap_or_else(|_| {
                "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string()
            }),
            limit: Some(5),
            cursor: None,
            sale_type: Some(SaleType::All),
            order_by: Some(OrderBy::PriceAsc),
        };

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
}
