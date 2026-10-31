//! Folds a `chain-events` NDJSON stream and reports what it earned.
//!
//! This is the batch half of the two-runtime design: the same [`apply`] a Durable Object
//! runs per emission, looped over a file instead. It exists so a stream can be measured
//! before any worker exists, and so a change to the fold can be diffed against a real run.
//!
//! ```sh
//! cargo run -p holder-behaviour --example replay -- \
//!     ~/code/defrag/mitos/.tmp/streams/perps.ndjson
//! cargo run -p holder-behaviour --example replay -- \
//!     ~/code/defrag/mitos/.tmp/merged.ndjson --collection perps --now 201677944
//! ```
//!
//! Trait rows are not in the event stream — they are a metadata projection — so
//! `has_all_traits` reports `unavailable` here. That is the honest answer for this input,
//! not a zero, and it is the same answer the evaluator gives a collection whose metadata
//! pass has not run.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::process::ExitCode;

use chain_events::{ChainEvent, Coverage, Family};
use holder_behaviour::{
    Anomaly, Assessment, Definition, EarnedSince, Evidence, MemoryStore, Outcome, Predicate,
    Progress, SECONDS_PER_DAY, Slot, Store, apply, evaluate,
};

/// The definitions this build can answer, mirroring the catalogue's ids and thresholds.
fn definitions() -> Vec<Definition<'static>> {
    vec![
        Definition {
            id: "never-sold",
            predicate: Predicate::NeverDisposed,
        },
        Definition {
            id: "mint-to-now",
            predicate: Predicate::HeldSinceMint,
        },
        Definition {
            id: "tenure-1k",
            predicate: Predicate::TotalTenureAtLeast { days: 1_000 },
        },
        Definition {
            id: "tenure-5k",
            predicate: Predicate::TotalTenureAtLeast { days: 5_000 },
        },
        Definition {
            id: "round-trip",
            predicate: Predicate::RoundTripsAtLeast { n: 5 },
        },
        Definition {
            id: "tri-faction",
            predicate: Predicate::HasAllTraits {
                category: "Faction".into(),
                values: vec![
                    "Astro Schism".into(),
                    "Genovault".into(),
                    "Kraashers".into(),
                ],
            },
        },
    ]
}

struct Args {
    path: String,
    collection: Option<String>,
    now: Option<Slot>,
    subject: Option<String>,
    limit: usize,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let mut path = None;
    let mut collection = None;
    let mut now = None;
    let mut subject = None;
    let mut limit = 8;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--collection" => collection = Some(next(&mut args, "--collection")?),
            "--now" => {
                now = Some(
                    next(&mut args, "--now")?
                        .parse()
                        .map_err(|_| "--now takes a slot".to_string())?,
                )
            }
            "--subject" => subject = Some(next(&mut args, "--subject")?),
            "--limit" => {
                limit = next(&mut args, "--limit")?
                    .parse()
                    .map_err(|_| "--limit takes a number".to_string())?
            }
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other => path = Some(other.to_string()),
        }
    }
    Ok(Args {
        path: path.ok_or("give the path to a chain-events NDJSON file")?,
        collection,
        now,
        subject,
        limit,
    })
}

