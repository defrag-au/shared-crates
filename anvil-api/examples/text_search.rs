use anvil_api::{AnvilClient, CollectionAssetsRequest, OrderBy};
use dotenv::dotenv;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables
    dotenv().ok();
    
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Get API key from environment
    let api_key = env::var("ANVIL_API_KEY")
        .expect("ANVIL_API_KEY environment variable must be set");

    // Create client
    let client = AnvilClient::new().with_api_key(&api_key);
    
    // Use Blackflag policy ID for the example
    let policy_id = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

    println!("🔍 Free Text Search Example with Anvil API");
    println!("==========================================");
    println!("Policy ID: {}", policy_id);
    println!();

    // Example 1: Search for "Luffy" in asset names/attributes
    println!("📋 Example 1: Search for 'Luffy' in Black Flag collection");
    println!("--------------------------------------------------------");
    
    let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(10))
        .with_search_term("Luffy")
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&request).await {
        Ok(response) => {
            println!("✅ Found {} assets matching 'Luffy'", response.results.len());
            
            for (i, asset) in response.results.iter().enumerate() {
                println!("  {}. {} ({})", i + 1, asset.name, asset.unit);
                
                if let Some(listing) = &asset.listing {
                    println!("     💰 {} ADA on {}", 
                        listing.price as f64 / 1_000_000.0, 
                        listing.marketplace
                    );
                }
                
                // Show attributes to see what matched
                if !asset.attributes.is_empty() {
                    println!("     🏷️  Traits: {:?}", asset.attributes);
                }
            }
            
            if response.results.is_empty() {
                println!("  (No assets found containing 'Luffy' - try different search terms)");
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    println!();
    
    // Example 2: Search for pirate-themed terms
    println!("📋 Example 2: Search for 'Captain' (pirate theme)");
    println!("------------------------------------------------");
    
    let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(8))
        .with_search_term("Captain")
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&request).await {
        Ok(response) => {
            println!("✅ Found {} assets matching 'Captain'", response.results.len());
            
            for (i, asset) in response.results.iter().enumerate() {
                println!("  {}. {} ({})", i + 1, asset.name, asset.unit);
                
                if let Some(listing) = &asset.listing {
                    println!("     💰 {} ADA on {}", 
                        listing.price as f64 / 1_000_000.0, 
                        listing.marketplace
                    );
                }
                
                // Highlight the rank if it contains Captain
                if let Some(rank) = asset.attributes.get("Rank") {
                    if rank.contains("Captain") {
                        println!("     🎖️  Rank: {} ⭐", rank);
                    } else {
                        println!("     🎖️  Rank: {}", rank);
                    }
                }
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    println!();
    
    // Example 3: Search combined with price filtering
    println!("📋 Example 3: Search 'Navigator' + under 5 ADA");
    println!("----------------------------------------------");
    
    let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(5))
        .with_search_term("Navigator")
        .with_price_range(None, Some(5_000_000)) // Max 5 ADA
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&request).await {
        Ok(response) => {
            println!("✅ Found {} Navigator assets under 5 ADA", response.results.len());
            
            for (i, asset) in response.results.iter().enumerate() {
                println!("  {}. {} ({})", i + 1, asset.name, asset.unit);
                
                if let Some(listing) = &asset.listing {
                    println!("     💰 {} ADA on {}", 
                        listing.price as f64 / 1_000_000.0, 
                        listing.marketplace
                    );
                }
                
                if let Some(rank) = asset.attributes.get("Rank") {
                    println!("     🎖️  Rank: {}", rank);
                }
            }
            
            if response.results.is_empty() {
                println!("  (No Navigator assets found under 5 ADA)");
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    println!();
    
    // Example 4: Search with trait filtering
    println!("📋 Example 4: Search 'Reef' + specific rank");
    println!("-------------------------------------------");
    
    let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(5))
        .with_search_term("Reef") // Should match "Lost Reef" background
        .with_trait("Rank", "Quartermaster")
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&request).await {
        Ok(response) => {
            println!("✅ Found {} assets with 'Reef' and Rank=Quartermaster", response.results.len());
            
            for (i, asset) in response.results.iter().enumerate() {
                println!("  {}. {} ({})", i + 1, asset.name, asset.unit);
                
                if let Some(listing) = &asset.listing {
                    println!("     💰 {} ADA on {}", 
                        listing.price as f64 / 1_000_000.0, 
                        listing.marketplace
                    );
                }
                
                // Show background and rank
                if let Some(bg) = asset.attributes.get("Background") {
                    println!("     🌊 Background: {}", bg);
                }
                if let Some(rank) = asset.attributes.get("Rank") {
                    println!("     🎖️  Rank: {}", rank);
                }
            }
            
            if response.results.is_empty() {
                println!("  (No assets found with both criteria - very specific combination)");
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    println!();
    println!("🎯 Free text search allows finding assets by:");
    println!("   - Asset names containing keywords");
    println!("   - Trait values containing keywords");
    println!("   - Combined with other filters for precision!");
    println!();
    println!("🏴‍☠️ Try other pirate-themed searches:");
    println!("   - 'Treasure' - for treasure-related traits");
    println!("   - 'Storm' - for stormy backgrounds");
    println!("   - 'Gold' - for golden items/backgrounds");
    println!("   - 'Skull' - for skull-related accessories");
    println!("   - Or any other creative terms!");

    Ok(())
}