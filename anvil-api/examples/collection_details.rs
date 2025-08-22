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

    println!("🏴‍☠️ Collection Details Utility Example");
    println!("=====================================");
    println!("Policy ID: {}", policy_id);
    println!();

    match client.get_collection_details(policy_id).await {
        Ok(collection) => {
            println!("✅ Collection Details Retrieved:");
            println!("   📛 Name: {}", collection.name);
            if let Some(handle) = &collection.handle {
                println!("   🆔 Handle: {}", handle);
            }
            println!("   🗂️ Policy ID: {}", collection.policy_id);

            if let Some(description) = &collection.description {
                println!("   📝 Description: {}", description);
            }

            if let Some(image) = &collection.image {
                println!("   🖼️  Image: {}", image);
            }

            if let Some(banner) = &collection.banner {
                println!("   🏞️  Banner: {}", banner);
            }

            if let Some(royalty_address) = &collection.royalty_address {
                println!("   💰 Royalty Address: {}", royalty_address);
            }

            println!(
                "   💯 Royalty Percentage: {:.2}%",
                collection.royalty_percentage
            );

            if let Some(socials) = &collection.socials {
                println!("   🌐 Social Links:");
                if let Some(website) = &socials.website {
                    println!("      🌐 Website: {}", website);
                }
                if let Some(twitter) = &socials.twitter {
                    println!("      🐦 Twitter: {}", twitter);
                }
                if let Some(discord) = &socials.discord {
                    println!("      💬 Discord: {}", discord);
                }
            }

            println!();
            println!("🎯 This utility makes it easy to get collection metadata");
            println!("   without having to parse asset responses manually!");
        }
        Err(e) => {
            println!("❌ Error getting collection details: {}", e);
        }
    }

    Ok(())
}
