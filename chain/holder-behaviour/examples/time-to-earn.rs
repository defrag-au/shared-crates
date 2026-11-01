//! What it costs in time to earn each achievement, from a cold start.
//!
//! The allocator answers "who is over the gate at the snapshot". This answers the other half:
//! if a partner holder turns up on day one, which achievements can they actually reach by the
//! end of the campaign? It reads the published bundle — every earned event carries the slot it
//! became true, and every lot carries both the slot and the unix time it arrived — so the
//! elapsed time is measured, not assumed.
//!
//! ```sh
//! cargo run --release -p holder-behaviour --example time-to-earn -- \
//!     .tmp/perps-status.bundle \
//!     --catalogue ~/code/defrag/cnft.dev-workers/config/achievements/perps.toml
//! ```
//!
//! Two readings, because one hides the other:
//!
//! - **Per achievement**, the days from a subject's first acquisition to the slot it earned it.
//!   A minimum much longer than the campaign window means the achievement is unreachable for a
//!   new holder, whatever the catalogue implies.
//! - **Core accumulation by cohort**, because a minter and a buyer do not take the same route to
//!   the GTD gate. Minters earn the provenance core at the mint; buyers have to earn their way
//!   in after arriving.
//!
//! The distribution is conditional on earning the achievement: it measures the fastest credible
//! route, not the probability that a new holder takes it. Behaviour is still an assumption.

mod support;

use std::collections::BTreeMap;
use std::process::ExitCode;

use holder_behaviour::{Bundle, Lot};
use support::catalogue::LoadedCatalogue;

const SECONDS_PER_DAY: f64 = 86_400.0;
const CORE_MAX: usize = 6;
const WINDOWS: [u32; 6] = [7, 14, 30, 60, 90, 180];

struct Args {
    bundle: String,
    catalogue: String,
    out: Option<String>,
}

