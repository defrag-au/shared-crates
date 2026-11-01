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
//! cargo run -p holder-behaviour --example replay -- \
//!     ~/code/defrag/mitos/.tmp/perps-with-market.ndjson \
//!     --catalogue ../cnft.dev-workers/config/achievements/perps.toml \
//!     --collection e6ba9c0f --traits .tmp/perps-traits.ndjson \
//!     --bundle .tmp/perps.bundle --built-at 1793427949
//! ```
//!
//! `--catalogue` swaps the six built-in definitions for the frozen catalogue, and
//! `--bundle` writes the snapshot this run produced in the published format. The catalogue
//! names more predicate kinds than this build implements; those are reported by kind and
//! marked **not evaluated** in the bundle (`population = 0`), never as a zero score.
//!
//! A reading has two axes: `--now` is the chain height, and `--now-unix` is the wall clock
//! durations are measured on. They default to the latest `coord.height` and the latest
//! `occurred_at` in the file, not to the machine's clock, so replaying one file twice writes
//! byte-identical bundles. Give both to describe one instant explicitly.
//!
//! Trait rows are not in the event stream — they are a metadata projection — so
//! `has_all_traits` reports `unavailable` here. That is the honest answer for this input,
//! not a zero, and it is the same answer the evaluator gives a collection whose metadata
//! pass has not run.

