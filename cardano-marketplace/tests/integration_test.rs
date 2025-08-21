use cardano_marketplace::{MarketplaceClient, TraitFilter};
use dotenv::dotenv;
use std::{collections, env};
use test_utils::init_test_tracing;

const BLACKFLAG_POLICY_ID: &str = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

async fn setup_client() -> MarketplaceClient {
    dotenv().ok();
    init_test_tracing();

    let api_key = env::var("ANVIL_API_KEY")
        .expect("ANVIL_API_KEY environment variable must be set for integration tests");

    MarketplaceClient::with_api_key(&api_key)
}

#[tokio::test]
async fn test_get_collection_details() {
    let client = setup_client().await;

    let result = client.get_collection_details(BLACKFLAG_POLICY_ID).await;
    assert!(
        result.is_ok(),
        "Failed to get collection details: {:?}",
        result.err()
    );

    let collection = result.unwrap();
    assert_eq!(collection.policy_id, BLACKFLAG_POLICY_ID);
    println!("collection: {:?}", collection);
}

#[tokio::test]
async fn test_get_floor_price() {
    let client = setup_client().await;

    let result = client.get_floor_price(BLACKFLAG_POLICY_ID).await;
    assert!(
        result.is_ok(),
        "Failed to get floor price: {:?}",
        result.err()
    );

    let floor = result.unwrap();
    assert!(floor.price > 0, "Floor price should be greater than 0");
    assert!(floor.count > 0, "Floor count should be greater than 0");
    assert!(!floor.sample_assets.is_empty(), "Should have sample assets");

    println!("Floor Price:");
    println!(
        "  Price: {} lovelace ({} ADA)",
        floor.price,
        floor.price as f64 / 1_000_000.0
    );
    println!("  Count: {}", floor.count);
    println!("  Sample assets: {}", floor.sample_assets.len());
    println!(
        "  Marketplace distribution: {:?}",
        floor.marketplace_distribution
    );
}

#[tokio::test]
async fn test_get_floor_price_with_traits() {
    let client = setup_client().await;

    // Create a trait filter for testing
    let trait_filter =
        TraitFilter::new().add_single_trait("Background".to_string(), "Lost Reef".to_string());

    let result = client
        .get_floor_price_filtered(BLACKFLAG_POLICY_ID, &trait_filter)
        .await;

    // This might fail if no assets match the filter, which is expected
    match result {
        Ok(floor) => {
            println!("Filtered Floor Price (Background: Lost Reef):");
            println!(
                "  Price: {} lovelace ({} ADA)",
                floor.price,
                floor.price as f64 / 1_000_000.0
            );
            println!("  Count: {}", floor.count);
            println!("  Sample assets: {}", floor.sample_assets.len());
        }
        Err(cardano_marketplace::MarketplaceError::NoListingsFound) => {
            println!("No listings found for Background: Cyan (expected)");
        }
        Err(e) => {
            panic!("Unexpected error: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_trait_filter_builder() {
    let filter = TraitFilter::new()
        .add_single_trait("Background".to_string(), "Blue".to_string())
        .add_trait(
            "Clothes".to_string(),
            vec!["Shirt".to_string(), "Jacket".to_string()],
        );

    assert!(!filter.is_empty());
    assert_eq!(filter.filters.len(), 2);
    assert_eq!(filter.filters["Background"], vec!["Blue"]);
    assert_eq!(filter.filters["Clothes"], vec!["Shirt", "Jacket"]);
}
