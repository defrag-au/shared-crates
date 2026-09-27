//! A census of Robinhood Chain NFT collections, and a close look at one target.
//!
//! ```sh
//! OPENSEA_API_KEY=… cargo run -p opensea-client --example robinhood_census
//! OPENSEA_API_KEY=… cargo run -p opensea-client --example robinhood_census -- 0x7980aa64093853cb78c927e05b88fed96e945f81
//! OPENSEA_API_KEY=… cargo run -p opensea-client --example robinhood_census -- stonkbrokers
//! ```
//!
//! The target is optional and is either a contract address or a collection slug.
//!
//! Three requests per collection — the list for identity, the detail for supply,
//! the stats for owners/floor/volume — so an OpenSea standard key's few requests
//! a second is the real constraint. The run paces itself, retries a rate limit,
//! and reports every collection it could not read rather than dropping it.

use std::collections::BTreeSet;
use std::time::Duration;

use opensea_client::{ChainSlug, CollectionDetails, CollectionStats, OpenseaClient, OpenseaError};

/// The API's own maximum is higher, but 50 keeps each page cheap and the cursor
/// behaviour identical.
const PAGE_SIZE: u32 = 50;
/// A runaway guard, not a target: the run says so if it stops here.
const PAGE_CAP: usize = 200;
/// ~4 requests a second, which is what OpenSea's standard keys allow.
const SPACING: Duration = Duration::from_millis(260);
const RETRIES: u32 = 3;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Ok(key) = std::env::var("OPENSEA_API_KEY") else {
        eprintln!("OPENSEA_API_KEY is not set — see the crate docs for where the key belongs.");
        std::process::exit(2);
    };

    let client = OpenseaClient::new(key).with_timeout(REQUEST_TIMEOUT);
    let chain = ChainSlug::new(ChainSlug::ROBINHOOD);
    let target = std::env::args().nth(1);

    census(&client, &chain).await?;

    if let Some(target) = target {
        inspect(&client, &chain, &target).await;
    }

    Ok(())
}

/// One collection's numbers, gathered from the two endpoints that carry them.
struct Row {
    name: String,
    supply: u64,
    owners: u64,
    sales: u64,
    floor: Option<f64>,
    floor_symbol: String,
    volume: f64,
    volume_symbol: String,
}

/// Everything the chain indexes, with each collection's numbers beside it.
async fn census(
    client: &OpenseaClient,
    chain: &ChainSlug,
) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("paging /collections?chain={chain} …");

    let mut collections = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0usize;

    loop {
        let page = client
            .collections_page(chain, Some(PAGE_SIZE), cursor.as_deref())
            .await?;
        collections.extend(page.collections);
        pages += 1;
        cursor = page.next;
        eprintln!("  page {pages}: {} collections so far", collections.len());

        match cursor {
            None => break,
            Some(_) if pages >= PAGE_CAP => {
                eprintln!("  stopping at the {PAGE_CAP}-page cap — this is not the whole set");
                break;
            }
            Some(_) => tokio::time::sleep(SPACING).await,
        }
    }

    let total = collections.len();
    eprintln!("enriching {total} collections (two requests each) …");

    let mut rows = Vec::new();
    let mut failures = Vec::new();

    for (index, collection) in collections.iter().enumerate() {
        let slug = collection.collection.as_str();
        let detail = detail(client, slug).await;
        tokio::time::sleep(SPACING).await;
        let stats = stats(client, slug).await;

        match (detail, stats) {
            (Ok(detail), Ok(stats)) => rows.push(Row {
                name: collection.name.clone(),
                supply: detail.total_supply,
                owners: stats.total.num_owners,
                sales: stats.total.sales,
                floor: stats.total.floor_price,
                floor_symbol: stats.total.floor_price_symbol,
                volume: stats.total.volume,
                volume_symbol: stats.total.volume_symbol,
            }),
            (detail, stats) => {
                let reason = detail.err().or_else(|| stats.err());
                failures.push((slug.to_owned(), reason.map(|error| error.to_string())));
            }
        }

        if (index + 1) % 10 == 0 {
            eprintln!("  {}/{total}", index + 1);
        }
        tokio::time::sleep(SPACING).await;
    }

    print_table(&rows, total);
    print_failures(&failures);

    Ok(())
}

fn print_table(rows: &[Row], total: usize) {
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by(|a, b| {
        b.volume
            .partial_cmp(&a.volume)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.sales.cmp(&a.sales))
    });

    println!();
    println!(
        "Robinhood Chain collections — {total} indexed, {} read, by volume",
        sorted.len()
    );

    let floor_symbols: BTreeSet<&str> = sorted
        .iter()
        .map(|row| row.floor_symbol.as_str())
        .collect();
    let volume_symbols: BTreeSet<&str> = sorted
        .iter()
        .map(|row| row.volume_symbol.as_str())
        .collect();
    println!(
        "floor in {} · volume in {}",
        join(&floor_symbols),
        join(&volume_symbols)
    );

    println!();
    println!(
        "  {:<3} {:<36} {:>7} {:>8} {:>8} {:>11} {:>12}",
        "#", "name", "supply", "owners", "sales", "floor", "volume"
    );
    for (index, row) in sorted.iter().enumerate() {
        let Row {
            name,
            supply,
            owners,
            sales,
            floor,
            volume,
            ..
        } = row;
        let floor = match floor {
            Some(price) => format!("{price:.4}"),
            None => "-".to_owned(),
        };
        println!(
            "  {:<3} {:<36} {supply:>7} {owners:>8} {sales:>8} {floor:>11} {volume:>12.4}",
            index + 1,
            shorten(name, 36)
        );
    }

    println!();
    println!("  slug is the key for every other OpenSea endpoint; names repeat, slugs do not.");
    println!("  run with a target to see one collection's detail and its first tokens, e.g.");
    println!("    … --example robinhood_census -- 0x7980aa64093853cb78c927e05b88fed96e945f81");
}

