use anvil_api::{AnvilClient, CollectionAssetsRequest, OrderBy, SaleType};
use dotenv::dotenv;
use futures::StreamExt; // for .next(), .take(), etc.
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables
    dotenv().ok();

    // Initialize logging with DEBUG to see pagination details
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // Get API key from environment
    let api_key =
        env::var("ANVIL_API_KEY").expect("ANVIL_API_KEY environment variable must be set");

    // Create client
    let client = AnvilClient::new().with_api_key(&api_key);

    // Use Blackflag policy ID to test pagination with many assets
    let policy_id = "812197d5f4cdd9ebb05d40e259c181982d4b3d8c2505b1a7ad800bdc";

    println!("🌊 Asset Streaming Examples");
    println!("===========================");
    println!("Policy ID: {} (Blackflag collection)", policy_id);
    println!();

    // Example 1: Stream floor assets (listed only, price ascending)
    println!("📋 Example 1: Stream floor assets (listed only)");
    println!("-----------------------------------------------");

    let floor_request = CollectionAssetsRequest::for_listed_assets(policy_id, Some(5))
        .with_order_by(OrderBy::PriceAsc);

    let floor_stream = client.stream_assets(floor_request);
    tokio::pin!(floor_stream);
    let mut count = 0;
    let max_items = 10; // Limit for demo

    while let Some(result) = floor_stream.next().await {
        match result {
            Ok(asset) => {
                count += 1;
                println!(
                    "{}. {} - {}",
                    count,
                    asset.name,
                    asset
                        .listing
                        .as_ref()
                        .map(|l| format!(
                            "{:.1} ADA on {}",
                            l.price as f64 / 1_000_000.0,
                            l.marketplace
                        ))
                        .unwrap_or_else(|| "Not listed".to_string())
                );

                // Show attributes if available
                if let Some(rarity) = asset.rarity {
                    println!("   ⭐ Rarity: #{}", rarity);
                }

                if count >= max_items {
                    println!("   (Stopping after {} items for demo)", max_items);
                    break;
                }
            }
            Err(e) => {
                println!("❌ Stream error: {}", e);
                break;
            }
        }
    }

    println!("   ✅ Streamed {} listed assets", count);
    println!();

    // Example 2: Stream all assets (both listed and unlisted) with pagination demo
    println!("📋 Example 2: Stream ALL assets (both listed and unlisted)");
    println!("---------------------------------------------------------");

    let all_request = CollectionAssetsRequest::new(policy_id)
        .with_sale_type(SaleType::All) // Include both listed and unlisted
        .with_limit(10); // Small page size to demonstrate pagination

    let all_stream = client.stream_assets(all_request);
    tokio::pin!(all_stream);
    let mut all_count = 0;
    let mut listed_count = 0;
    let mut unlisted_count = 0;

    while let Some(result) = all_stream.next().await {
        match result {
            Ok(asset) => {
                all_count += 1;

                if asset.listing.is_some() {
                    listed_count += 1;
                    println!(
                        "{}. {} - LISTED at {}",
                        all_count,
                        asset.name,
                        asset
                            .listing
                            .as_ref()
                            .map(|l| format!("{:.1} ADA", l.price as f64 / 1_000_000.0))
                            .unwrap()
                    );
                } else {
                    unlisted_count += 1;
                    println!("{}. {} - NOT LISTED", all_count, asset.name);
                }

                // Show rarity if available
                if let Some(rarity) = asset.rarity {
                    println!("   ⭐ Rarity: #{}", rarity);
                }
            }
            Err(e) => {
                println!("❌ Stream error: {}", e);
                break;
            }
        }
    }

    println!();
    println!("   📊 Complete Collection Summary:");
    println!("     • Total Assets: {}", all_count);
    println!("     • Listed: {}", listed_count);
    println!("     • Unlisted: {}", unlisted_count);
    println!(
        "     • Listing Rate: {:.1}%",
        (listed_count as f64 / all_count as f64) * 100.0
    );
    println!();
    println!("🎯 Streaming Benefits:");
    println!("   ✅ Automatic pagination handling");
    println!("   ✅ Memory efficient (one asset at a time)");
    println!("   ✅ Can stop/resume at any point");
    println!("   ✅ Perfect for processing large collections");
    println!("   ✅ Works with any filtering criteria");

    Ok(())
}
