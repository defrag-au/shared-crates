//! Replay a capture through the decoder, with no socket and no key.
//!
//! ```text
//! cargo run -p opensea-ws-client --example replay -- capture.jsonl
//! ```
//!
//! The other half of `capture`: it makes a corpus of real frames reusable, so an
//! ingest change is developed and checked against what the socket actually sent
//! rather than against a live subscription. That is the point of keeping the raw
//! bytes — a decoded event re-encoded has already lost whatever the body model
//! omits, so replaying one would check the model against itself.
//!
//! One line per frame on stdout; a count and any unreadable lines on stderr. Exits
//! non-zero if any line would not parse, so it can be a gate rather than only a
//! look.

use std::io::BufRead;

use opensea_stream::{EventStamp, StreamEvent};

const USAGE: &str = "usage: replay <capture.jsonl>";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or(USAGE)?;
    let file = std::fs::File::open(&path)?;

    let mut read = 0u64;
    let mut events = 0u64;
    let mut failed = 0u64;

    for line in std::io::BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        read += 1;

        let (Ok(stamp), Ok(event)) = (EventStamp::from_wire(&line), StreamEvent::from_wire(&line))
        else {
            failed += 1;
            eprintln!("unreadable: {}", &line[..line.len().min(200)]);
            continue;
        };

        let Some(stamp) = stamp else {
            println!("(protocol)  {}", protocol_event(&line));
            continue;
        };

        events += 1;
        let body = match event {
            Some(event) => summarise(&event),
            // A stamp without a body would be a crate bug rather than bad input:
            // both read the same frame.
            None => "event, no body".to_owned(),
        };
        println!("{}  {}  {}", stamp.topic, stamp.event_type, body);
    }

    eprintln!("{read} frames, {events} events, {failed} unreadable");
    if failed > 0 {
        return Err(format!("{failed} frame(s) would not parse").into());
    }
    Ok(())
}

/// The protocol message a non-event frame carries, for the log line.
fn protocol_event(line: &str) -> String {
    opensea_stream::FrameHeader::from_wire(line)
        .map(|header| header.event.as_wire().to_owned())
        .unwrap_or_else(|_| "unreadable".to_owned())
}

/// One line about an event.
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
        StreamEvent::ItemCancelled(cancelled) => format!(
            "cancelled {} {} {}",
            cancelled.item.nft_id, cancelled.base_price, cancelled.payment_token.symbol
        ),
        StreamEvent::OrderInvalidate(invalidated) => format!(
            "invalid   {} {}",
            invalidated
                .item
                .as_ref()
                .map(|item| item.nft_id.as_str())
                .unwrap_or("-"),
            invalidated.order_hash
        ),
        StreamEvent::ItemListed(listed) => format!(
            "list      {} {} {}",
            listed.item.nft_id, listed.base_price, listed.payment_token.symbol
        ),
        StreamEvent::ItemReceivedBid(bid) => format!(
            "bid       {} {} {}",
            bid.item.nft_id, bid.base_price, bid.payment_token.symbol
        ),
        StreamEvent::CollectionOffer(offer) => format!(
            "colloffer {} {} {}",
            offer.collection.slug, offer.base_price, offer.payment_token.symbol
        ),
        StreamEvent::TraitOffer(offer) => format!(
            "traitoff  {} {} {}",
            offer.collection.slug, offer.base_price, offer.payment_token.symbol
        ),
        StreamEvent::OrderRevalidate(order) => {
            format!("revalid   {} {}", order.collection.slug, order.order_hash)
        }
        StreamEvent::Unmodelled(unmodelled) => format!("other     {}", unmodelled.event_type),
    }
}