fn print_failures(failures: &[(String, Option<String>)]) {
    if failures.is_empty() {
        return;
    }
    println!();
    println!("{} collections could not be read:", failures.len());
    for (slug, reason) in failures {
        match reason {
            Some(reason) => println!("  {slug}: {reason}"),
            None => println!("  {slug}: unknown"),
        }
    }
}

/// One contract or collection, in full: what it is, what it holds, what it earns.
async fn inspect(client: &OpenseaClient, chain: &ChainSlug, target: &str) {
    println!();
    println!("── {target}");

    let is_address = target.starts_with("0x") || target.starts_with("0X");
    let mut address = None;

    let slug = if is_address {
        match client.contract(chain, target).await {
            Ok(contract) => {
                address = Some(contract.address.clone());
                println!("  contract    {}", contract.address);
                println!("  chain       {}", contract.chain);
                println!("  standard    {}", contract.contract_standard);
                println!(
                    "  name        {}",
                    contract.name.as_deref().unwrap_or("(unnamed)")
                );
                println!("  collection  {}", contract.collection);
                contract.collection
            }
            Err(error) => {
                println!("  could not resolve that contract: {error}");
                return;
            }
        }
    } else {
        target.to_owned()
    };

    match detail(client, &slug).await {
        Ok(detail) => print_detail(&detail),
        Err(error) => println!("  could not read the collection: {error}"),
    }

    match stats(client, &slug).await {
        Ok(stats) => print_stats(&stats),
        Err(error) => println!("  could not read the stats: {error}"),
    }

    if let Some(address) = address
        && let Ok(page) = client
            .contract_nfts_page(chain, &address, Some(5), None)
            .await
    {
        if page.nfts.is_empty() {
            println!("  tokens      none indexed");
        } else {
            println!("  tokens      {} of the first page:", page.nfts.len());
            for nft in &page.nfts {
                let name = nft.name.as_deref().unwrap_or("(unnamed)");
                println!(
                    "    #{:<6} {:<28} {} traits",
                    nft.identifier,
                    shorten(name, 28),
                    nft.traits.len()
                );
            }
            match page.next {
                Some(_) => println!("    … and more"),
                None => println!("    (that is the whole contract)"),
            }
        }
    }
}

fn print_detail(detail: &CollectionDetails) {
    println!("  name        {}", detail.summary.name);
    println!("  supply      {}", detail.total_supply);
    println!("  unique      {}", detail.unique_item_count);
    println!("  created     {}", detail.created_date);
    for fee in &detail.fees {
        println!(
            "  fee         {}% to {} (required: {})",
            fee.fee, fee.recipient, fee.required
        );
    }
    for contract in &detail.summary.contracts {
        println!("  contract    {} on {}", contract.address, contract.chain);
    }
    let listing = &detail.pricing_currencies.listing_currency;
    println!(
        "  priced in   {} ({}, {} decimals)",
        listing.symbol, listing.name, listing.decimals
    );
    if let Some(banner) = &detail.summary.banner_image_url {
        println!("  banner      {banner}");
    }
}

fn print_stats(stats: &CollectionStats) {
    let total = &stats.total;
    println!(
        "  lifetime    {} {} volume · {} sales · {} owners",
        total.volume, total.volume_symbol, total.sales, total.num_owners
    );
    match total.floor_price {
        Some(price) => println!("  floor       {price} {}", total.floor_price_symbol),
        None => println!("  floor       none"),
    }
    for interval in &stats.intervals {
        println!(
            "  {:<11} {} {} volume · {} sales",
            interval.interval, interval.volume, interval.volume_symbol, interval.sales
        );
    }
}

/// The detail call, retried through a rate limit — a census that dies at
/// collection 150 has wasted the run.
async fn detail(client: &OpenseaClient, slug: &str) -> Result<CollectionDetails, OpenseaError> {
    let mut attempt = 0;
    loop {
        match client.collection(slug).await {
            Err(OpenseaError::RateLimited {
                retry_after_seconds,
            }) if attempt < RETRIES => {
                attempt += 1;
                wait(retry_after_seconds, attempt).await;
            }
            other => return other,
        }
    }
}

/// The stats call, on the same reasoning as [`detail`].
async fn stats(client: &OpenseaClient, slug: &str) -> Result<CollectionStats, OpenseaError> {
    let mut attempt = 0;
    loop {
        match client.collection_stats(slug).await {
            Err(OpenseaError::RateLimited {
                retry_after_seconds,
            }) if attempt < RETRIES => {
                attempt += 1;
                wait(retry_after_seconds, attempt).await;
            }
            other => return other,
        }
    }
}

async fn wait(retry_after_seconds: Option<u64>, attempt: u32) {
    let seconds = retry_after_seconds.unwrap_or(1).max(1);
    eprintln!("    rate limited — waiting {seconds}s (attempt {attempt} of {RETRIES})");
    tokio::time::sleep(Duration::from_secs(seconds)).await;
}

/// `a, b, c`, for the symbol lines.
fn join(symbols: &BTreeSet<&str>) -> String {
    symbols.iter().copied().collect::<Vec<_>>().join(", ")
}

/// Clip to a column width without splitting a character.
fn shorten(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_owned();
    }
    let mut clipped: String = value.chars().take(width.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}
