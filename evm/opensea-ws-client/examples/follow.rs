//! Follow a watch set: join the socket, capture raw frames, and record how far the
//! consumer got.
//!
//! ```text
//! OPENSEA_API_KEY=… cargo run -p opensea-ws-client --example follow -- \
//!     watch.json capture.jsonl checkpoints.jsonl
//! ```
//!
//! `watch.json` is a [`WatchSet`] — the config the follower reads. `examples/watch.json`
//! is one, holding the single collection whose **stream** slug is known to deliver
//! (`stonkbrokers-434284142` — the suffixed form, not the `stonkbrokers` the REST API
//! returns):
//!
//! ```json
//! { "collections": [ { "slug": "stonkbrokers-434284142", "chain": "eip155:4663" } ] }
//! ```
//!
//! That slug was measured to deliver **unfiltered**. With the ownership filter it
//! delivered nothing at all in a 30-second window, which is what a 4,444-supply
//! collection looks like and is not evidence of a fault — but it is also what a
//! wrong slug looks like, and the stream does not distinguish the two. Which is the
//! trap this config is where you record your way out of: a slug that has been
//! *observed* to deliver belongs here, and a quiet join is not a refusal.
//!
//! Every frame is appended to the capture file **verbatim**, before anything is done
//! with it, so a crash loses nothing that arrived. The checkpoint file is rewritten
//! every hundred events and is what a reconcile reads to know where to start — see
//! [`opensea_ws_client::checkpoint`] for why it is written rarely and rewritten
//! rather than appended.
//!
//! Warnings go to stderr through a subscriber, because the failure this tool has is
//! silence: a wrong slug and a filter that matches nothing both join `ok` and then
//! deliver nothing, which looks exactly like a quiet collection. The subscriber is
//! fixed at the default level — `RUST_LOG` is not consulted — so what it shows is
//! what the client emits.
//!
//! The key comes from the environment and is never written anywhere.

use std::io::Write;

use opensea_stream::{StreamEvent, WatchSet};
use opensea_ws_client::{Checkpoints, StreamClient, Subscription};

const USAGE: &str = "usage: follow <watch.json> <capture.jsonl> [checkpoints.jsonl]";

/// How many events between checkpoint writes.
///
/// The file is rewritten rather than appended, so this is a durability window and
/// not a file-size one: a crash can lose the events since the last flush, and could
/// lose an equal window to a socket drop anyway. The stream has no replay, which is
/// why a reconcile is what closes a gap rather than a checkpoint's precision.
const FLUSH_EVERY: u64 = 100;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // No subscriber means the client's warnings — a refused join, an undecodable
    // frame, a reconnect — go nowhere, which is the one thing this tool must not do.
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .try_init();

    let api_key = std::env::var("OPENSEA_API_KEY")
        .map_err(|_| "OPENSEA_API_KEY is not set in the environment")?;

    let mut args = std::env::args().skip(1);
    let config_path = args.next().ok_or(USAGE)?;
    let capture_path = args.next().ok_or(USAGE)?;
    let checkpoint_path = args
        .next()
        .unwrap_or_else(|| "checkpoints.jsonl".to_owned());

    let watch: WatchSet = serde_json::from_str(&std::fs::read_to_string(&config_path)?)?;
    if watch.is_empty() {
        return Err(format!("{config_path} names no collections — nothing to follow").into());
    }

    eprintln!("following {} collection(s):", watch.len());
    for (topic, filter) in watch.subscriptions() {
        eprintln!("  {topic}  {filter:?}");
    }

    let subscriptions = watch
        .subscriptions()
        .into_iter()
        .map(|(topic, filter)| Subscription::new(topic, filter))
        .collect();

    let mut client = StreamClient::connect(&api_key, subscriptions).await?;
    let mut checkpoints = Checkpoints::open(&checkpoint_path)?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(&capture_path)?);
    eprintln!("capturing to {capture_path}, checkpoints to {checkpoint_path}");

    let mut seen = 0u64;
    while let Some(delivered) = client.next_event().await {
        // The raw frame first: a consumer that fails below has still kept the
        // evidence, and a capture is worth more than a clean exit.
        writeln!(out, "{}", delivered.raw)?;
        out.flush()?;

        let topic = delivered.stamp.topic.as_wire();
        checkpoints.observe(&topic, &delivered.stamp, event_timestamp(&delivered.event));

        seen += 1;
        if seen.is_multiple_of(FLUSH_EVERY) {
            checkpoints.flush()?;
        }
        eprintln!("{seen:>6}  {topic}  {}", summarise(&delivered.event));
    }

    checkpoints.flush()?;
    eprintln!("stream ended after {seen} events; checkpoints in {checkpoint_path}");
    Ok(())
}

/// The movement timestamp, where the event has one. A metadata update does not, so
/// the checkpoint keeps the last movement's.
fn event_timestamp(event: &StreamEvent) -> Option<&str> {
    match event {
        StreamEvent::ItemTransferred(movement) => Some(&movement.event_timestamp),
        StreamEvent::ItemSold(sale) => Some(&sale.event_timestamp),
        _ => None,
    }
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
