//! Proposes achievement points from measured rarity, for the curation pass.
//!
//! Points are **authored data**: this proposes a starting table, and a reviewer edits the
//! catalogue. It exists so the first pass is measured rather than guessed — every achievement
//! already has a rarity row in the bundle, and how few holders have it is a real signal.
//!
//! ```sh
//! cargo run --release -p holder-behaviour --example curate-points -- \
//!     --bundle .tmp/perps-status.bundle \
//!     --catalogue ~/code/defrag/cnft.dev-workers/config/achievements/perps.toml \
//!     --write
//! ```
//!
//! # The proposal
//!
//! `points = round(base[role] × √(population / holders_with) × axis_weight)`, clamped to
//! `[step, cap]` and rounded to `step`.
//!
//! The square root, not the ratio: a plain inverse would make the rarest achievement worth
//! hundreds of times the commonest, and one lucky drop would outrank a season of holding. Under
//! the root, an achievement half as common is worth 1.41× as much.
//!
//! # Two axes, not one
//!
//! Rarity on its own conflates two different things, because scarcity has two causes:
//!
//! - **Few holders could.** `three-crowns` is held by two wallets because the collection holds
//!   few such sets. Its share measures the collection's supply, not what the holder did — and a
//!   holder can buy their way into it.
//! - **Few holders did.** `hundred-day` is available to almost every holder and 413 of 769 took
//!   it. Its share measures a decision, sustained.
//!
//! Only the second is an achievement, so the catalogue's own `axis` — which already says what
//! kind of act an achievement asks for — multiplies the rarity weight. The defaults are in
//! [`axis_weight`]; `--axis name=weight` overrides one for a run.
//!
//! # Why the base is a thousand and not ten
//!
//! A score is read as an accumulation, not a mark out of ten. The weights are the same shape at
//! any base — only the size of the number a holder is told changes — so the base is set where the
//! total reads as something built up over a season: a committed holder lands in the thousands, and
//! a threshold is a number a reader recognises rather than one they can count on their fingers.
//!
//! # What this does not decide
//!
//! The thresholds. Points and thresholds are one decision made in two halves: run this, put the
//! numbers in the catalogue, then run `allocate` to see how many holders a candidate `gtd_score`
//! admits.

mod support;

use std::collections::BTreeMap;
use std::process::ExitCode;

use holder_behaviour::Bundle;

use support::catalogue::LoadedCatalogue;

/// What each axis says about *how* an achievement is attained, not just how rare it is.
///
/// Rarity has two causes — few could, or few did — and only the second is something a holder
/// earned. The catalogue already sorts achievements by the kind of act they ask for, so this maps
/// that to the correction: a sustained decision weighs more than a purchase.
fn axis_weight(axis: &str) -> Option<f64> {
    match axis {
        // Sustained, revocable, and the slowest: holding through a drawdown was a decision made
        // again every day it was hard.
        "conviction" => Some(1.4),
        // An act that could have failed: trading, timing, taking a side.
        "market" => Some(1.2),
        // Present at a moment. A choice, but a single one, and partly the luck of the clock.
        "provenance" => Some(0.9),
        // Which assets a wallet holds. Bought rather than earned: the ceiling is the collection's
        // supply, so rarity here overstates what the holder did.
        "curation" => Some(0.7),
        // Token behaviour, which this bundle does not measure at all.
        "stake" => Some(0.5),
        _ => None,
    }
}

/// One achievement, with what it measures and what it would be worth.
struct Row {
    index: usize,
    id: String,
    role: String,
    axis: String,
    holders: u32,
    population: u32,
    points: u32,
    /// Whether the bundle measured it. An unmeasured row has no share to weight, so it is worth
    /// its role's base — stated as a floor rather than presented as a measurement.
    measured: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("curate-points: {error}");
            ExitCode::FAILURE
        }
    }
}