fn next(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("replay: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let file =
        std::fs::File::open(&args.path).map_err(|error| format!("open {}: {error}", args.path))?;

    let mut store = MemoryStore::default();
    let mut events: u64 = 0;
    let mut kept: u64 = 0;
    let mut skipped_collection: u64 = 0;
    let mut skipped_unparsed: u64 = 0;
    let mut anomalies: Vec<Anomaly> = Vec::new();
    let mut changed: u64 = 0;
    let mut holder: u64 = 0;
    let mut market: u64 = 0;
    let mut asset: u64 = 0;
    let mut admin: u64 = 0;
    let mut latest: Slot = 0;
    let mut chosen: Option<String> = args.collection.clone();

    for line in BufReader::new(file).lines() {
        let line = line.map_err(|error| format!("read: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        events += 1;
        let event: ChainEvent = match serde_json::from_str(&line) {
            Ok(event) => event,
            Err(_) => {
                skipped_unparsed += 1;
                continue;
            }
        };
        let collection = event.collection.to_string();
        let first = chosen.get_or_insert_with(|| collection.clone());
        let matches = match &args.collection {
            Some(wanted) => collection.contains(wanted.as_str()),
            None => &collection == first,
        };
        if !matches {
            skipped_collection += 1;
            continue;
        }
        kept += 1;
        match event.body.family() {
            Family::Holder => holder += 1,
            Family::Market => market += 1,
            Family::Asset => asset += 1,
            Family::Admin => admin += 1,
        }
        if let Some(coord) = event.coord {
            latest = latest.max(coord.height);
        }
        let applied = apply(&mut store, &event).map_err(|error| format!("{error:?}"))?;
        if !applied.subjects.is_empty() {
            changed += 1;
        }
        anomalies.extend(applied.anomalies);
    }

    let now = args.now.unwrap_or(latest);
    let subjects = store.subjects().map_err(|error| format!("{error:?}"))?;

    println!("file        {}", args.path);
    println!(
        "collection  {}",
        chosen.as_deref().unwrap_or("(none found)")
    );
    println!("now         slot {now}");
    println!(
        "events      {events} read · {kept} folded · {skipped_collection} other collection · \
         {skipped_unparsed} unparsed"
    );
    println!(
        "families    holder {holder} · market {market} · asset {asset} · admin {admin} · \
         {changed} changed a lot"
    );
    println!("subjects    {}", subjects.len());

    if anomalies.is_empty() {
        println!("anomalies   none");
    } else {
        let mut by_kind: BTreeMap<&str, u64> = BTreeMap::new();
        for anomaly in &anomalies {
            let key = match anomaly {
                Anomaly::DisposedWithoutAcquisition { .. } => "disposed with no lot",
                Anomaly::NoCoordinate { .. } => "no coordinate",
                Anomaly::CollectionScoped => "collection-scoped movement",
            };
            *by_kind.entry(key).or_default() += 1;
        }
        let summary: Vec<String> = by_kind
            .iter()
            .map(|(kind, count)| format!("{count} {kind}"))
            .collect();
        println!("anomalies   {} ({})", anomalies.len(), summary.join(" · "));
        for anomaly in anomalies.iter().take(args.limit) {
            println!("            {anomaly:?}");
        }
    }

    let definitions = definitions();
    let at = Assessment {
        now,
        holder_coverage: if skipped_unparsed == 0 && anomalies.is_empty() {
            Coverage::Complete
        } else {
            Coverage::Partial
        },
    };

    let mut tally: BTreeMap<&str, [u64; 4]> = definitions
        .iter()
        .map(|definition| (definition.id, [0; 4]))
        .collect();
    let mut standings: Vec<(String, Vec<Outcome<'_>>)> = Vec::new();
    for subject in &subjects {
        let outcomes =
            evaluate(&store, *subject, &at, &definitions).map_err(|error| format!("{error:?}"))?;
        for outcome in &outcomes {
            let slot = tally.get_mut(outcome.id).expect("every id is tallied");
            match outcome.progress {
                Progress::Earned { .. } => slot[0] += 1,
                Progress::Pending { .. } => slot[1] += 1,
                Progress::Missed => slot[2] += 1,
                Progress::Unavailable(_) => slot[3] += 1,
            }
        }
        if let Some(wanted) = &args.subject {
            let account = store
                .account_of(*subject)
                .map_err(|error| format!("{error:?}"))?
                .map(|account| account.address)
                .unwrap_or_default();
            if &account == wanted {
                standings.push((account, outcomes));
            }
        }
    }

    println!();
    println!(
        "{:<14} {:>8} {:>8} {:>8} {:>13}",
        "achievement", "earned", "pending", "missed", "unavailable"
    );
    for definition in &definitions {
        let [earned, pending, missed, unavailable] = tally[definition.id];
        println!(
            "{:<14} {earned:>8} {pending:>8} {missed:>8} {unavailable:>13}",
            definition.id
        );
    }

    for (account, outcomes) in &standings {
        println!();
        println!("standing    {account}");
        for outcome in outcomes {
            println!("            {}", describe(outcome, &store)?);
        }
    }

    if args.subject.is_some() && standings.is_empty() {
        println!();
        println!("standing    not found in this stream");
    }

    Ok(())
}

fn describe(outcome: &Outcome<'_>, store: &MemoryStore) -> Result<String, String> {
    let state = match outcome.progress {
        Progress::Earned { since } => match since {
            EarnedSince::Exact(slot) => format!("earned at {slot}"),
            EarnedSince::AsAt(slot) => format!("earned as at {slot}"),
        },
        Progress::Pending {
            have,
            need,
            eta_slot,
        } => {
            // Tenure is the only predicate measured in seconds, so report it that way.
            let scale = if outcome.id.starts_with("tenure") {
                format!(" ({:.0}d)", have as f64 / SECONDS_PER_DAY as f64)
            } else {
                String::new()
            };
            match eta_slot {
                Some(eta) => format!("pending {have}/{need}{scale}, eta {eta}"),
                None => format!("pending {have}/{need}{scale}"),
            }
        }
        Progress::Missed => "missed".to_string(),
        Progress::Unavailable(gap) => format!("unavailable: {gap:?}"),
    };
    let cited: Vec<String> = outcome
        .evidence
        .iter()
        .map(|evidence| match evidence {
            Evidence::Lot {
                unit,
                acquired_at,
                disposed_at,
                ..
            } => {
                let asset = store
                    .token_of(*unit)
                    .ok()
                    .flatten()
                    .map(|token| token.to_string())
                    .unwrap_or_else(|| format!("unit {unit:?}"));
                match disposed_at {
                    Some(disposed) => format!("{asset} {acquired_at}..{disposed}"),
                    None => format!("{asset} open from {acquired_at}"),
                }
            }
            Evidence::Traits { unit, values } => {
                format!("unit {unit:?} {}", values.join(", "))
            }
        })
        .collect();
    if cited.is_empty() {
        Ok(format!("{:<14} {state}", outcome.id))
    } else {
        Ok(format!("{:<14} {state} — {}", outcome.id, cited.join("; ")))
    }
}
