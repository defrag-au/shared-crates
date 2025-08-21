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
    let api_key =
        env::var("ANVIL_API_KEY").expect("ANVIL_API_KEY environment variable must be set");

    // Create client
    let client = AnvilClient::new().with_api_key(&api_key);

    // Use Blackflag policy ID
    let policy_id = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

    println!("🔍 Testing Combined Free Text + Trait Filtering");
    println!("==============================================");

    // Test 1: Get baseline - all "Pirate" assets to see trait distribution
    println!("\n📋 Test 1: Get all 'Pirate' assets (baseline)");
    let baseline_request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(20))
        .with_search_term("Pirate")
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&baseline_request).await {
        Ok(response) => {
            println!(
                "✅ Found {} 'Pirate' assets (listed only)",
                response.results.len()
            );

            // Show first few with their traits
            for (i, asset) in response.results.iter().take(5).enumerate() {
                println!("  {}. {} ({})", i + 1, asset.name, asset.unit);
                if let Some(rank) = asset.attributes.get("Rank") {
                    println!("     🎖️  Rank: {}", rank);
                }
                if let Some(bg) = asset.attributes.get("Background") {
                    println!("     🌊 Background: {}", bg);
                }
                if let Some(listing) = &asset.listing {
                    println!("     💰 {} ADA", listing.price as f64 / 1_000_000.0);
                }
            }

            if response.results.len() > 5 {
                println!("  ... and {} more", response.results.len() - 5);
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    // Test 2: Combine free text "Pirate" + specific rank filter
    println!("\n📋 Test 2: 'Pirate' + Rank='Captain' (combined search)");
    let combined_request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(10))
        .with_search_term("Pirate")
        .with_trait("Rank", "Captain")
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&combined_request).await {
        Ok(response) => {
            println!(
                "✅ Found {} 'Pirate' assets with Rank='Captain'",
                response.results.len()
            );

            for (i, asset) in response.results.iter().enumerate() {
                println!("  {}. {} ({})", i + 1, asset.name, asset.unit);
                if let Some(rank) = asset.attributes.get("Rank") {
                    println!("     🎖️  Rank: {}", rank);
                }
                if let Some(listing) = &asset.listing {
                    println!(
                        "     💰 {} ADA on {}",
                        listing.price as f64 / 1_000_000.0,
                        listing.marketplace
                    );
                }
            }

            if response.results.is_empty() {
                println!("  (No Captain-ranked Pirates currently listed)");
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    // Test 3: Try a different rank that might be more common
    println!("\n📋 Test 3: 'Pirate' + Rank='Quartermaster' (different rank)");
    let combined_request2 = CollectionAssetsRequest::for_listed_assets(policy_id, Some(10))
        .with_search_term("Pirate")
        .with_trait("Rank", "Quartermaster")
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&combined_request2).await {
        Ok(response) => {
            println!(
                "✅ Found {} 'Pirate' assets with Rank='Quartermaster'",
                response.results.len()
            );

            for (i, asset) in response.results.iter().enumerate() {
                println!("  {}. {} ({})", i + 1, asset.name, asset.unit);
                if let Some(rank) = asset.attributes.get("Rank") {
                    println!("     🎖️  Rank: {}", rank);
                }
                if let Some(listing) = &asset.listing {
                    println!(
                        "     💰 {} ADA on {}",
                        listing.price as f64 / 1_000_000.0,
                        listing.marketplace
                    );
                }
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    // Test 4: Multiple trait filters + free text
    println!("\n📋 Test 4: '86' + Multiple traits (Rank + Background)");
    let multi_trait_request = CollectionAssetsRequest::new(policy_id)
        .with_search_term("86")
        .with_trait("Rank", "Swab")
        .with_trait("Background", "Rosy Tide")
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&multi_trait_request).await {
        Ok(response) => {
            println!(
                "✅ Found {} '86' assets with Rank='Swab' + Background='Rosy Tide'",
                response.results.len()
            );

            for (i, asset) in response.results.iter().enumerate() {
                println!("  {}. {} ({})", i + 1, asset.name, asset.unit);
                if let Some(rank) = asset.attributes.get("Rank") {
                    println!("     🎖️  Rank: {}", rank);
                }
                if let Some(bg) = asset.attributes.get("Background") {
                    println!("     🌊 Background: {}", bg);
                }
                if let Some(listing) = &asset.listing {
                    println!(
                        "     💰 {} ADA on {}",
                        listing.price as f64 / 1_000_000.0,
                        listing.marketplace
                    );
                }
            }

            if response.results.is_empty() {
                println!("  (Very specific combination - may not exist)");
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    // Test 5: Just trait filter without free text (for comparison)
    println!("\n📋 Test 5: Just Rank='Quartermaster' (no free text for comparison)");
    let trait_only_request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(10))
        .with_trait("Rank", "Quartermaster")
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&trait_only_request).await {
        Ok(response) => {
            println!(
                "✅ Found {} assets with Rank='Quartermaster' (any name)",
                response.results.len()
            );

            for (i, asset) in response.results.iter().take(3).enumerate() {
                println!("  {}. {} ({})", i + 1, asset.name, asset.unit);
                if let Some(listing) = &asset.listing {
                    println!("     💰 {} ADA", listing.price as f64 / 1_000_000.0);
                }
            }

            if response.results.len() > 3 {
                println!("  ... and {} more", response.results.len() - 3);
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    println!("\n🎯 Key Findings:");
    println!("   📊 Check DEBUG logs to see if both 'term' and 'properties' appear in URLs");
    println!("   🔍 Compare results between combined vs separate filtering");
    println!("   ⚖️  This will inform marketplace search UI design");

    Ok(())
}