struct Args {
    bundle: String,
    catalogue: String,
    base_core: f64,
    base_flavour: f64,
    cap: u32,
    /// The step every weight is rounded to, so a ledger reads as authored numbers rather than
    /// as the output of a square root.
    round: u32,
    /// Overrides of [`axis_weight`], by axis name.
    axes: BTreeMap<String, f64>,
    /// Rewrite the `points` lines in the catalogue. Off by default: printing is the review.
    write: bool,
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let catalogue = LoadedCatalogue::load(&args.catalogue)?;
    let bytes =
        std::fs::read(&args.bundle).map_err(|error| format!("read {}: {error}", args.bundle))?;
    let bundle = Bundle::open(&bytes).map_err(|error| format!("open {}: {error}", args.bundle))?;
    if usize::from(bundle.achievement_count()) != catalogue.len() {
        return Err(format!(
            "{} scores {} achievements but {} defines {}",
            args.bundle,
            bundle.achievement_count(),
            args.catalogue,
            catalogue.len()
        ));
    }

    let weight_of = |axis: &str| -> Result<f64, String> {
        if let Some(weight) = args.axes.get(axis) {
            return Ok(*weight);
        }
        axis_weight(axis).ok_or_else(|| {
            format!("axis {axis:?} has no attainment weight — pass --axis {axis}=<weight>")
        })
    };

    let mut rows = Vec::with_capacity(catalogue.len());
    for index in 0..catalogue.len() {
        let entry = catalogue
            .entry(index)
            .ok_or_else(|| format!("no catalogue entry at #{index}"))?;
        let rarity = bundle
            .rarity(index as u16)
            .ok_or_else(|| format!("no rarity row for #{index} ({})", entry.id))?;
        let base = match entry.role {
            "core" => args.base_core,
            "flavour" => args.base_flavour,
            role => return Err(format!("#{index} ({}) has role {role:?}", entry.id)),
        };
        let axis = weight_of(entry.axis)?;
        // A row with no population, or none holding it, has no measured reach to weight by. A
        // zero denominator is not "infinitely rare" — it is an achievement the fold never fired,
        // which is a gap in the source until someone says otherwise.
        let measured = rarity.population > 0 && rarity.holders_with > 0;
        let step = f64::from(args.round.max(1));
        let points = match measured {
            // A ratio, not a share: an achievement nobody has would divide by zero, and one
            // everybody has should be worth its base rather than nothing.
            true => {
                let ratio = f64::from(rarity.population) / f64::from(rarity.holders_with.max(1));
                let raw = base * ratio.sqrt() * axis;
                let rounded = (raw / step).round() * step;
                rounded.clamp(step, f64::from(args.cap)) as u32
            }
            false => ((base * axis) / step).round() as u32 * args.round.max(1),
        };
        rows.push(Row {
            index,
            id: entry.id.to_string(),
            role: entry.role.to_string(),
            axis: entry.axis.to_string(),
            holders: rarity.holders_with,
            population: rarity.population,
            points,
            measured,
        });
    }

    // Rarest first: the tail is where the editorial decisions are, and the head reads as one block.
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by(|left, right| {
        let (a, b) = (&rows[*left], &rows[*right]);
        (a.holders, a.id.as_str()).cmp(&(b.holders, b.id.as_str()))
    });

    println!(
        "{} achievements · {} subjects · base core {:.0} · base flavour {:.0} · cap {} · step {}",
        rows.len(),
        rows.first().map_or(0, |row| row.population),
        args.base_core,
        args.base_flavour,
        args.cap,
        args.round
    );
    let axes: BTreeMap<&str, f64> = rows
        .iter()
        .map(|row| (row.axis.as_str(), weight_of(&row.axis).unwrap_or(0.0)))
        .collect();
    println!(
        "attainment ×{}",
        axes.iter()
            .map(|(axis, weight)| format!(" {axis} {weight:.1}"))
            .collect::<String>()
    );
    println!(
        "\n{:<28} {:<10} {:<8} {:>8} {:>8} {:>7}",
        "id", "axis", "role", "holders", "share", "points"
    );
    for index in &order {
        let row = &rows[*index];
        match row.measured {
            true => {
                let share = f64::from(row.holders) / f64::from(row.population);
                println!(
                    "{:<28} {:<10} {:<8} {:>8} {:>7.2}% {:>7}",
                    row.id,
                    row.axis,
                    row.role,
                    row.holders,
                    share * 100.0,
                    row.points
                );
            }
            false => println!(
                "{:<28} {:<10} {:<8} {:>8} {:>8} {:>7}*",
                row.id, row.axis, row.role, "—", "—", row.points
            ),
        }
    }
    let unmeasured = rows.iter().filter(|row| !row.measured).count();
    if unmeasured > 0 {
        println!(
            "\n* {unmeasured} with no measured reach in this bundle: worth the role's base, a \
             floor rather than a measurement. Nothing can earn one until the source answers."
        );
    }