mod support;

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use chain_events::{ChainEvent, Coverage, Family};
use holder_behaviour::{
    AcquiredBy, Anomaly, Assessment, Bundle, Definition, Earned, EarnedSince, Evidence, Lot,
    MemoryStore, Outcome, Predicate, Progress, Rarity, SECONDS_PER_DAY, Slot, Snapshot, Store,
    SubjectId, SubjectRecord, UnitId, apply, catalogue_digest, evaluate, write_bundle,
};
use support::catalogue::LoadedCatalogue;

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
    now_unix: Option<u64>,
    subject: Option<String>,
    limit: usize,
    catalogue: Option<String>,
    bundle: Option<String>,
    built_at: Option<u64>,
    traits: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let mut path = None;
    let mut collection = None;
    let mut now = None;
    let mut now_unix = None;
    let mut subject = None;
    let mut limit = 8;
    let mut catalogue = None;
    let mut bundle = None;
    let mut built_at = None;
    let mut traits = None;
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
            "--now-unix" => {
                now_unix = Some(
                    next(&mut args, "--now-unix")?
                        .parse()
                        .map_err(|_| "--now-unix takes unix seconds".to_string())?,
                )
            }
            "--subject" => subject = Some(next(&mut args, "--subject")?),
            "--limit" => {
                limit = next(&mut args, "--limit")?
                    .parse()
                    .map_err(|_| "--limit takes a number".to_string())?
            }
            "--catalogue" => catalogue = Some(next(&mut args, "--catalogue")?),
            "--bundle" => bundle = Some(next(&mut args, "--bundle")?),
            "--traits" => traits = Some(next(&mut args, "--traits")?),
            "--built-at" => {
                built_at = Some(
                    next(&mut args, "--built-at")?
                        .parse()
                        .map_err(|_| "--built-at takes unix seconds".to_string())?,
                )
            }
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other => path = Some(other.to_string()),
        }
    }
    Ok(Args {
        path: path.ok_or("give the path to a chain-events NDJSON file")?,
        collection,
        now,
        now_unix,
        subject,
        limit,
        catalogue,
        bundle,
        built_at,
        traits,
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
    if args.bundle.is_some() && args.catalogue.is_none() {
        return Err(
            "--bundle needs --catalogue: a row index is only meaningful against one".to_string(),
        );
    }
    let catalogue = match &args.catalogue {
        Some(path) => Some(LoadedCatalogue::load(path)?),
        None => None,
    };
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
    let mut latest_unix: u64 = 0;
    let mut holder_cursor: Slot = 0;
    let mut market_cursor: Slot = 0;
    let mut chosen: Option<String> = args.collection.clone();
    // The collection the matching events actually name, as CAIP-19 — what a bundle states.
    // `chosen` above may be a substring the caller searched for, which is not an identity.
    let mut identity: Option<String> = None;

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
        identity.get_or_insert_with(|| collection.clone());
        kept += 1;
        latest_unix = latest_unix.max(event.occurred_at);
        let family = event.body.family();
        match family {
            Family::Holder => holder += 1,
            Family::Market => market += 1,
            Family::Asset => asset += 1,
            Family::Admin => admin += 1,
        }
        if let Some(coord) = event.coord {
            latest = latest.max(coord.height);
            match family {
                Family::Holder => holder_cursor = holder_cursor.max(coord.height),
                Family::Market => market_cursor = market_cursor.max(coord.height),
                Family::Asset | Family::Admin => {}
            }
        }
        let applied = apply(&mut store, &event).map_err(|error| format!("{error:?}"))?;
        if !applied.subjects.is_empty() {
            changed += 1;
        }
        anomalies.extend(applied.anomalies);
    }

    // Seeding happens after the fold because the store interns a unit when an event first
    // names it — the snapshot joins on that unit table rather than creating its own.
    let seeded = match &args.traits {
        Some(path) => Some(seed_traits(&mut store, path)?),
        None => None,
    };

    let now = args.now.unwrap_or(latest);
    // The two axes of one reading. The unix clock defaults to the latest event the file
    // carries, not to the machine's clock, so replaying the same file twice measures the
    // same tenures and writes byte-identical bundles.
    let now_unix = args.now_unix.unwrap_or(latest_unix);
    let subjects = store.subjects().map_err(|error| format!("{error:?}"))?;

    println!("file        {}", args.path);
    println!(
        "collection  {} · searched {}",
        identity.as_deref().unwrap_or("(none found)"),
        chosen.as_deref().unwrap_or("(none given)")
    );
    println!("now         slot {now} · unix {now_unix}");
    println!(
        "events      {events} read · {kept} folded · {skipped_collection} other collection · \
         {skipped_unparsed} unparsed"
    );
    println!(
        "families    holder {holder} · market {market} · asset {asset} · admin {admin} · \
         {changed} changed the store"
    );
    println!("subjects    {}", subjects.len());
    if let Some((rows, matched, unmatched)) = seeded {
        println!("traits      {matched}/{rows} seeded · {unmatched} not in this stream");
    }

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

    let fallback = definitions();
    let (definitions, positions): (Vec<Definition<'_>>, Vec<usize>) = match &catalogue {
        Some(catalogue) => (catalogue.definitions(), catalogue.positions()),
        None => {
            let positions = (0..fallback.len()).collect();
            (fallback.clone(), positions)
        }
    };
    let coverage = if skipped_unparsed == 0 && anomalies.is_empty() {
        Coverage::Complete
    } else {
        Coverage::Partial
    };
    let at = Assessment {
        now,
        now_unix,
        holder_coverage: coverage,
    };

    if let Some(catalogue) = &catalogue {
        println!(
            "catalogue   {} achievements · {} evaluable · {} not in this build",
            catalogue.len(),
            definitions.len(),
            catalogue.len() - definitions.len()
        );
        for (kind, count) in catalogue.gaps() {
            println!("            gap {kind} × {count}");
        }
    }

    let mut tally: BTreeMap<&str, [u64; 4]> = definitions
        .iter()
        .map(|definition| (definition.id, [0; 4]))
        .collect();
    let mut standings: Vec<(String, Vec<Outcome<'_>>)> = Vec::new();
    // (core earned, flavour earned) per subject, for the standings summary.
    let mut per_holder: Vec<(u32, u32)> = Vec::new();
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
        if let Some(catalogue) = &catalogue {
            let mut core = 0u32;
            let mut flavour = 0u32;
            for (outcome, position) in outcomes.iter().zip(&positions) {
                if !matches!(outcome.progress, Progress::Earned { .. }) {
                    continue;
                }
                match catalogue.role_at(*position) {
                    Some("core") => core += 1,
                    _ => flavour += 1,
                }
            }
            per_holder.push((core, flavour));
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

    if let Some(catalogue) = &catalogue {
        print_standings(catalogue, &positions, &definitions, &tally, &per_holder);
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

    if let Some(path) = &args.bundle {
        let catalogue = catalogue.as_ref().expect("checked at parse");
        let collection = identity.as_deref().ok_or_else(|| {
            "--bundle needs a matching event: a bundle must name the collection it scores"
                .to_string()
        })?;
        let built_at = args.built_at.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs())
                .unwrap_or(0)
        });
        let snapshot = build_snapshot(
            &store,
            &subjects,
            catalogue,
            &definitions,
            &positions,
            &at,
            built_at,
            holder_cursor,
            market_cursor,
            collection,
            coverage,
        )?;
        let bytes = write_bundle(&snapshot);
        verify(&bytes, &snapshot)?;
        std::fs::write(path, &bytes).map_err(|error| format!("write {path}: {error}"))?;
        println!();
        println!("bundle      {path} · {} bytes", bytes.len());
        println!(
            "            verified {} rows · {} of {} evaluated achievements answered · built-at \
             {built_at}",
            snapshot.subjects.len(),
            snapshot
                .rarity
                .iter()
                .filter(|rarity| rarity.population > 0)
                .count(),
            definitions.len(),
        );
        println!("            holder cursor {holder_cursor} · market cursor {market_cursor}");
        println!(
            "            collection {} · {coverage:?} coverage",
            snapshot.collection
        );
    }

    Ok(())
}