#[derive(Default)]
struct Stat {
    earners: u32,
    /// Earners with no acquisition, so no start to measure from.
    no_lot: u32,
    /// Earners whose predicate the fold only knew was true at the reading slot.
    as_at: u32,
    days: Vec<f64>,
    /// The same timings for subjects whose first lot was bought, not minted.
    buyer_days: Vec<f64>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("time-to-earn: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let bytes =
        std::fs::read(&args.bundle).map_err(|error| format!("read {}: {error}", args.bundle))?;
    let bundle = Bundle::open(&bytes).map_err(|error| format!("open {}: {error}", args.bundle))?;
    let catalogue = LoadedCatalogue::load(&args.catalogue)?;

    let (slot_seconds, lot_pairs) = slot_seconds(&bundle)?;
    let holder_cursor = bundle.holder_cursor();
    let mut stats: Vec<Stat> = (0..catalogue.len()).map(|_| Stat::default()).collect();
    let mut cohorts: [u32; 2] = [0, 0];
    let mut core: BTreeMap<(bool, usize), Vec<f64>> = BTreeMap::new();
    let mut subject_count = 0u32;

    for subject in bundle.subjects() {
        let subject = subject.map_err(|error| format!("read subject: {error}"))?;
        subject_count += 1;
        let lots: Vec<Lot> = subject.lots().collect();
        let first = lots.iter().min_by_key(|lot| lot.acquired_at);
        if let Some(first) = first {
            cohorts[if first.from_mint { 0 } else { 1 }] += 1;
        }

        let mut core_slots: Vec<u64> = Vec::new();
        for earned in subject.earned() {
            let index = earned.achievement as usize;
            let Some(stat) = stats.get_mut(index) else {
                return Err(format!(
                    "bundle achievement #{index} is not in the catalogue"
                ));
            };
            stat.earners += 1;
            let is_core = catalogue.role_at(index) == Some("core");
            let recomputed = catalogue
                .predicate_days(index)
                .and_then(|days| true_span_slot(&lots, days, slot_seconds, holder_cursor));
            let timed_slot = match recomputed {
                Some(slot) => Some(slot),
                None if earned.valid_from == holder_cursor => None,
                None => Some(earned.valid_from),
            };
            match (timed_slot, first) {
                (Some(slot), Some(first)) => {
                    let days = elapsed_days(first, slot, slot_seconds);
                    stat.days.push(days);
                    if !first.from_mint {
                        stat.buyer_days.push(days);
                    }
                    if is_core {
                        core_slots.push(slot);
                    }
                }
                (Some(_), None) => stat.no_lot += 1,
                (None, _) => stat.as_at += 1,
            }
        }

        if let Some(first) = first {
            core_slots.sort_unstable();
            for (position, slot) in core_slots.iter().enumerate() {
                let count = position + 1;
                if count <= CORE_MAX {
                    let days = elapsed_days(first, *slot, slot_seconds);
                    core.entry((first.from_mint, count)).or_default().push(days);
                }
            }
        }
    }

    println!(
        "bundle      {} · {} bytes · {}",
        args.bundle,
        bytes.len(),
        bundle.collection()
    );
    println!(
        "catalogue   {:016x} · {} definitions · {} subjects",
        bundle.catalogue_digest(),
        catalogue.len(),
        subject_count
    );
    println!(
        "clock       {slot_seconds:.3}s per slot, from {lot_pairs} lot pairs · first acquisition is the start"
    );

    print_achievements(&catalogue, &stats);
    print_core(&core, &cohorts);
    print_windows(&catalogue, &stats);

    if let Some(path) = &args.out {
        write_csv(path, &catalogue, &stats)?;
        println!("wrote       {path}");
    }
    Ok(())
}

fn elapsed_days(first: &Lot, earned_at: u64, slot_seconds: f64) -> f64 {
    // An achievement can be earned before the first acquisition when it is about an offer or a
    // listing rather than a holding; clamping keeps the reading a lower bound instead of a
    // negative.
    let slots = earned_at.saturating_sub(first.acquired_at);
    slots as f64 * slot_seconds / SECONDS_PER_DAY
}

/// The slot a continuous-holding predicate actually became true, recomputed from the lots.
///
/// The evaluator reports `EarnedSince::AsAt` for these predicates, so the bundle's slot is the
/// reading, not the earn. The threshold is a duration, so the true slot is recoverable: the
/// first lot whose span reached it, plus the span.
fn true_span_slot(lots: &[Lot], days: u32, slot_seconds: f64, cursor: u64) -> Option<u64> {
    let need = (f64::from(days) * SECONDS_PER_DAY / slot_seconds).round() as u64;
    if need == 0 {
        return None;
    }
    lots.iter()
        .filter(|lot| {
            let end = lot.disposed_at.unwrap_or(cursor);
            end.saturating_sub(lot.acquired_at) >= need
        })
        .map(|lot| lot.acquired_at.saturating_add(need))
        .min()
}

fn slot_seconds(bundle: &Bundle<'_>) -> Result<(f64, usize), String> {
    let mut pairs: Vec<(u64, u64)> = Vec::new();
    for subject in bundle.subjects() {
        let subject = subject.map_err(|error| format!("read subject: {error}"))?;
        for lot in subject.lots() {
            if lot.acquired_at > 0 && lot.acquired_unix > 0 {
                pairs.push((lot.acquired_at, lot.acquired_unix));
            }
        }
    }
    pairs.sort_unstable();
    let measured = match (pairs.first(), pairs.last()) {
        (Some((first_slot, first_unix)), Some((last_slot, last_unix)))
            if last_slot > first_slot && last_unix > first_unix =>
        {
            let seconds = (last_unix - first_unix) as f64 / (last_slot - first_slot) as f64;
            (0.1..=60.0).contains(&seconds).then_some(seconds)
        }
        _ => None,
    };
    Ok((measured.unwrap_or(1.0), pairs.len()))
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let index = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[index]
}

fn days_text(value: f64) -> String {
    if value.is_nan() {
        "—".to_string()
    } else {
        format!("{value:.1}")
    }
}

fn percent_text(value: f64) -> String {
    if value.is_nan() {
        "—".to_string()
    } else {
        format!("{value:.1}%")
    }
}

fn print_achievements(catalogue: &LoadedCatalogue, stats: &[Stat]) {
    let mut rows: Vec<(usize, f64)> = stats
        .iter()
        .enumerate()
        .filter(|(_, stat)| stat.earners > 0)
        .map(|(index, stat)| {
            let mut days = stat.days.clone();
            days.sort_by(f64::total_cmp);
            (index, percentile(&days, 0.5))
        })
        .collect();
    rows.sort_by(|left, right| {
        left.1
            .total_cmp(&right.1)
            .then_with(|| catalogue.id_at(left.0).cmp(&catalogue.id_at(right.0)))
    });

    println!();
    println!("time to earn   (days from first acquisition, earners with a lot)");
    println!(
        "{:<20} {:<8} {:<10} {:>7} {:>7} {:>6} {:>6} {:>6} {:>7} {:>6} {:>6}",
        "achievement",
        "role",
        "axis",
        "earners",
        "no-lot",
        "as-at",
        "min",
        "p10",
        "median",
        "p90",
        "≤30d"
    );
    for (index, _) in rows {
        let stat = &stats[index];
        let mut days = stat.days.clone();
        days.sort_by(f64::total_cmp);
        let timed = days.len();
        let (min, p10, median, p90, within_30) = match timed {
            0 => (f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN),
            _ => {
                let within = days.iter().filter(|day| **day <= 30.0).count() as f64;
                (
                    days[0],
                    percentile(&days, 0.1),
                    percentile(&days, 0.5),
                    percentile(&days, 0.9),
                    within * 100.0 / timed as f64,
                )
            }
        };
        println!(
            "{:<20} {:<8} {:<10} {:>7} {:>7} {:>6} {:>6} {:>6} {:>7} {:>6} {:>6}",
            catalogue.id_at(index).unwrap_or("?"),
            catalogue.role_at(index).unwrap_or("?"),
            catalogue.axis_at(index).unwrap_or("?"),
            stat.earners,
            stat.no_lot,
            stat.as_at,
            days_text(min),
            days_text(p10),
            days_text(median),
            days_text(p90),
            percent_text(within_30)
        );
    }
    println!(
        "note        as-at means the fold only knew the predicate was true at the reading slot."
    );
    println!(
        "            `any_span_at_least` is recomputed from the lots; round-trip and holding counts"
    );
    println!("            are not, and are excluded rather than given a false time.");
}

fn print_core(core: &BTreeMap<(bool, usize), Vec<f64>>, cohorts: &[u32; 2]) {
    println!();
    println!(
        "core accumulation   (median days from first acquisition to the Nth core, sample in brackets)"
    );
    println!(
        "{:<8} {:>9} {:>13} {:>13} {:>13} {:>13} {:>13}",
        "cohort", "subjects", "core1", "core2", "core3", "core4", "core5"
    );
    for (label, minter) in [("minter", true), ("buyer", false)] {
        let subjects = cohorts[if minter { 0 } else { 1 }];
        print!("{label:<8} {subjects:>9}");
        for count in 1..=5 {
            let cell = match core.get(&(minter, count)) {
                Some(days) if !days.is_empty() => {
                    let mut days = days.clone();
                    days.sort_by(f64::total_cmp);
                    format!("{:.1} ({})", percentile(&days, 0.5), days.len())
                }
                _ => "—".to_string(),
            };
            print!(" {cell:>13}");
        }
        println!();
    }
    println!(
        "note        a cohort is set by the first lot: minted, or bought. Market-only subjects have no row."
    );
}

fn print_windows(catalogue: &LoadedCatalogue, stats: &[Stat]) {
    println!();
    println!("campaign window   (core achievements whose timing fits)");
    println!(
        "{:>7} {:>16} {:>12} {:>16} {:>12}",
        "window", "median ≤ W", "p90 ≤ W", "buyers ≤ W", "buyer p90"
    );
    for window in WINDOWS {
        let mut median_fits = 0;
        let mut p90_fits = 0;
        let mut buyer_fits = 0;
        let mut buyer_p90 = 0;
        for (index, stat) in stats.iter().enumerate() {
            if catalogue.role_at(index) != Some("core") {
                continue;
            }
            if !stat.days.is_empty() {
                let mut days = stat.days.clone();
                days.sort_by(f64::total_cmp);
                if percentile(&days, 0.5) <= f64::from(window) {
                    median_fits += 1;
                }
                if percentile(&days, 0.9) <= f64::from(window) {
                    p90_fits += 1;
                }
            }
            if !stat.buyer_days.is_empty() {
                let mut days = stat.buyer_days.clone();
                days.sort_by(f64::total_cmp);
                if percentile(&days, 0.5) <= f64::from(window) {
                    buyer_fits += 1;
                }
                if percentile(&days, 0.9) <= f64::from(window) {
                    buyer_p90 += 1;
                }
            }
        }
        println!("{window:>6}d {median_fits:>16} {p90_fits:>12} {buyer_fits:>16} {buyer_p90:>12}");
    }
    println!(
        "note        a long minimum means a new holder cannot earn it inside the window, however many they attempt."
    );
    println!(
        "            the buyer columns exclude mint-only achievements, which a partner holder can never earn."
    );
}

fn write_csv(path: &str, catalogue: &LoadedCatalogue, stats: &[Stat]) -> Result<(), String> {
    let mut out = String::from(
        "achievement,role,axis,earners,no_lot,as_at,min_days,p10_days,median_days,p90_days,within_30_pct\n",
    );
    for (index, stat) in stats.iter().enumerate() {
        if stat.earners == 0 {
            continue;
        }
        let mut days = stat.days.clone();
        days.sort_by(f64::total_cmp);
        let (min, p10, median, p90, within_30) = if days.is_empty() {
            (
                "-".to_string(),
                "-".to_string(),
                "-".to_string(),
                "-".to_string(),
                "-".to_string(),
            )
        } else {
            (
                format!("{:.1}", days[0]),
                format!("{:.1}", percentile(&days, 0.1)),
                format!("{:.1}", percentile(&days, 0.5)),
                format!("{:.1}", percentile(&days, 0.9)),
                format!(
                    "{:.1}",
                    days.iter().filter(|day| **day <= 30.0).count() as f64 * 100.0
                        / days.len() as f64
                ),
            )
        };
        out.push_str(&format!(
            "{},{},{},{},{},{as_at},{min},{p10},{median},{p90},{within_30}\n",
            catalogue.id_at(index).unwrap_or("?"),
            catalogue.role_at(index).unwrap_or("?"),
            catalogue.axis_at(index).unwrap_or("?"),
            stat.earners,
            stat.no_lot,
            as_at = stat.as_at
        ));
    }
    std::fs::write(path, out).map_err(|error| format!("write {path}: {error}"))
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let mut bundle = None;
    let mut catalogue = None;
    let mut out = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("time-to-earn <bundle> --catalogue <path> [--out <path.csv>]");
                std::process::exit(0);
            }
            "--catalogue" => {
                catalogue = Some(args.next().ok_or("--catalogue needs a path".to_string())?)
            }
            "--out" => out = Some(args.next().ok_or("--out needs a path".to_string())?),
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other if bundle.is_none() => bundle = Some(other.to_string()),
            other => return Err(format!("unexpected argument {other}")),
        }
    }
    Ok(Args {
        bundle: bundle.ok_or("give the path to a bundle")?,
        catalogue: catalogue.ok_or("--catalogue is required")?,
        out,
    })
}
