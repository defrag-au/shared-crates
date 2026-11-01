//! Models a gated allocation over a published bundle.
//!
//! The bundle already carries every subject's earned set and each achievement's rarity, so
//! this reads it and applies a policy: no refold, no stream, no identity store. It answers the
//! question a mint has to settle before it opens — under this policy, how many subjects land in
//! each phase, and does the guaranteed phase fit inside its reserve?
//!
//! ```sh
//! cargo run --release -p holder-behaviour --example allocate -- \
//!     .tmp/perps.bundle \
//!     --catalogue ~/code/defrag/cnft.dev-workers/config/achievements/perps.toml \
//!     --gtd-core 4 --fcfs-core 2 --fcfs-flavour 3 \
//!     --gtd-reserve 3000 --fcfs-reserve 1500 --public-reserve 1500
//! ```
//!
//! What counts is the pipeline the design doc names: core achievements are never filtered;
//! a flavour achievement above `rarity_floor` does not count; a subject keeps its
//! `count_rarest` rarest remaining flavour achievements.
//!
//! Three caveats it prints rather than hides:
//!
//! - A subject is a wallet, not an identity. Only wallets that establish an anchor can be
//!   allocated, and an identity that unions wallets can only gain achievements. The counts
//!   are therefore a floor on qualifying people, not a forecast of the linked population.
//! - Every eligible subject is assumed to take the full `per_identity` cap. For a free mint
//!   that is the right upper bound for a guaranteed phase, which must fit its reserve.
//! - The token axis is absent from this bundle, and the partner collections are a separate
//!   population with no bundle at all — pass their measured holder sets with `--partners`
//!   and their pressure is a turnout sweep, not a count. Their crossover with Perps cannot
//!   be measured before bonding.

mod support;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::process::ExitCode;

use holder_behaviour::{Bundle, Lot, catalogue_digest};
use support::catalogue::{Allocation, LoadedCatalogue};

const DEFAULT_GTD_CORE: u32 = 4;
const DEFAULT_FCFS_CORE: u32 = 2;
const DEFAULT_FCFS_FLAVOUR: u32 = 3;
const DEFAULT_CAP: u32 = 3;
const DEFAULT_FLOOR: f64 = 0.02;
const DEFAULT_RAREST: u32 = 3;
const DEFAULT_GTD_RESERVE: u32 = 3_000;
const DEFAULT_FCFS_RESERVE: u32 = 1_500;
const DEFAULT_PUBLIC_RESERVE: u32 = 1_500;

struct Args {
    bundle: String,
    catalogue: String,
    gtd_core: Option<u32>,
    fcfs_core: Option<u32>,
    fcfs_flavour: Option<u32>,
    cap: Option<u32>,
    floor: Option<f64>,
    rarest: Option<u32>,
    gtd_reserve: u32,
    fcfs_reserve: u32,
    public_reserve: u32,
    sweep_core: u32,
    sweep_flavour: u32,
    /// Measured holder sets, one `holder_census --out` JSONL per partner collection.
    partners: Vec<String>,
    out: Option<String>,
}

#[derive(Clone, Copy)]
struct Reserves {
    gtd: u32,
    fcfs: u32,
    public: u32,
}

impl Reserves {
    fn total(self) -> u32 {
        self.gtd + self.fcfs + self.public
    }
}

/// One subject's inputs to the policy, computed once for every threshold sweep.
struct Counts {
    core: u32,
    /// Flavour achievements that clear the floor, rarest first, already capped.
    credits: Vec<u16>,
    /// The most distinct units held at one instant — the ladder's input.
    peak: u32,
}

#[derive(Default)]
struct Tally {
    gtd: u32,
    fcfs: u32,
    ungated: u32,
    fcfs_by_core: u32,
    fcfs_by_flavour: u32,
    one_core_short: u32,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("allocate: {error}");
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

    let digest = catalogue_digest(catalogue.ids());
    if digest != bundle.catalogue_digest() {
        return Err(format!(
            "bundle was built against catalogue {:016x}, not {digest:016x} — rebuild the bundle",
            bundle.catalogue_digest()
        ));
    }

    let catalogue_policy = catalogue.allocation();
    let policy = resolve_policy(&args, catalogue_policy);
    validate(&policy)?;
    let reserves = Reserves {
        gtd: args.gtd_reserve,
        fcfs: args.fcfs_reserve,
        public: args.public_reserve,
    };