/// Reopens the bytes just written and finds every row in them.
///
/// A writer that produced a file it cannot read back is the failure this catches, and it
/// catches it before the file is published rather than in a client. Every subject is looked
/// up by its own key, so both the binary-search path and the linear enterprise-address path
/// are exercised.
fn verify(bytes: &[u8], snapshot: &Snapshot) -> Result<(), String> {
    let bundle = Bundle::open(bytes).map_err(|error| format!("reopen: {error}"))?;
    if bundle.subject_count() as usize != snapshot.subjects.len() {
        return Err(format!(
            "reopen: {} subjects, wrote {}",
            bundle.subject_count(),
            snapshot.subjects.len()
        ));
    }
    for row in &snapshot.subjects {
        let found = bundle
            .find(&row.key)
            .map_err(|error| format!("find {}: {error}", row.key))?
            .ok_or_else(|| format!("find {}: written but not found", row.key))?;
        let mut expected: Vec<u16> = row.earned.iter().map(|entry| entry.achievement).collect();
        expected.sort_unstable();
        let actual: Vec<u16> = found.earned().map(|entry| entry.achievement).collect();
        if expected != actual {
            return Err(format!(
                "{}: wrote earned {expected:?}, read {actual:?}",
                row.key
            ));
        }
        if found.lots().count() != row.lots.len() {
            return Err(format!(
                "{}: wrote {} lots, read {}",
                row.key,
                row.lots.len(),
                found.lots().count()
            ));
        }
    }
    for (index, rarity) in snapshot.rarity.iter().enumerate() {
        let found = bundle
            .rarity(index as u16)
            .ok_or_else(|| format!("rarity {index} missing"))?;
        if found != *rarity {
            return Err(format!("rarity {index}: wrote {rarity:?}, read {found:?}"));
        }
    }
    Ok(())
}

/// One row of a trait snapshot, as `ncli ownership bundle --traits decoded --out` writes it.
#[derive(serde::Deserialize)]
struct TraitRow {
    asset_name_hex: String,
    traits: BTreeMap<String, Vec<String>>,
}

/// Seeds the store's metadata from a trait snapshot, joined on the on-chain asset name.
///
/// The store's unit table is the contract: a row for an asset the stream never saw is
/// counted, never interned, because a seeded unit no event moved would appear in `subjects`
/// as an asset with no history. Returns `(rows, matched, unmatched)`.
fn seed_traits(store: &mut MemoryStore, path: &str) -> Result<(usize, usize, usize), String> {
    let mut units: BTreeMap<String, UnitId> = BTreeMap::new();
    for subject in store.subjects().map_err(|error| format!("{error:?}"))? {
        for lot in store.lots(subject).map_err(|error| format!("{error:?}"))? {
            let name = store
                .token_of(lot.unit)
                .map_err(|error| format!("{error:?}"))?
                .and_then(|token| token.token_id().map(str::to_string));
            if let Some(name) = name {
                units.insert(name, lot.unit);
            }
        }
    }

    let file = std::fs::File::open(path).map_err(|error| format!("open {path}: {error}"))?;
    let mut rows = 0usize;
    let mut matched = 0usize;
    let mut unmatched = 0usize;
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|error| format!("read {path}: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        rows += 1;
        let row: TraitRow = serde_json::from_str(&line)
            .map_err(|error| format!("parse {path} line {rows}: {error}"))?;
        let Some(unit) = units.get(&row.asset_name_hex).copied() else {
            unmatched += 1;
            continue;
        };
        let pairs: Vec<(String, String)> = row
            .traits
            .into_iter()
            .flat_map(|(category, values)| {
                values
                    .into_iter()
                    .map(move |value| (category.clone(), value))
            })
            .collect();
        let borrowed: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(category, value)| (category.as_str(), value.as_str()))
            .collect();
        store.seed_traits(unit, &borrowed);
        matched += 1;
    }
    Ok((rows, matched, unmatched))
}

