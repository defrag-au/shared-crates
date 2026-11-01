//! Gross size of the three Robinhood partner collections.
//!
//! The chain-wide `robinhood_census` pages every collection and enriches each one. This asks
//! only the three contracts the campaign scores: resolve the contract, read the collection,
//! read its stats. Three requests per partner instead of three per collection on the chain.
//!
//! ```sh
//! cargo run -p opensea-client --example robinhood_partners
//! ```
//!
//! `.env` is optional and an exported key wins. The examples are the entry points; the
//! library reads the key the caller passes it and never loads a file on a consumer's behalf.
//!
//! `owners` is OpenSea's count of distinct owners for the collection, not the holder set. The
//! set — and the overlap with Perps holders — needs `evm_logs`; this tells us whether the
//! population is worth capturing before that run.

use std::time::Duration;

use opensea_client::{ChainSlug, OpenseaClient};

const CONTRACTS: [&str; 3] = [
    "0x505a22ffed8d37ebe580ffd98d2cdb0021189146",
    "0x23708e501cbeaa92f45183197c673fdabf993484",
    "0xe638f58c87258ece6c0eddc46f70327663635c69",
];

/// A polite pause between requests; the standard key is a few requests a second.
const SPACING: Duration = Duration::from_millis(350);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The sibling checkout's `.env` first, then the ordinary upward search. An exported key
    // wins: dotenv does not override the environment.
    let _ = dotenvy::from_path("../cnft.dev-workers/.env");
    let _ = dotenvy::dotenv();
    let Ok(key) = std::env::var("OPENSEA_API_KEY") else {
        eprintln!("OPENSEA_API_KEY is not set — it belongs in the environment, never in the tree.");
        std::process::exit(2);
    };
    let client = OpenseaClient::new(key).with_timeout(Duration::from_secs(20));
    let chain = ChainSlug::new(ChainSlug::ROBINHOOD);

    println!("partner collections · Robinhood ({chain})");
    println!();
    println!(
        "  {:<44} {:<22} {:<24} {:>7} {:>8} {:>8} {:>10}",
        "contract", "slug", "name", "supply", "owners", "sales", "floor"
    );

    let mut failures = Vec::new();
    for address in CONTRACTS {
        match read_one(&client, &chain, address).await {
            Ok(row) => println!(
                "  {:<44} {:<22} {:<24} {:>7} {:>8} {:>8} {:>10}",
                row.address, row.slug, row.name, row.supply, row.owners, row.sales, row.floor
            ),
            Err(reason) => failures.push((address, reason)),
        }
        tokio::time::sleep(SPACING).await;
    }

    println!();
    println!(
        "  owners is OpenSea's distinct-owner count; the holder set needs an evm_logs replay."
    );
    for (address, reason) in failures {
        println!("  {address}: {reason}");
    }
    Ok(())
}

struct Row {
    address: String,
    slug: String,
    name: String,
    supply: u64,
    owners: u64,
    sales: u64,
    floor: String,
}

async fn read_one(client: &OpenseaClient, chain: &ChainSlug, address: &str) -> Result<Row, String> {
    let contract = client
        .contract(chain, address)
        .await
        .map_err(|error| format!("contract: {error}"))?;
    let name = contract
        .name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "(unnamed)".to_string());
    let slug = contract.collection;
    tokio::time::sleep(SPACING).await;

    let detail = client
        .collection(&slug)
        .await
        .map_err(|error| format!("collection: {error}"))?;
    tokio::time::sleep(SPACING).await;

    let stats = client
        .collection_stats(&slug)
        .await
        .map_err(|error| format!("stats: {error}"))?;

    let floor = match stats.total.floor_price {
        Some(price) => format!("{price:.4} {}", stats.total.floor_price_symbol),
        None => "-".to_string(),
    };
    Ok(Row {
        address: contract.address,
        slug,
        name,
        supply: detail.total_supply,
        owners: stats.total.num_owners,
        sales: stats.total.sales,
        floor,
    })
}
