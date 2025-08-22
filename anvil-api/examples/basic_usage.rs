use anvil_api::{AnvilClient, CollectionAssetsRequest, SaleType};
use dotenv::dotenv;
use std::env;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    // Initialize tracing for debug output
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let client = match env::var("ANVIL_API_KEY") {
        Ok(api_key) => AnvilClient::new().with_api_key(&api_key),
        Err(_) => {
            println!("Warning: ANVIL_API_KEY not found in environment, proceeding without auth");
            AnvilClient::new()
        }
    };

    let request = CollectionAssetsRequest::new(
        "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string(),
    )
    .with_limit(20)
    .with_sale_type(SaleType::All);

    match client.get_collection_assets(&request).await {
        Ok(response) => {
            info!("✅ Successfully retrieved {} assets", response.count);
            info!("📄 Page state: {:?}", response.page_state);

            for (i, asset) in response.results.iter().enumerate() {
                println!("{}. Asset: {} ({})", i + 1, asset.name, asset.unit);
                if let Some(listing) = &asset.listing {
                    println!("   💰 Price: {} ADA", listing.price as f64 / 1_000_000.0);
                    println!("   🏪 Marketplace: {}", listing.marketplace);
                }
                if let Some(rarity_rank) = asset.rarity {
                    println!("   🏆 Rarity Rank: {}", rarity_rank);
                }
                println!();
            }
        }
        Err(err) => {
            eprintln!("❌ Error: {:?}", err);
            println!("\n💡 Make sure you have ANVIL_API_KEY set in your .env file if authentication is required");
        }
    }

    Ok(())
}