/// Turns the folded store into a snapshot, evaluating every subject against what this build
/// can answer.
///
/// The population is `0` for an achievement this build could not evaluate. That is the
/// difference between "nobody earned it" and "nobody was asked", and a bundle that wrote
/// `0 / 700` there would be claiming the first.
#[allow(clippy::too_many_arguments)]
fn build_snapshot(
    store: &MemoryStore,
    subjects: &[SubjectId],
    catalogue: &LoadedCatalogue,
    definitions: &[Definition<'_>],
    positions: &[usize],
    at: &Assessment,
    built_at: u64,
    holder_cursor: Slot,
    market_cursor: Slot,
    collection: &str,
    coverage: Coverage,
) -> Result<Snapshot, String> {
    let mut rows = Vec::with_capacity(subjects.len());
    let mut holders = vec![0u32; catalogue.len()];
    // Whether any subject produced an answer other than `Unavailable`. An achievement the
    // whole reading could not answer has no eligible population — writing 700 there would
    // claim a collection-wide miss.
    let mut answered = vec![false; catalogue.len()];

    for subject in subjects {
        let account = store
            .account_of(*subject)
            .map_err(|error| format!("{error:?}"))?
            .ok_or_else(|| format!("{subject:?} has no account"))?;
        let outcomes =
            evaluate(store, *subject, at, definitions).map_err(|error| format!("{error:?}"))?;
        let mut earned = Vec::new();
        for (index, outcome) in outcomes.iter().enumerate() {
            let Some(&achievement) = positions.get(index) else {
                return Err(format!("outcome {index} has no catalogue position"));
            };
            if !matches!(outcome.progress, Progress::Unavailable(_)) {
                answered[achievement] = true;
            }
            let Progress::Earned { since } = outcome.progress else {
                continue;
            };
            let valid_from = match since {
                EarnedSince::Exact(slot) | EarnedSince::AsAt { slot, .. } => slot,
            };
            holders[achievement] += 1;
            earned.push(Earned {
                achievement: achievement as u16,
                valid_from,
            });
        }

        let held = store.lots(*subject).map_err(|error| format!("{error:?}"))?;
        let lots = held
            .into_iter()
            .map(|holding| Lot {
                unit: holding.unit.0,
                quantity: holding.quantity,
                open: holding.open,
                acquired_at: holding.acquired_at,
                acquired_unix: holding.acquired_unix,
                disposed_at: holding.disposed_at,
                disposed_unix: holding.disposed_unix,
                from_mint: holding.acquired_by == AcquiredBy::Mint,
            })
            .collect();

        rows.push(SubjectRecord {
            key: account.address,
            earned,
            lots,
        });
    }

    let population = subjects.len() as u32;
    let rarity = (0..catalogue.len())
        .map(|index| Rarity {
            holders_with: holders[index],
            population: if catalogue.evaluated(index) && answered[index] {
                population
            } else {
                0
            },
        })
        .collect();

    Ok(Snapshot {
        built_at,
        collection: collection.to_string(),
        coverage,
        holder_cursor,
        market_cursor,
        catalogue_digest: catalogue_digest(
            (0..catalogue.len()).filter_map(|index| catalogue.id_at(index)),
        ),
        achievement_count: catalogue.len() as u16,
        rarity,
        subjects: rows,
    })
}

/// How the earned set lands across the population — the distribution report the design doc
/// asks for before any allocation rule, and the input to a holder-facing page.
fn print_standings(
    catalogue: &LoadedCatalogue,
    positions: &[usize],
    definitions: &[Definition<'_>],
    tally: &BTreeMap<&str, [u64; 4]>,
    per_holder: &[(u32, u32)],
) {
    let total = per_holder.len();
    if total == 0 {
        return;
    }
    fn percentile(values: &mut [u64], p: f64) -> u64 {
        values.sort_unstable();
        let index = ((values.len() as f64 - 1.0) * p).round() as usize;
        values[index]
    }
    let mut earned: Vec<u64> = per_holder.iter().map(|(c, f)| u64::from(c + f)).collect();
    let mut core: Vec<u64> = per_holder.iter().map(|(c, _)| u64::from(*c)).collect();
    let max = earned.iter().copied().max().unwrap_or(0);
    let median = percentile(&mut earned, 0.5);
    let p90 = percentile(&mut earned, 0.9);
    let median_core = percentile(&mut core, 0.5);
    let p90_core = percentile(&mut core, 0.9);
    let none_core = core.iter().filter(|count| **count == 0).count();
    let none_at_all = per_holder
        .iter()
        .filter(|(c, f)| *c == 0 && *f == 0)
        .count();

    println!();
    println!("standings   {total} subjects · earned median {median} · p90 {p90} · max {max}");
    println!(
        "            core median {median_core} · p90 {p90_core} · {none_core} with none · {none_at_all} with nothing"
    );

    // The catalogue's proposed gate: GTD at two core, FCFS at one core or three flavour.
    let gtd = per_holder.iter().filter(|(c, _)| *c >= 2).count();
    let fcfs = per_holder
        .iter()
        .filter(|(c, f)| *c < 2 && (*c >= 1 || *f >= 3))
        .count();
    let public = total - gtd - fcfs;
    let share = |count: usize| count as f64 * 100.0 / total as f64;
    println!(
        "            gate gtd {gtd} ({:.1}%) · fcfs {fcfs} ({:.1}%) · public {public} ({:.1}%)",
        share(gtd),
        share(fcfs),
        share(public)
    );

    let mut core_shares: Vec<(&str, f64)> = definitions
        .iter()
        .zip(positions)
        .filter(|(_, position)| catalogue.role_at(**position) == Some("core"))
        .map(|(definition, _)| (definition.id, tally[definition.id][0] as f64 / total as f64))
        .collect();
    core_shares.sort_by(|a, b| b.1.total_cmp(&a.1));
    let above: Vec<String> = core_shares
        .iter()
        .filter(|(_, share)| *share > 0.02)
        .map(|(id, share)| format!("{id} {:.1}%", share * 100.0))
        .collect();
    println!(
        "            core above 2% {}",
        match above.is_empty() {
            true => "none".to_string(),
            false => above.join(" · "),
        }
    );
    let rarest: Vec<String> = core_shares
        .iter()
        .rev()
        .take(4)
        .map(|(id, share)| format!("{id} {:.1}%", share * 100.0))
        .collect();
    println!("            core rarest   {}", rarest.join(" · "));
}

fn describe(outcome: &Outcome<'_>, store: &MemoryStore) -> Result<String, String> {
    let state = match outcome.progress {
        Progress::Earned { since } => match since {
            EarnedSince::Exact(slot) => format!("earned at {slot}"),
            EarnedSince::AsAt { slot, unix } => format!("earned as at {slot} ({unix})"),
        },
        Progress::Pending {
            have,
            need,
            eta_unix,
        } => {
            // Tenure is the only predicate measured in seconds, so report it that way.
            let scale = if outcome.id.starts_with("tenure") {
                format!(" ({:.0}d)", have as f64 / SECONDS_PER_DAY as f64)
            } else {
                String::new()
            };
            match eta_unix {
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
            Evidence::HeldAtOnce { units } => {
                let listed: Vec<String> = units
                    .iter()
                    .take(3)
                    .map(|unit| format!("{unit:?}"))
                    .collect();
                let more = if units.len() > 3 { ", …" } else { "" };
                format!(
                    "{} units held at once ({}{more})",
                    units.len(),
                    listed.join(", ")
                )
            }
            Evidence::Market {
                order,
                side,
                filled,
                at,
                unix,
            } => {
                let what = if *filled { "filled" } else { "placed" };
                let order = order.as_deref().unwrap_or("unkeyed");
                format!("{what} {side:?} {order} at {at:?} ({unix})")
            }
            Evidence::Movements { movements, units } => {
                format!("{movements} movements across {units} assets")
            }
        })
        .collect();
    if cited.is_empty() {
        Ok(format!("{:<14} {state}", outcome.id))
    } else {
        Ok(format!("{:<14} {state} — {}", outcome.id, cited.join("; ")))
    }
}