    let role_total = |role: &str| -> u32 {
        rows.iter()
            .filter(|row| row.role == role)
            .map(|row| row.points)
            .sum()
    };
    println!(
        "\ncore {} · total {} · flavour {} · total {}",
        rows.iter().filter(|row| row.role == "core").count(),
        role_total("core"),
        rows.iter().filter(|row| row.role == "flavour").count(),
        role_total("flavour"),
    );

    if args.write {
        let written = write_points(&args.catalogue, &rows)?;
        println!("\n wrote {written} points into {}", args.catalogue);
    } else {
        println!("\n# Points by catalogue index. Paste each beside its `id`, or pass --write.\n");
        for row in &rows {
            println!("# {:>2} {:<28} {}", row.index, row.id, row.points);
        }
    }
    Ok(())
}

/// Rewrite each achievement's `points` line, in place, by id.
///
/// The catalogue is hand-written and carries prose; this touches one line per achievement and
/// nothing else. An id it does not know is left alone, so the tool can price a catalogue that has
/// grown a new entry without deleting it.
fn write_points(path: &str, rows: &[Row]) -> Result<usize, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("read {path}: {error}"))?;
    let by_id: BTreeMap<&str, u32> = rows
        .iter()
        .map(|row| (row.id.as_str(), row.points))
        .collect();
    let mut out: Vec<String> = Vec::with_capacity(text.lines().count() + rows.len());
    let mut pending: Option<u32> = None;
    let mut written = 0;
    for line in text.lines() {
        if let Some(points) = pending.take() {
            // Replace an existing line, or insert one after the id when the entry has none yet.
            let replacing = line.starts_with("points = ");
            out.push(format!("points = {points}"));
            written += 1;
            if replacing {
                continue;
            }
        }
        if let Some(id) = line.strip_prefix("id = \"") {
            pending = by_id.get(id.trim_end_matches('"')).copied();
        }
        out.push(line.to_string());
    }
    let mut rewritten = out.join("\n");
    rewritten.push('\n');
    std::fs::write(path, rewritten).map_err(|error| format!("write {path}: {error}"))?;
    Ok(written)
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let mut parsed = Args {
        bundle: String::new(),
        catalogue: String::new(),
        base_core: 1_000.0,
        base_flavour: 500.0,
        cap: 10_000,
        round: 50,
        axes: BTreeMap::new(),
        write: false,
    };
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--bundle" => parsed.bundle = value()?,
            "--catalogue" => parsed.catalogue = value()?,
            "--base-core" => {
                parsed.base_core = value()?
                    .parse()
                    .map_err(|error| format!("--base-core: {error}"))?
            }
            "--base-flavour" => {
                parsed.base_flavour = value()?
                    .parse()
                    .map_err(|error| format!("--base-flavour: {error}"))?
            }
            "--cap" => {
                parsed.cap = value()?
                    .parse()
                    .map_err(|error| format!("--cap: {error}"))?
            }
            "--round" => {
                parsed.round = value()?
                    .parse()
                    .map_err(|error| format!("--round: {error}"))?
            }
            "--axis" => {
                let pair = value()?;
                let (name, weight) = pair
                    .split_once('=')
                    .ok_or_else(|| format!("--axis {pair}: expected name=weight"))?;
                let weight: f64 = weight
                    .parse()
                    .map_err(|error| format!("--axis {pair}: {error}"))?;
                parsed.axes.insert(name.to_string(), weight);
            }
            "--write" => parsed.write = true,
            other => return Err(format!("unknown flag {other}")),
        }
    }
    if parsed.bundle.is_empty() || parsed.catalogue.is_empty() {
        return Err(
            "usage: curate-points --bundle <path> --catalogue <path> [--base-core N] \
             [--base-flavour N] [--cap N] [--round N] [--axis name=weight] [--write]"
                .to_string(),
        );
    }
    Ok(parsed)
}