    println!(
        "bundle      {} · {} bytes · {}",
        args.bundle,
        bytes.len(),
        bundle.collection()
    );
    println!(
        "built       {} · coverage {:?}",
        bundle.built_at(),
        bundle.coverage()
    );
    println!(
        "catalogue   {digest:016x} · {} definitions · {} evaluable",
        catalogue.len(),
        catalogue.definitions().len()
    );
    println!(
        "subjects    {} wallets (not identities)",
        bundle.subject_count()
    );
    println!(
        "policy      gtd core≥{} · fcfs core≥{} or flavour≥{} · floor {:.1}% · rarest {} · cap {}",
        policy.gtd_core,
        policy.fcfs_core,
        policy.fcfs_flavour,
        policy.rarity_floor * 100.0,
        policy.count_rarest,
        policy.per_identity
    );
    println!(
        "reserve     gtd {} · fcfs {} · public {} = {}",
        reserves.gtd,
        reserves.fcfs,
        reserves.public,
        reserves.total()
    );
    if catalogue_policy.is_none() {
        println!("note        the catalogue carries no [allocation]; defaults were used");
    }

    let counts = counts_for(&bundle, &catalogue, policy)?;
    let total = counts.len() as u32;
    let tally = tally(&counts, policy);
    print_tally(total, policy, reserves, &tally);
    print_gtd_frontier(&counts, policy, reserves, args.sweep_core);
    print_fcfs_frontier(&counts, policy, reserves, args.sweep_flavour);
    print_floor_sweep(&bundle, &catalogue, policy, total)?;
    print_rarest_sweep(&bundle, &catalogue, policy, total)?;
    print_cap_table(policy, reserves, &tally);
    print_ladder(&counts, policy);
    print_partners(&args.partners, policy, reserves, &tally)?;
    print_caveats();

    if let Some(path) = &args.out {
        write_grid(
            path,
            &counts,
            policy,
            reserves,
            args.sweep_core,
            args.sweep_flavour,
        )?;
        println!("wrote       {path}");
    }
    Ok(())
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let mut bundle = None;
    let mut catalogue = None;
    let mut gtd_core = None;
    let mut fcfs_core = None;
    let mut fcfs_flavour = None;
    let mut cap = None;
    let mut floor = None;
    let mut rarest = None;
    let mut gtd_reserve = DEFAULT_GTD_RESERVE;
    let mut fcfs_reserve = DEFAULT_FCFS_RESERVE;
    let mut public_reserve = DEFAULT_PUBLIC_RESERVE;
    let mut sweep_core = 10;
    let mut sweep_flavour = 6;
    let mut partners = Vec::new();
    let mut out = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "--catalogue" => catalogue = Some(next(&mut args, "--catalogue")?),
            "--gtd-core" => gtd_core = Some(number(&mut args, "--gtd-core")?),
            "--fcfs-core" => fcfs_core = Some(number(&mut args, "--fcfs-core")?),
            "--fcfs-flavour" => fcfs_flavour = Some(number(&mut args, "--fcfs-flavour")?),
            "--cap" => cap = Some(number(&mut args, "--cap")?),
            "--floor" => floor = Some(decimal(&mut args, "--floor")?),
            "--rarest" => rarest = Some(number(&mut args, "--rarest")?),
            "--gtd-reserve" => gtd_reserve = number(&mut args, "--gtd-reserve")?,
            "--fcfs-reserve" => fcfs_reserve = number(&mut args, "--fcfs-reserve")?,
            "--public-reserve" => public_reserve = number(&mut args, "--public-reserve")?,
            "--sweep-core" => sweep_core = number(&mut args, "--sweep-core")?,
            "--sweep-flavour" => sweep_flavour = number(&mut args, "--sweep-flavour")?,
            "--partners" => partners.push(next(&mut args, "--partners")?),
            "--out" => out = Some(next(&mut args, "--out")?),
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other if bundle.is_none() => bundle = Some(other.to_string()),
            other => return Err(format!("unexpected argument {other}")),
        }
    }

    Ok(Args {
        bundle: bundle.ok_or("give the path to a bundle")?,
        catalogue: catalogue.ok_or("--catalogue is required")?,
        gtd_core,
        fcfs_core,
        fcfs_flavour,
        cap,
        floor,
        rarest,
        gtd_reserve,
        fcfs_reserve,
        public_reserve,
        sweep_core,
        sweep_flavour,
        partners,
        out,
    })
}

