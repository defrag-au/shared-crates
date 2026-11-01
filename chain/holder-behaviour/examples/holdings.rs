//! Peak and current concurrent holdings per subject, from a published bundle.
//!
//! A holding-count ladder is only useful if its rungs are spread across the population. This
//! measures where they would land before any of them are added to a frozen catalogue: the
//! number of distinct units a subject held at one instant (the peak), and the number still
//! open at the snapshot.
//!
//! ```sh
//! cargo run --release -p holder-behaviour --example holdings -- .tmp/perps-v2.bundle
//! ```
//!
//! The peak is the number the achievement would key on: `post-it-overload` is already a
//! historical peak, and a ladder keyed to the current count would un-earn when a holder sells.
//! A unit held through two overlapping lots is one unit, not two.

use std::collections::{BTreeMap, BTreeSet};
use std::process::ExitCode;

use holder_behaviour::{Bundle, Lot};

const RUNGS: [u32; 4] = [1, 5, 25, 100];

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("holdings: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let path = std::env::args().nth(1).ok_or("give the path to a bundle")?;
    let bytes = std::fs::read(&path).map_err(|error| format!("read {path}: {error}"))?;
    let bundle = Bundle::open(&bytes).map_err(|error| format!("open {path}: {error}"))?;
    let cursor = bundle.holder_cursor();

    let mut peaks: Vec<u32> = Vec::new();
    let mut current: Vec<u32> = Vec::new();
    for subject in bundle.subjects() {
        let subject = subject.map_err(|error| format!("read subject: {error}"))?;
        let lots: Vec<Lot> = subject.lots().collect();
        peaks.push(peak_concurrent(&lots, cursor));
        current.push(current_distinct(&lots));
    }

    let total = peaks.len() as u32;
    println!(
        "bundle      {path} · {} bytes · {}",
        bytes.len(),
        bundle.collection()
    );
    println!("subjects    {total} wallets · holder cursor {cursor}");
    println!();
    println!(
        "{:<9} {:>9} {:>7} {:>9} {:>7}",
        "threshold", "peak", "share", "current", "share"
    );
    for rung in RUNGS {
        let peak = peaks.iter().filter(|count| **count >= rung).count() as u32;
        let open = current.iter().filter(|count| **count >= rung).count() as u32;
        println!(
            "{rung:<9} {peak:>9} {:>6.1}% {open:>9} {:>6.1}%",
            share(peak, total),
            share(open, total)
        );
    }

    println!();
    println!("distribution (peak concurrent units)");
    println!("{:<10} {:>9} {:>7}", "bucket", "subjects", "share");
    let buckets: [(&str, u32, u32); 7] = [
        ("0", 0, 0),
        ("1", 1, 1),
        ("2-4", 2, 4),
        ("5-9", 5, 9),
        ("10-24", 10, 24),
        ("25-49", 25, 49),
        ("50+", 50, u32::MAX),
    ];
    for (label, low, high) in buckets {
        let count = peaks
            .iter()
            .filter(|count| **count >= low && **count <= high)
            .count() as u32;
        println!("{label:<10} {count:>9} {:>6.1}%", share(count, total));
    }

    let max = peaks.iter().copied().max().unwrap_or(0);
    let with_any = peaks.iter().filter(|count| **count > 0).count() as u32;
    let mut held = peaks.clone();
    held.retain(|count| *count > 0);
    held.sort_unstable();
    let median = held
        .get(held.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(0);
    println!();
    println!("max peak    {max} · {with_any} subjects ever held one · median peak {median}");
    println!("note        each unit counts once, even when it arrives through overlapping lots.");
    println!(
        "            a rung at 1 is a participation ribbon; the interesting rungs are where the counts fall away."
    );
    Ok(())
}

fn peak_concurrent(lots: &[Lot], cursor: u64) -> u32 {
    #[derive(Clone, Copy)]
    struct Mark {
        slot: u64,
        start: bool,
        unit: u32,
    }

    let mut marks: Vec<Mark> = Vec::new();
    for lot in lots {
        let end = lot.disposed_at.unwrap_or(cursor);
        // A half-open interval that ends where it starts covers no instant.
        if end <= lot.acquired_at {
            continue;
        }
        marks.push(Mark {
            slot: lot.acquired_at,
            start: true,
            unit: lot.unit,
        });
        marks.push(Mark {
            slot: end,
            start: false,
            unit: lot.unit,
        });
    }
    // Ends before starts at a shared slot, so a sale and a purchase are never simultaneous.
    marks.sort_unstable_by(|left, right| {
        left.slot
            .cmp(&right.slot)
            .then_with(|| left.start.cmp(&right.start))
    });

    let mut open: BTreeMap<u32, u32> = BTreeMap::new();
    let mut peak = 0u32;
    for mark in marks {
        if mark.start {
            *open.entry(mark.unit).or_default() += 1;
        } else if let Some(count) = open.get_mut(&mark.unit) {
            *count -= 1;
            if *count == 0 {
                open.remove(&mark.unit);
            }
        }
        peak = peak.max(open.len() as u32);
    }
    peak
}

fn current_distinct(lots: &[Lot]) -> u32 {
    let open: BTreeSet<u32> = lots
        .iter()
        .filter(|lot| lot.open > 0)
        .map(|lot| lot.unit)
        .collect();
    open.len() as u32
}

fn share(count: u32, total: u32) -> f64 {
    if total == 0 {
        0.0
    } else {
        f64::from(count) * 100.0 / f64::from(total)
    }
}
