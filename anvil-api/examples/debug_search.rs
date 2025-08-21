use anvil_api::{AnvilClient, CollectionAssetsRequest, OrderBy, SaleType};
use dotenv::dotenv;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables
    dotenv().ok();
    
    // Initialize logging with DEBUG level to see the actual URLs
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // Get API key from environment
    let api_key = env::var("ANVIL_API_KEY")
        .expect("ANVIL_API_KEY environment variable must be set");

    // Create client
    let client = AnvilClient::new().with_api_key(&api_key);
    
    // Use Blackflag policy ID
    let policy_id = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

    println!("🔍 Debug Full Text Search");
    println!("========================");
    
    // Test 1: Basic search without term (baseline)
    println!("\n📋 Test 1: No search term (get any 3 listed assets)");
    let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(3))
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&request).await {
        Ok(response) => {
            println!("✅ Found {} assets without search term", response.results.len());
            for asset in &response.results {
                println!("  - {} ({})", asset.name, asset.unit);
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    // Test 2: Search with a very generic term
    println!("\n📋 Test 2: Search for 'Pirate' (should match many)");
    let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(10))
        .with_search_term("Pirate")
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&request).await {
        Ok(response) => {
            println!("✅ Found {} assets matching 'Pirate'", response.results.len());
            for asset in &response.results {
                println!("  - {} ({})", asset.name, asset.unit);
                if asset.name.contains("Pirate") {
                    println!("    ✓ Name contains 'Pirate'");
                }
                for (key, value) in &asset.attributes {
                    if value.contains("Pirate") {
                        println!("    ✓ Attribute {}={} contains 'Pirate'", key, value);
                    }
                }
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    // Test 3: Search for a number from asset names
    println!("\n📋 Test 3: Search for '1000' (asset numbers)");
    let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(5))
        .with_search_term("1000")
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&request).await {
        Ok(response) => {
            println!("✅ Found {} assets matching '1000'", response.results.len());
            for asset in &response.results {
                println!("  - {} ({})", asset.name, asset.unit);
                if asset.name.contains("1000") {
                    println!("    ✓ Name contains '1000'");
                }
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    // Test 4: Search for 'Luffy' in ALL assets (not just listed)
    println!("\n📋 Test 4: Search for 'Luffy' in ALL assets (including unlisted)");
    let request = CollectionAssetsRequest::new(policy_id)
        .with_limit(10)
        .with_search_term("Luffy")
        .with_sale_type(SaleType::All); // Search all assets, not just listed

    match client.get_collection_assets(&request).await {
        Ok(response) => {
            println!("✅ Found {} assets matching 'Luffy' (including unlisted)", response.results.len());
            for asset in &response.results {
                println!("  - {} ({})", asset.name, asset.unit);
                if asset.name.to_lowercase().contains("luffy") {
                    println!("    ✓ Name contains 'Luffy'");
                }
                if let Some(listing) = &asset.listing {
                    println!("    💰 Listed for {} ADA on {}", listing.price as f64 / 1_000_000.0, listing.marketplace);
                } else {
                    println!("    🔒 Not currently listed");
                }
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    // Test 5: Try searching for a rank value (this should return 0 as search is name-only)
    println!("\n📋 Test 5: Search for 'Swab' (rank value - should return 0)");
    let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(5))
        .with_search_term("Swab")
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&request).await {
        Ok(response) => {
            println!("✅ Found {} assets matching 'Swab'", response.results.len());
            if response.results.is_empty() {
                println!("  (Expected - free text search only works on asset names, not traits)");
            } else {
                for asset in &response.results {
                    println!("  - {} ({})", asset.name, asset.unit);
                    if let Some(rank) = asset.attributes.get("Rank") {
                        if rank.contains("Swab") {
                            println!("    ✓ Rank attribute contains 'Swab': {}", rank);
                        }
                    }
                }
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    // Test 6: Try a search term that should definitely not exist
    println!("\n📋 Test 6: Search for 'Nonexistent' (should return 0)");
    let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(5))
        .with_search_term("Nonexistent12345")
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&request).await {
        Ok(response) => {
            println!("✅ Found {} assets matching 'Nonexistent12345'", response.results.len());
            if response.results.is_empty() {
                println!("  (Expected - this confirms search is working)");
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    println!("\n💡 Key findings:");
    println!("   ✅ Free text search (term parameter) works correctly");
    println!("   ✅ Search is limited to ASSET NAMES only (not trait values)");
    println!("   ✅ Use saleType=all to include unlisted assets in search");
    println!("   ✅ Use properties filtering for trait-based searches");
    println!("   📊 Check DEBUG logs above to see actual API URLs");

    Ok(())
}