fn next(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn number(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<u32, String> {
    let value = next(args, flag)?;
    value
        .parse()
        .map_err(|_| format!("{flag} takes a whole number, not {value}"))
}

fn decimal(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<f64, String> {
    let value = next(args, flag)?;
    value
        .parse()
        .map_err(|_| format!("{flag} takes a decimal, not {value}"))
}

fn print_help() {
    println!("allocate <bundle> --catalogue <path> [options]");
    println!();
    println!("  --gtd-core N        core achievements for GTD (catalogue, else 4)");
    println!("  --fcfs-core N       core achievements for FCFS (catalogue, else 2)");
    println!("  --fcfs-flavour N    flavour achievements for FCFS (catalogue, else 3)");
    println!("  --cap N             mints per identity (catalogue, else 3)");
    println!("  --floor F           rarity floor as a share (catalogue, else 0.02)");
    println!("  --rarest N          flavour achievements that count (catalogue, else 3)");
    println!("  --gtd-reserve N     GTD supply (default {DEFAULT_GTD_RESERVE})");
    println!("  --fcfs-reserve N    FCFS supply (default {DEFAULT_FCFS_RESERVE})");
    println!("  --public-reserve N  public supply (default {DEFAULT_PUBLIC_RESERVE})");
    println!("  --sweep-core N      highest GTD core threshold shown (default 10)");
    println!("  --sweep-flavour N   highest FCFS flavour threshold shown (default 6)");
    println!("  --partners <path>   a partner holder set from `holder_census --out`, repeatable");
    println!("  --out <path.csv>    write the threshold grid");
}

fn resolve_policy(args: &Args, from_catalogue: Option<Allocation>) -> Allocation {
    let base = from_catalogue.unwrap_or(Allocation {
        gtd_core: DEFAULT_GTD_CORE,
        fcfs_core: DEFAULT_FCFS_CORE,
        fcfs_flavour: DEFAULT_FCFS_FLAVOUR,
        per_identity: DEFAULT_CAP,
        rarity_floor: DEFAULT_FLOOR,
        count_rarest: DEFAULT_RAREST,
    });
    Allocation {
        gtd_core: args.gtd_core.unwrap_or(base.gtd_core),
        fcfs_core: args.fcfs_core.unwrap_or(base.fcfs_core),
        fcfs_flavour: args.fcfs_flavour.unwrap_or(base.fcfs_flavour),
        per_identity: args.cap.unwrap_or(base.per_identity),
        rarity_floor: args.floor.unwrap_or(base.rarity_floor),
        count_rarest: args.rarest.unwrap_or(base.count_rarest),
    }
}

fn validate(policy: &Allocation) -> Result<(), String> {
    if !(0.0..=1.0).contains(&policy.rarity_floor) {
        return Err(format!(
            "--floor takes a share in 0..=1, not {}",
            policy.rarity_floor
        ));
    }
    if policy.count_rarest == 0 {
        return Err("--rarest must be at least 1".to_string());
    }
    if policy.per_identity == 0 {
        return Err("--cap must be at least 1".to_string());
    }
    Ok(())
}

fn counts_for(
    bundle: &Bundle<'_>,
    catalogue: &LoadedCatalogue,
    policy: Allocation,
) -> Result<Vec<Counts>, String> {
    let mut subjects = Vec::with_capacity(bundle.subject_count() as usize);
    let cursor = bundle.holder_cursor();
    for subject in bundle.subjects() {
        let subject = subject.map_err(|error| format!("read subject: {error}"))?;
        let lots: Vec<Lot> = subject.lots().collect();
        let peak = peak_concurrent(&lots, cursor);
        let mut core = 0u32;
        let mut flavour: Vec<(f64, u16)> = Vec::new();
        for earned in subject.earned() {
            let index = earned.achievement;
            match catalogue.role_at(index as usize) {
                Some("core") => core += 1,
                Some("flavour") => {
                    let rarity = bundle.rarity(index).ok_or_else(|| {
                        format!("bundle has no rarity row for achievement #{index}")
                    })?;
                    if rarity.population == 0 {
                        return Err(format!(
                            "{} earned #{index}, which the bundle marks unmeasured",
                            subject.key_str()
                        ));
                    }
                    let share = f64::from(rarity.holders_with) / f64::from(rarity.population);
                    if share <= policy.rarity_floor {
                        flavour.push((share, index));
                    }
                }
                Some(role) => {
                    return Err(format!(
                        "achievement #{index} has role {role:?}, which is neither core nor flavour"
                    ));
                }
                None => {
                    return Err(format!(
                        "bundle achievement #{index} is not in this catalogue"
                    ));
                }
            }
        }
        flavour.sort_by(|left, right| left.0.total_cmp(&right.0));
        flavour.truncate(policy.count_rarest as usize);
        subjects.push(Counts {
            core,
            credits: flavour.into_iter().map(|(_, index)| index).collect(),
            peak,
        });
    }
    Ok(subjects)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Gtd,
    Fcfs,
    Ungated,
}

fn phase(subject: &Counts, policy: Allocation) -> Phase {
    if subject.core >= policy.gtd_core {
        Phase::Gtd
    } else if subject.core >= policy.fcfs_core
        || (policy.fcfs_flavour > 0 && subject.credits.len() as u32 >= policy.fcfs_flavour)
    {
        Phase::Fcfs
    } else {
        Phase::Ungated
    }
}

fn tally(counts: &[Counts], policy: Allocation) -> Tally {
    let mut tally = Tally::default();
    for subject in counts {
        if subject.core < policy.gtd_core && subject.core + 1 == policy.gtd_core {
            tally.one_core_short += 1;
        }
        match phase(subject, policy) {
            Phase::Gtd => tally.gtd += 1,
            Phase::Fcfs => {
                tally.fcfs += 1;
                if subject.core >= policy.fcfs_core {
                    tally.fcfs_by_core += 1;
                } else {
                    tally.fcfs_by_flavour += 1;
                }
            }
            Phase::Ungated => tally.ungated += 1,
        }
    }
    tally
}

fn share_of(count: u32, total: u32) -> f64 {
    if total == 0 {
        0.0
    } else {
        f64::from(count) * 100.0 / f64::from(total)
    }
}

/// The most distinct units held at one instant, from the lots' half-open intervals.
///
/// Ends sort before starts at a shared slot, so a sale and a purchase are never simultaneous;
/// a unit held through two overlapping lots is one unit.
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

fn print_ladder(counts: &[Counts], policy: Allocation) {
    println!();
    println!("holding ladder (peak concurrent units, before any ladder achievement exists)");
    println!(
        "{:>7} {:>9} {:>7} {:>7} {:>8} {:>7}",
        "peak≥", "subjects", "gtd", "fcfs", "ungated", "share"
    );
    let total = counts.len() as u32;
    for rung in [1u32, 5, 25, 100] {
        let selected: Vec<&Counts> = counts
            .iter()
            .filter(|subject| subject.peak >= rung)
            .collect();
        let mut gtd = 0;
        let mut fcfs = 0;
        let mut ungated = 0;
        for subject in &selected {
            match phase(subject, policy) {
                Phase::Gtd => gtd += 1,
                Phase::Fcfs => fcfs += 1,
                Phase::Ungated => ungated += 1,
            }
        }
        println!(
            "{rung:>7} {:>9} {gtd:>7} {fcfs:>7} {ungated:>8} {:>6.1}%",
            selected.len(),
            share_of(selected.len() as u32, total)
        );
    }
    println!(
        "note        this does not add a path; it shows which part of the ladder is already gated."
    );
}

fn print_tally(total: u32, policy: Allocation, reserves: Reserves, tally: &Tally) {
    println!();
    println!(
        "{:<11} {:>8} {:>7} {:>9} {:>9} {:>10} {:>7}",
        "phase", "subjects", "share", "demand", "reserve", "headroom", "fill"
    );
    for (name, count, reserve, note) in [
        ("gtd", tally.gtd, reserves.gtd, ""),
        ("fcfs", tally.fcfs, reserves.fcfs, ""),
        (
            "ungated",
            tally.ungated,
            reserves.public,
            "  (holders only)",
        ),
    ] {
        let share = share_of(count, total);
        let demand = count * policy.per_identity;
        let headroom = i64::from(reserve) - i64::from(demand);
        let fill = f64::from(demand) * 100.0 / f64::from(reserve.max(1));
        println!(
            "{name:<11} {count:>8} {share:>6.1}% {demand:>9} {reserve:>9} {headroom:>+10} {fill:>6.1}%{note}"
        );
    }
    println!();
    println!(
        "fcfs        {} by core · {} by flavour (after the floor and the rarest cap)",
        tally.fcfs_by_core, tally.fcfs_by_flavour
    );
    if policy.gtd_core > 0 {
        println!(
            "near-miss   {} subjects are one core short of GTD (core {})",
            tally.one_core_short,
            policy.gtd_core - 1
        );
    }
}

fn print_gtd_frontier(counts: &[Counts], policy: Allocation, reserves: Reserves, max_core: u32) {
    let total = counts.len() as u32;
    println!();
    println!("gtd frontier (fcfs and floor fixed)");
    println!(
        "{:>4} {:>8} {:>7} {:>9} {:>10}",
        "core", "gtd", "share", "demand", "headroom"
    );
    for core in 0..=max_core {
        let candidate = Allocation {
            gtd_core: core,
            ..policy
        };
        let tally = tally(counts, candidate);
        let demand = tally.gtd * policy.per_identity;
        let headroom = i64::from(reserves.gtd) - i64::from(demand);
        println!(
            "{core:>4} {:>8} {:>6.1}% {demand:>9} {headroom:>+10}",
            tally.gtd,
            share_of(tally.gtd, total)
        );
    }
}

fn print_fcfs_frontier(
    counts: &[Counts],
    policy: Allocation,
    reserves: Reserves,
    max_flavour: u32,
) {
    let total = counts.len() as u32;
    println!();
    println!("fcfs frontier (gtd core≥{} fixed)", policy.gtd_core);
    println!(
        "{:>7} {:>8} {:>7} {:>9} {:>8}",
        "flavour", "fcfs", "share", "demand", "oversub"
    );
    for flavour in 0..=max_flavour {
        let candidate = Allocation {
            fcfs_flavour: flavour,
            ..policy
        };
        let tally = tally(counts, candidate);
        let demand = tally.fcfs * policy.per_identity;
        let oversub = f64::from(demand) / f64::from(reserves.fcfs.max(1));
        println!(
            "{flavour:>7} {:>8} {:>6.1}% {demand:>9} {oversub:>7.2}x",
            tally.fcfs,
            share_of(tally.fcfs, total)
        );
    }
    let chosen = tally(counts, policy);
    if chosen.fcfs_by_flavour == 0 {
        println!(
            "note        the flavour path adds nobody at this max share — all {} are by core",
            chosen.fcfs_by_core
        );
    }
}

fn print_floor_sweep(
    bundle: &Bundle<'_>,
    catalogue: &LoadedCatalogue,
    policy: Allocation,
    total: u32,
) -> Result<(), String> {
    println!();
    println!(
        "max share (fcfs at core≥{} or flavour≥{}; lower is stricter)",
        policy.fcfs_core, policy.fcfs_flavour
    );
    println!("{:>9} {:>8} {:>7}", "max share", "fcfs", "share");
    for floor in [0.01, 0.02, 0.05, 0.10, 0.25, 0.50, 1.0] {
        let candidate = Allocation {
            rarity_floor: floor,
            ..policy
        };
        let counts = counts_for(bundle, catalogue, candidate)?;
        let tally = tally(&counts, candidate);
        println!(
            "{:>8.1}% {:>8} {:>6.1}%",
            floor * 100.0,
            tally.fcfs,
            share_of(tally.fcfs, total)
        );
    }
    Ok(())
}

fn print_rarest_sweep(
    bundle: &Bundle<'_>,
    catalogue: &LoadedCatalogue,
    policy: Allocation,
    total: u32,
) -> Result<(), String> {
    println!();
    println!(
        "rarest cap (fcfs at core≥{} or flavour≥{})",
        policy.fcfs_core, policy.fcfs_flavour
    );
    println!("{:>7} {:>8} {:>7}", "rarest", "fcfs", "share");
    for rarest in 1..=5 {
        let candidate = Allocation {
            count_rarest: rarest,
            ..policy
        };
        let counts = counts_for(bundle, catalogue, candidate)?;
        let tally = tally(&counts, candidate);
        println!(
            "{rarest:>7} {:>8} {:>6.1}%",
            tally.fcfs,
            share_of(tally.fcfs, total)
        );
    }
    Ok(())
}

fn print_cap_table(policy: Allocation, reserves: Reserves, tally: &Tally) {
    println!();
    println!("identities needed to fill the gated phases at 100% take-up");
    println!(
        "{:>3} {:>9} {:>6} {:>10} {:>6} {:>11} {:>6}",
        "cap", "gtd need", "have", "fcfs need", "have", "total need", "have"
    );
    for cap in 1..=8 {
        let gtd_need = reserves.gtd.div_ceil(cap);
        let fcfs_need = reserves.fcfs.div_ceil(cap);
        println!(
            "{cap:>3} {gtd_need:>9} {:>6} {fcfs_need:>10} {:>6} {:>11} {:>6}",
            tally.gtd,
            tally.fcfs,
            gtd_need + fcfs_need,
            tally.gtd + tally.fcfs
        );
    }
    println!(
        "cap         policy is {} per identity · gtd demand {} of {} · fcfs demand {} of {}",
        policy.per_identity,
        tally.gtd * policy.per_identity,
        reserves.gtd,
        tally.fcfs * policy.per_identity,
        reserves.fcfs
    );
}

fn print_caveats() {
    println!();
    println!("caveats");
    println!(
        "  - subjects are wallets, not identities: only those that establish an anchor can be"
    );
    println!(
        "    allocated, and an identity that unions wallets can only gain, so these counts are"
    );
    println!("    a floor on qualifying people.");
    println!(
        "  - the token axis is absent from this bundle, and a partner holder is an EVM wallet with"
    );
    println!(
        "    no Perps history of its own: a partner who also scores needs a linked Cardano wallet,"
    );
    println!("    which is the crossover this cannot measure before bonding.");
    println!(
        "  - demand assumes every eligible subject takes the full cap, which is the upper bound a"
    );
    println!("    guaranteed phase must fit.");
}

/// Every partner collection's measured holders, unioned into one reach picture.
///
/// A partner wallet is not a bundle subject: it holds no Perps history, so it earns no
/// Perps achievement. What it can do is anchor an identity, which makes the count of
/// distinct wallets the pool a turnout applies to.
struct PartnerBook {
    /// One entry per collection: its label and how many holders it has.
    collections: Vec<(String, u32)>,
    /// Per wallet: how many partner collections it holds, and how many assets in total.
    wallets: BTreeMap<String, PartnerWallet>,
}

#[derive(Default, Clone, Copy)]
struct PartnerWallet {
    collections: u32,
    assets: u32,
}

/// A line of `holder_census --out`.
#[derive(serde::Deserialize)]
struct HeldLine {
    address: String,
    balance: u32,
}

/// Turnout shares the partner pressure is shown at. The pool is large enough that a single
/// number would imply a forecast this cannot make.
const TURNOUTS: [f64; 7] = [0.01, 0.025, 0.05, 0.10, 0.25, 0.50, 1.00];

fn print_partners(
    paths: &[String],
    policy: Allocation,
    reserves: Reserves,
    tally: &Tally,
) -> Result<(), String> {
    if paths.is_empty() {
        println!();
        println!(
            "partners    none given — pass --partners <holders.jsonl> for each partner collection"
        );
        return Ok(());
    }

    let book = load_partners(paths)?;
    println!();
    println!("partner collections (measured from Transfer logs, not OpenSea's owner count)");
    println!("{:<46} {:>9}", "collection", "holders");
    for (label, holders) in &book.collections {
        println!("{label:<46} {holders:>9}");
    }

    let wallets = book.wallets.len() as u32;
    let in_two = book
        .wallets
        .values()
        .filter(|wallet| wallet.collections >= 2)
        .count() as u32;
    let in_three = book
        .wallets
        .values()
        .filter(|wallet| wallet.collections >= 3)
        .count() as u32;
    println!();
    println!("partner reach");
    println!("  wallets            {wallets}");
    println!("  in ≥2 collections  {in_two}");
    println!("  in ≥3 collections  {in_three}");
    let rungs: Vec<String> = [2u32, 3, 5, 10, 25, 50]
        .iter()
        .map(|rung| {
            let count = book
                .wallets
                .values()
                .filter(|wallet| wallet.assets >= *rung)
                .count();
            format!("≥{rung} {count}")
        })
        .collect();
    println!("  assets across all  {}", rungs.join(" · "));

    // What the Perps policy leaves of the guaranteed reserve, at full take-up.
    let headroom = i64::from(reserves.gtd) - i64::from(tally.gtd * policy.per_identity);
    println!();
    println!("partner-only demand (one partner wallet is one identity, because it is the anchor)");
    println!(
        "{:>8} {:>11} {:>9} {:>9} {:>11} {:>13}",
        "turnout", "identities", "at 1 each", "at cap", "vs fcfs", "gtd fits at 1"
    );
    for share in TURNOUTS {
        let identities = (f64::from(wallets) * share).round() as u32;
        let demand = identities * policy.per_identity;
        let oversub = f64::from(demand) / f64::from(reserves.fcfs.max(1));
        let gtd_fits = if headroom > 0 {
            identities.min(headroom as u32)
        } else {
            0
        };
        println!(
            "{:>7.1}% {identities:>11} {identities:>9} {demand:>9} {oversub:>10.2}x {gtd_fits:>13}",
            share * 100.0
        );
    }
    println!(
        "note        one mint per partner identity fills the FCFS reserve at a turnout of {:.1}%",
        f64::from(reserves.fcfs) * 100.0 / f64::from(wallets.max(1))
    );
    println!("            of the pool; above that an entitlement is a race, not a reward.");
    println!(
        "note        gtd headroom is {headroom:+} mints after the Perps policy — that is what a"
    );
    println!("            partner discount has to fit inside, and a partner who takes it needs a");
    println!("            linked Cardano wallet that also reaches the Perps threshold.");
    Ok(())
}

fn load_partners(paths: &[String]) -> Result<PartnerBook, String> {
    let mut book = PartnerBook {
        collections: Vec::new(),
        wallets: BTreeMap::new(),
    };
    for path in paths {
        let text =
            std::fs::read_to_string(path).map_err(|error| format!("read {path}: {error}"))?;
        let mut holders = 0u32;
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let held: HeldLine = serde_json::from_str(line)
                .map_err(|error| format!("{path}:{}: {error}", number + 1))?;
            if held.balance == 0 {
                continue;
            }
            holders += 1;
            let wallet = book.wallets.entry(held.address).or_default();
            wallet.collections += 1;
            wallet.assets += held.balance;
        }
        book.collections.push((label(path), holders));
    }
    Ok(book)
}

/// `…/0x2370….holders.jsonl` → `0x2370…`, so a census written by `--out` labels itself.
fn label(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let name = name.strip_suffix(".holders.jsonl").unwrap_or(name);
    name.to_string()
}

fn write_grid(
    path: &str,
    counts: &[Counts],
    policy: Allocation,
    reserves: Reserves,
    max_core: u32,
    max_flavour: u32,
) -> Result<(), String> {
    let mut out = String::from(
        "gtd_core,fcfs_core,fcfs_flavour,gtd,fcfs,ungated,gtd_demand,fcfs_demand,gtd_headroom,fcfs_headroom\n",
    );
    for core in 0..=max_core {
        for flavour in 0..=max_flavour {
            let candidate = Allocation {
                gtd_core: core,
                fcfs_flavour: flavour,
                ..policy
            };
            let tally = tally(counts, candidate);
            let gtd_demand = tally.gtd * policy.per_identity;
            let fcfs_demand = tally.fcfs * policy.per_identity;
            let gtd_headroom = i64::from(reserves.gtd) - i64::from(gtd_demand);
            let fcfs_headroom = i64::from(reserves.fcfs) - i64::from(fcfs_demand);
            writeln!(
                out,
                "{core},{fcfs_core},{flavour},{gtd},{fcfs},{ungated},{gtd_demand},{fcfs_demand},{gtd_headroom},{fcfs_headroom}",
                fcfs_core = candidate.fcfs_core,
                gtd = tally.gtd,
                fcfs = tally.fcfs,
                ungated = tally.ungated
            )
            .map_err(|error| format!("write grid: {error}"))?;
        }
    }
    std::fs::write(path, out).map_err(|error| format!("write {path}: {error}"))
}
