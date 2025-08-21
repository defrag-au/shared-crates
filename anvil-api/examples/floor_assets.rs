use anvil_api::AnvilClient;
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
    let api_key =
        env::var("ANVIL_API_KEY").expect("ANVIL_API_KEY environment variable must be set");

    // Create client
    let client = AnvilClient::new().with_api_key(&api_key);

    // Use Blackflag policy ID
    let policy_id = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

    println!("💰 Floor Assets Utility Example");
    println!("===============================");
    println!("Policy ID: {}", policy_id);
    println!();

    // Test getting top 10 floor assets
    match client.get_floor(policy_id, 10).await {
        Ok(floor_assets) => {
            println!("✅ Found {} floor assets:", floor_assets.len());
            println!();

            for (i, asset) in floor_assets.iter().enumerate() {
                println!("{}. {} ({})", i + 1, asset.name, asset.unit);

                if let Some(listing) = &asset.listing {
                    println!("   💰 {} ADA on {}", 
                        listing.price as f64 / 1_000_000.0, 
                        listing.marketplace
                    );
                } else {
                    println!("   🔒 Not currently listed (unexpected)");
                }

                // Show some key attributes
                if let Some(rank) = asset.attributes.get("Rank") {
                    println!("   🎖️  Rank: {}", rank);
                }

                if let Some(rarity) = asset.rarity {
                    println!("   ⭐ Rarity: #{}", rarity);
                }

                println!(); // Empty line for readability
            }

            if !floor_assets.is_empty() {
                let cheapest = &floor_assets[0];
                let most_expensive = &floor_assets[floor_assets.len() - 1];

                if let (Some(cheapest_listing), Some(expensive_listing)) = 
                    (&cheapest.listing, &most_expensive.listing) {
                    
                    println!("📊 Floor Summary:");
                    println!("   🔥 Floor Price: {} ADA ({})", 
                        cheapest_listing.price as f64 / 1_000_000.0,
                        cheapest.name
                    );
                    println!("   📈 Highest in Top {}: {} ADA ({})", 
                        floor_assets.len(),
                        expensive_listing.price as f64 / 1_000_000.0,
                        most_expensive.name
                    );

                    let price_range = expensive_listing.price - cheapest_listing.price;
                    println!("   📏 Price Range: {} ADA", 
                        price_range as f64 / 1_000_000.0
                    );
                }
            }

            println!();
            println!("🎯 This utility makes floor price discovery simple:");
            println!("   - Get cheapest assets instantly");
            println!("   - Perfect for marketplace interfaces"); 
            println!("   - Already sorted by price ascending");
        }
        Err(e) => {
            println!("❌ Error getting floor assets: {}", e);
        }
    }

    Ok(())
}