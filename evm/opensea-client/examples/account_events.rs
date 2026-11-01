//! What OpenSea will tell you about one account, and how far back it goes.
//!
//! ```sh
//! cargo run -p opensea-client --example account_events -- 0x1f00…0364 [--pages 5] [--limit 50]
//! ```
//!
//! # Why this probe exists
//!
//! `GET /api/v2/events/accounts/{address}` is the only OpenSea surface that could
//! supplement a chain replay with marketplace context. The stream is live-only, and the
//! collection-wide routes do not exist — probed 2026-11-01 against a route known to
//! answer 401, `/events/collections/{slug}` and `/events/chain/{chain}/contract/{address}`
//! both 404. So "which accounts, and how far back" is answered here or not at all.
//!
//! The two numbers that decide its usefulness:
//!
//! - **span** — the oldest event the paging reaches. A campaign needs history back to a
//!   collection's mint, not back to the ingestion of a watcher.
//! - **from/to** — whether a movement names both accounts. Without them an event is a
//!   price with no parties.
//!
//! # `serde_json::Value`, deliberately
//!
//! The response is not modelled, which is the point: this is the capture that decides
//! what the types should be. `Value` is confined to this example under the exception
//! `rules/core/typed-json-only` allows — an external shape that cannot yet be known.
//! Once the shape settles it moves into `types.rs` with a fixture, and this file reads
//! the typed version.
//!
//! # Where the endpoint disagrees with itself
//!
//! The list key is not one of the ones the rest of v2 uses, so this accepts the two
//! plausible names (`asset_events`, `events`) and prints the body's own keys when it
//! finds neither — a probe that guessed wrong should say what it saw rather than report
//! zero events, which is also what an empty account looks like.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use opensea_client::OpenseaClient;
use serde_json::Value;

/// A polite pause between pages; the standard key allows a few requests a second.
const SPACING: Duration = Duration::from_millis(350);

const USAGE: &str = "usage: account_events <address> [--pages N] [--limit N]";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let Some(address) = args.next() else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };

    let mut pages = 10usize;
    let mut limit = 50u32;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--pages" => pages = args.next().ok_or(USAGE)?.parse()?,
            "--limit" => limit = args.next().ok_or(USAGE)?.parse()?,
            other => {
                eprintln!("unknown argument {other}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }

    // The sibling checkout's `.env` first, then the ordinary upward search. An exported
    // key wins: dotenv does not override the environment.
    let _ = dotenvy::from_path("../cnft.dev-workers/.env");
    let _ = dotenvy::dotenv();
    let Ok(key) = std::env::var("OPENSEA_API_KEY") else {
        eprintln!("OPENSEA_API_KEY is not set — it belongs in the environment, never in the tree.");
        std::process::exit(2);
    };
    let client = OpenseaClient::new(key).with_timeout(Duration::from_secs(20));

    let mut types: BTreeMap<String, u32> = BTreeMap::new();
    // The keys each event type carries, so a party field is found rather than guessed at.
    let mut shapes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut oldest: Option<u64> = None;
    let mut newest: Option<u64> = None;
    let mut with_parties = 0u32;
    let mut seen = 0u32;
    let mut cursor: Option<String> = None;
    let mut fetched = 0usize;

    while fetched < pages {
        let mut path = format!("/events/accounts/{address}?limit={limit}");
        if let Some(next) = &cursor {
            path.push_str(&format!("&next={next}"));
        }

        let body: Value = match client.get_json(&path).await {
            Ok(body) => body,
            Err(error) => {
                eprintln!("page {} failed: {error}", fetched + 1);
                break;
            }
        };

        let events = body
            .get("asset_events")
            .or_else(|| body.get("events"))
            .and_then(Value::as_array);
        let Some(events) = events else {
            let keys: Vec<&str> = body
                .as_object()
                .map(|object| object.keys().map(String::as_str).collect())
                .unwrap_or_default();
            eprintln!("the body carries no event list — its keys are {keys:?}");
            break;
        };

        fetched += 1;
        eprintln!("page {fetched}: {} events", events.len());

        for event in events {
            seen += 1;
            let kind = event
                .get("event_type")
                .and_then(Value::as_str)
                .unwrap_or("(untyped)")
                .to_string();
            *types.entry(kind.clone()).or_default() += 1;
            if let Some(object) = event.as_object() {
                shapes
                    .entry(kind.clone())
                    .or_default()
                    .extend(object.keys().cloned());
            }
            if let Some(stamp) = event.get("event_timestamp").and_then(Value::as_u64) {
                oldest = Some(oldest.map_or(stamp, |old| old.min(stamp)));
                newest = Some(newest.map_or(stamp, |new| new.max(stamp)));
            }
            // The party fields are not the same on every type, which is the thing this
            // probe is here to find out; `addresses` counts whichever ones exist.
            if addresses(event).len() == 2 {
                with_parties += 1;
            }
        }

        match body.get("next").and_then(Value::as_str) {
            Some(next) if !next.is_empty() => cursor = Some(next.to_string()),
            _ => {
                cursor = None;
                break;
            }
        }
        tokio::time::sleep(SPACING).await;
    }

    println!("account  {address}");
    println!(
        "pages    {fetched} · {seen} events · limit {limit}{}",
        match &cursor {
            Some(_) => " · more remain",
            None => " · exhausted",
        }
    );
    let mut histogram: Vec<(String, u32)> = types.into_iter().collect();
    histogram.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    println!(
        "types    {}",
        histogram
            .iter()
            .map(|(kind, count)| format!("{kind} {count}"))
            .collect::<Vec<_>>()
            .join(" · ")
    );
    match (oldest, newest) {
        (Some(oldest), Some(newest)) => {
            let days = (newest.saturating_sub(oldest)) / 86_400;
            println!(
                "span     {} → {} ({days} days)",
                day(oldest as i64),
                day(newest as i64)
            );
        }
        _ => println!("span     no event_timestamp on any event"),
    }
    println!("parties  {with_parties} of {seen} events name two accounts");
    for (kind, keys) in shapes.iter().take(3) {
        println!(
            "{:<8} {}",
            kind,
            keys.iter().cloned().collect::<Vec<_>>().join(", ")
        );
    }
    Ok(())
}

/// Every account address an event names, wherever the type puts it.
fn addresses(event: &Value) -> Vec<&str> {
    let mut found = Vec::new();
    for key in [
        "from_address",
        "to_address",
        "seller",
        "buyer",
        "maker",
        "taker",
    ] {
        if let Some(address) = event.get(key).and_then(Value::as_str) {
            found.push(address);
        }
    }
    for key in ["from_account", "to_account"] {
        if let Some(address) = event.get(key).and_then(|a| a.get("address")) {
            if let Some(address) = address.as_str() {
                found.push(address);
            }
        }
    }
    found
}

/// Unix seconds → `YYYY-MM-DD`, by the civil-from-days inverse so this needs no date crate.
fn day(seconds: i64) -> String {
    let z = seconds.div_euclid(86_400) + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}
