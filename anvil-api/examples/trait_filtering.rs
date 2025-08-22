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
    let api_key =
        env::var("ANVIL_API_KEY").expect("ANVIL_API_KEY environment variable must be set");

    // Create client
    let client = AnvilClient::new().with_api_key(&api_key);

    // Use Blackflag policy ID for the example
    let policy_id = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

    println!("🔍 Trait Filtering Example with Anvil API");
    println!("=========================================");
    println!("Policy ID: {}", policy_id);
    println!();

    // Example 1: Filter for assets with specific trait (Rank = Swab)
    println!("📋 Example 1: Single trait filter (Rank = Swab)");
    println!("-----------------------------------------------");

    let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(5))
        .with_trait("Rank", "Swab")
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&request).await {
        Ok(response) => {
            println!(
                "✅ Found {} assets with Rank = Swab",
                response.results.len()
            );

            for (i, asset) in response.results.iter().enumerate() {
                println!("  {}. {} ({})", i + 1, asset.name, asset.unit);

                if let Some(listing) = &asset.listing {
                    println!(
                        "     💰 {} ADA on {}",
                        listing.price as f64 / 1_000_000.0,
                        listing.marketplace
                    );
                }

                // Show relevant attributes
                if let Some(rank) = asset.attributes.get("Rank") {
                    println!("     🎖️  Rank: {}", rank);
                }
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    println!();

    // Example 2: Multiple trait filtering
    println!("📋 Example 2: Multiple trait filters (Rank = Swab AND Background = Golden Mirage)");
    println!("-----------------------------------------------------------------------------");

    let traits = vec![("Rank", "Swab"), ("Background", "Golden Mirage")];

    let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(3))
        .with_traits(traits)
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&request).await {
        Ok(response) => {
            println!(
                "✅ Found {} assets with Rank = Swab AND Background = Lost Reef",
                response.results.len()
            );

            for (i, asset) in response.results.iter().enumerate() {
                println!("  {}. {} ({})", i + 1, asset.name, asset.unit);

                if let Some(listing) = &asset.listing {
                    println!(
                        "     💰 {} ADA on {}",
                        listing.price as f64 / 1_000_000.0,
                        listing.marketplace
                    );
                }

                // Show all attributes for verification
                println!("     🏷️  Traits: {:?}", asset.attributes);
            }

            if response.results.is_empty() {
                println!("  (No assets found with both traits - this is common for specific combinations)");
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    println!();

    // Example 3: Price + trait filtering
    println!("📋 Example 3: Price range + trait filtering (Rank = Swab, under 10 ADA)");
    println!("-----------------------------------------------------------------------");

    let request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(5))
        .with_trait("Rank", "Swab")
        .with_price_range(None, Some(10_000_000)) // Max 10 ADA (10M lovelace)
        .with_order_by(OrderBy::PriceAsc);

    match client.get_collection_assets(&request).await {
        Ok(response) => {
            println!(
                "✅ Found {} Swab assets under 10 ADA",
                response.results.len()
            );

            for (i, asset) in response.results.iter().enumerate() {
                println!("  {}. {} ({})", i + 1, asset.name, asset.unit);

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

    println!();
    println!("🎯 Trait filtering allows precise server-side filtering instead of");
    println!("   fetching all assets and filtering client-side!");
    println!();
    println!("💡 Try different traits like:");
    println!("   - Background: Lost Reef, Cobalt Waves, Anhedonian Eclipse");
    println!("   - Rank: Swab, Navigator, Quartermaster, Captain");
    println!("   - Or explore other trait combinations!");

    Ok(())
}
