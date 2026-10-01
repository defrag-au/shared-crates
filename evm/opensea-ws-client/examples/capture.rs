//! Capture OpenSea asset movements, verbatim, as JSONL.
//!
//! ```text
//! OPENSEA_API_KEY=… cargo run -p opensea-ws-client --example capture -- \
//!     .tmp/movements.jsonl collection:stonkbrokers-434284142
//! ```
//!
//! Writes the **raw frames**, one per line — not a re-encoding of the decoded
//! event. A corpus of real bytes is what this crate's fixtures are built from, and
//! a decoded event re-encoded loses whatever the body model omits, `metadata` most
//! of all. A one-line summary goes to stderr so the file stays a clean corpus.
//!
//! The subscription is narrowed to the two event types that are an asset movement,
//! which is also what keeps it affordable: unfiltered, the same collections carry
//! most of their volume as listings, bids and metadata updates.
//!
//! `CAPTURE_SECONDS` bounds the run — unset means run until interrupted, which is
//! what a follower wants and a bounded corpus capture does not.
//!
//! Reads the key from the environment and never writes it out.

use std::io::Write;
use std::time::Duration;

use opensea_stream::{EventFilter, EventType, StreamEvent, Topic};
use opensea_ws_client::{StreamClient, Subscription};
use tokio::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPENSEA_API_KEY")
        .map_err(|_| "OPENSEA_API_KEY is not set in the environment")?;

    let mut args = std::env::args().skip(1);
    let out_path = args
        .next()
        .ok_or("usage: capture <out.jsonl> <topic> [topic…]")?;

    let topics = args
        .map(|raw| raw.parse::<Topic>())
        .collect::<Result<Vec<_>, _>>()?;
    if topics.is_empty() {
        return Err("usage: capture <out.jsonl> <topic> [topic…]".into());
    }

    let deadline = std::env::var("CAPTURE_SECONDS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .map(|seconds| Instant::now() + Duration::from_secs(seconds));

    let filter = EventFilter::only([EventType::ItemTransferred, EventType::ItemSold]);
    let watch = topics
        .into_iter()
        .map(|topic| Subscription::new(topic, filter.clone()))
        .collect();

    let mut client = StreamClient::connect(&api_key, watch).await?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(&out_path)?);
    eprintln!("capturing to {out_path}");

    let mut captured = 0u64;
    loop {
        let delivered = match deadline {
            Some(at) => match tokio::time::timeout_at(at, client.next_event()).await {
                Ok(delivered) => delivered,
                Err(_) => break,
            },
            None => client.next_event().await,
        };

        let Some(delivered) = delivered else {
            break;
        };

        writeln!(out, "{}", delivered.raw)?;
        out.flush()?;
        captured += 1;
        eprintln!("{}", summarise(&delivered.event));
    }

    eprintln!("captured {captured} events to {out_path}");
    Ok(())
}

/// One line about an event, for a human watching stderr.
fn summarise(event: &StreamEvent) -> String {
    match event {
        StreamEvent::ItemTransferred(movement) => format!(
            "transfer  {} {} -> {}",
            movement.item.nft_id, movement.from_account.address, movement.to_account.address
        ),
        StreamEvent::ItemSold(sale) => format!(
            "sale      {} {} {}",
            sale.item.nft_id, sale.sale_price, sale.payment_token.symbol
        ),
        StreamEvent::ItemMetadataUpdated(update) => format!(
            "metadata  {} traits={}",
            update.item.nft_id,
            update.item.metadata.traits.len()
        ),
        StreamEvent::Unmodelled(unmodelled) => format!("other     {}", unmodelled.event_type),
    }
}
