//! Models a gated allocation over a published bundle.
//!
//! The bundle already carries every subject's earned set and each achievement's rarity, so
//! this reads it and applies a policy: no refold, no stream, no identity store. It answers the
//! question a mint has to settle before it opens — under this policy, how many subjects land in
//! each phase, and does the guaranteed phase fit inside its reserve?
//!
//! ```sh
//! cargo run --release -p holder-behaviour --example allocate -- \
//!     .tmp/perps-status.bundle \
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

use holder_behaviour::{
    Allocation, Bundle, Counts, Lot, Phase, StatusPredicate, StatusView, catalogue_digest, held_at,
    peak_concurrent, phase, score, status_holds,
};
use support::catalogue::{LoadedCatalogue, Plan, Status};

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
    /// Core achievements a partner holding buys. Never buys the Perp itself.
    partner_promote: u32,
    /// Model without the catalogue's holding gate.
    ignore_gate: bool,
    /// How many identities to move into the guaranteed phase when the promoting status is a
    /// live one this bundle cannot read.
    assume_promoted: u32,
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
///
/// [`Counts`], [`Phase`] and the phase decision itself are `holder_behaviour`'s: this tool and
/// the site that shows a holder their estimate must not each carry a copy of the rule.

#[derive(Default)]
struct Tally {
    gtd: u32,
    fcfs: u32,
    ungated: u32,
    fcfs_by_core: u32,
    fcfs_by_flavour: u32,
    one_core_short: u32,
    /// Scored enough for a gated phase but fails the holding gate — public only.
    gated_out: u32,
    /// Subjects in the guaranteed phase through a status rather than a score.
    promoted: u32,
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

    let catalogue_plan = catalogue.plan();
    let policy = resolve_policy(&args, catalogue_plan.map(|plan| plan.allocation));
    validate(&policy)?;
    let (gate, promotions) = resolve_statuses(&catalogue, catalogue_plan, args.ignore_gate)?;
    check_gate(&bundle, &gate)?;
    let promote_bonus = catalogue_plan.map(|plan| plan.promote_bonus).unwrap_or(0);
    // A promotion this bundle can read is applied per subject; a trait one cannot be, so it is
    // reported and the caller states how many identities it should move.
    let (evaluable_promotions, live_promotions): (Vec<&Status>, Vec<&Status>) =
        promotions.into_iter().partition(|status| {
            matches!(&status.predicate, StatusPredicate::HoldsOf { collection, .. }
                if collection.to_string() == bundle.collection())
        });
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
    println!("framing     status anchors · achievements boost · every number here is an estimate");
    println!("gate        {}", describe_statuses(&gate));
    if !evaluable_promotions.is_empty() || !live_promotions.is_empty() {
        let mut parts = Vec::new();
        if !evaluable_promotions.is_empty() {
            parts.push(describe_statuses(&evaluable_promotions));
        }
        if !live_promotions.is_empty() {
            parts.push(format!(
                "live-only: {} · {} assumed",
                describe_statuses(&live_promotions),
                args.assume_promoted
            ));
        }
        println!("promotes    {}", parts.join(" · "));
    }
    println!(
        "reserve     gtd {} · fcfs {} · public {} = {}",
        reserves.gtd,
        reserves.fcfs,
        reserves.public,
        reserves.total()
    );
    if catalogue_plan.is_none() {
        println!("note        the catalogue carries no [allocation]; defaults were used");
    }

    let mut counts = counts_for(&bundle, &catalogue, policy, &gate, &evaluable_promotions)?;
    if args.assume_promoted > 0 {
        apply_assumed_promotions(&mut counts, policy, args.assume_promoted);
    }
    let total = counts.len() as u32;
    let tally = tally(&counts, policy);
    print_tally(total, policy, reserves, &tally, promote_bonus);
    print_gtd_frontier(&counts, policy, reserves, args.sweep_core);
    print_fcfs_frontier(&counts, policy, reserves, args.sweep_flavour);
    print_floor_sweep(
        &bundle,
        &catalogue,
        policy,
        &gate,
        &evaluable_promotions,
        total,
    )?;
    print_rarest_sweep(
        &bundle,
        &catalogue,
        policy,
        &gate,
        &evaluable_promotions,
        total,
    )?;
    print_cap_table(policy, reserves, &tally);
    print_ladder(&counts, policy);
    print_partners(&args.partners, policy, reserves)?;
    print_partner_promotion(&counts, policy, reserves, args.partner_promote);
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
    let mut partner_promote = 1;
    let mut ignore_gate = false;
    let mut assume_promoted = 0;
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
            "--partner-promote" => partner_promote = number(&mut args, "--partner-promote")?,
            "--ignore-gate" => ignore_gate = true,
            "--assume-promoted" => assume_promoted = number(&mut args, "--assume-promoted")?,
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
        partner_promote,
        ignore_gate,
        assume_promoted,
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
    println!("  --partner-promote N core achievements a partner holding buys (default 1)");
    println!("  --ignore-gate       model without the catalogue's holding requirement");
    println!("  --assume-promoted N treat N identities as holding a live-only promoting status");
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

/// The statuses a run actually uses, resolved from the ids `[allocation]` names.
///
/// `--ignore-gate` empties the gate and leaves promotions alone: the two answer different
/// questions, and a run that dropped both would not be a comparison of anything.
fn resolve_statuses<'a>(
    catalogue: &'a LoadedCatalogue,
    plan: Option<&Plan>,
    ignore_gate: bool,
) -> Result<(Vec<&'a Status>, Vec<&'a Status>), String> {
    let resolve = |ids: &[String]| -> Result<Vec<&'a Status>, String> {
        ids.iter()
            .map(|id| {
                catalogue
                    .status(id)
                    .ok_or_else(|| format!("`{id}` is named by [allocation] but not declared"))
            })
            .collect()
    };
    let Some(plan) = plan else {
        return Ok((Vec::new(), Vec::new()));
    };
    let gate = if ignore_gate {
        Vec::new()
    } else {
        resolve(&plan.requires)?
    };
    Ok((gate, resolve(&plan.promotes)?))
}

/// A gate this bundle cannot answer is an error, not a quiet pass.
///
/// The gate is a statement about what an identity holds **at claim time**, which at runtime is a
/// live read rather than this snapshot. The bundle can only answer a holding count on its own
/// collection, so anything else — a trait, `$PERP`, a partner — is refused here rather than being
/// modelled as satisfied.
fn check_gate(bundle: &Bundle<'_>, gate: &[&Status]) -> Result<(), String> {
    for status in gate {
        match &status.predicate {
            StatusPredicate::HoldsOf {
                collection,
                at_least,
            } => {
                if collection.to_string() != bundle.collection() {
                    return Err(format!(
                        "status `{}` asks about {collection}, which this bundle cannot answer — \
                         it scores {} · pass --ignore-gate to model without it",
                        status.id,
                        bundle.collection()
                    ));
                }
                if *at_least == 0 {
                    return Err(format!(
                        "status `{}` asks for at least zero, which gates nobody",
                        status.id
                    ));
                }
            }
            StatusPredicate::HoldsTrait { .. } => {
                return Err(format!(
                    "status `{}` needs a trait read, which a bundle does not carry — it is a \
                     live check; model it with --assume-promoted",
                    status.id
                ));
            }
        }
    }
    Ok(())
}

fn describe_statuses(statuses: &[&Status]) -> String {
    if statuses.is_empty() {
        return "none".to_string();
    }
    statuses
        .iter()
        .map(|status| match &status.predicate {
            StatusPredicate::HoldsOf { at_least, .. } => format!("{} ≥{at_least}", status.id),
            StatusPredicate::HoldsTrait { category, .. } => {
                format!("{} holds a {category}", status.id)
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

fn ask_all(statuses: &[&Status], view: &StatusView<'_>) -> Result<bool, String> {
    for status in statuses {
        let holds = status_holds(&status.predicate, view)
            .map_err(|error| format!("status `{}`: {error}", status.id))?;
        if !holds {
            return Ok(false);
        }
    }
    Ok(true)
}

fn ask_any(statuses: &[&Status], view: &StatusView<'_>) -> Result<bool, String> {
    for status in statuses {
        let holds = status_holds(&status.predicate, view)
            .map_err(|error| format!("status `{}`: {error}", status.id))?;
        if holds {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Mark `count` subjects as promoted when the status behind them cannot be read here.
///
/// Deterministic and stated: the highest scorers that are not already in the guaranteed phase,
/// in bundle order. The point is to size an effect, not to guess who holds a leader role — the
/// trait read is the only thing that knows that.
fn apply_assumed_promotions(counts: &mut [Counts], policy: Allocation, count: u32) {
    let mut order: Vec<usize> = (0..counts.len()).collect();
    order.sort_by(|left, right| {
        counts[*right]
            .core
            .cmp(&counts[*left].core)
            .then(left.cmp(right))
    });
    let mut promoted = 0;
    for index in order {
        if promoted == count {
            break;
        }
        // Only subjects the promotion can actually move: already in the guarantee is not a
        // promotion, and a gated-out subject has no profile to spend it on.
        if counts[index].enabled && counts[index].core < policy.gtd_core && !counts[index].promoted
        {
            counts[index].promoted = true;
            promoted += 1;
        }
    }
}

fn counts_for(
    bundle: &Bundle<'_>,
    catalogue: &LoadedCatalogue,
    policy: Allocation,
    gate: &[&Status],
    promotions: &[&Status],
) -> Result<Vec<Counts>, String> {
    let mut subjects = Vec::with_capacity(bundle.subject_count() as usize);
    let cursor = bundle.holder_cursor();
    // The bundle carries no traits, so a trait status is invisible here — `check_gate` refuses
    // one on the gate, and `run` keeps one out of `promotions` for the same reason.
    let no_traits: [(String, String); 0] = [];
    for subject in bundle.subjects() {
        let subject = subject.map_err(|error| format!("read subject: {error}"))?;
        let scored = score(
            bundle,
            &subject,
            |index| catalogue.role_at(index as usize),
            policy,
        )
        .map_err(|error| error.to_string())?;
        let lots: Vec<Lot> = subject.lots().collect();
        let current = held_at(&lots, cursor);
        let view = StatusView {
            collection: bundle.collection(),
            units: u64::from(current),
            traits: &no_traits,
        };
        let enabled = ask_all(gate, &view)?;
        let promoted = ask_any(promotions, &view)?;
        subjects.push(scored.counts(peak_concurrent(&lots, cursor), current, enabled, promoted));
    }
    Ok(subjects)
}

fn tally(counts: &[Counts], policy: Allocation) -> Tally {
    let mut tally = Tally::default();
    for subject in counts {
        if subject.core < policy.gtd_core && subject.core + 1 == policy.gtd_core {
            tally.one_core_short += 1;
        }
        if !subject.enabled
            && (subject.core >= policy.fcfs_core
                || (policy.fcfs_flavour > 0 && subject.credits.len() as u32 >= policy.fcfs_flavour))
        {
            tally.gated_out += 1;
        }
        match phase(subject, policy) {
            Phase::Gtd => {
                tally.gtd += 1;
                if subject.promoted {
                    tally.promoted += 1;
                }
            }
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

fn print_tally(
    total: u32,
    policy: Allocation,
    reserves: Reserves,
    tally: &Tally,
    promote_bonus: u32,
) {
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
    if tally.gated_out > 0 {
        println!(
            "gated out   {} scored for a gated phase and hold none — public only",
            tally.gated_out
        );
    }
    if tally.promoted > 0 {
        println!(
            "promoted    {} in GTD on a status rather than a score · +{} mints at a bonus of {}",
            tally.promoted,
            tally.promoted * promote_bonus,
            promote_bonus
        );
    }
    println!(
        "anchor      gtd {} anchored on status · {} boosted by score",
        tally.promoted,
        tally.gtd - tally.promoted
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
    gate: &[&Status],
    promotions: &[&Status],
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
        let counts = counts_for(bundle, catalogue, candidate, gate, promotions)?;
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
    gate: &[&Status],
    promotions: &[&Status],
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
        let counts = counts_for(bundle, catalogue, candidate, gate, promotions)?;
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

/// Every partner collection's measured holders, and the union of them.
///
/// A partner wallet is not a bundle subject: it holds no Perps history of its own. What it
/// can do is anchor an identity. Scoring reads a balance per collection, so the numbers that
/// matter are per collection — a gate on "≥5 of one collection" admits a different population
/// from "any of the three".
struct PartnerBook {
    collections: Vec<PartnerCollection>,
    /// Per wallet: how many partner collections it holds, and how many assets in total.
    wallets: BTreeMap<String, PartnerWallet>,
}

struct PartnerCollection {
    label: String,
    /// Every holder's balance, so a ladder can be taken per collection.
    balances: Vec<u32>,
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

/// The rungs a partner gate is likely to be written at.
const LADDER: [u32; 5] = [2, 3, 5, 10, 25];

/// Turnout shares the partner pressure is shown at. The pool is large enough that a single
/// number would imply a forecast this cannot make.
const TURNOUTS: [f64; 7] = [0.01, 0.025, 0.05, 0.10, 0.25, 0.50, 1.00];

/// How much of the below-threshold Perps population also holds a partner asset. Unknowable
/// before bonding, so swept rather than assumed.
const OVERLAPS: [f64; 6] = [0.0, 0.10, 0.25, 0.50, 0.75, 1.00];

fn print_partners(paths: &[String], policy: Allocation, reserves: Reserves) -> Result<(), String> {
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
    print!("{:<46} {:>8}", "collection", "holders");
    for rung in LADDER {
        print!(" {:>6}", format!("≥{rung}"));
    }
    println!();
    for collection in &book.collections {
        print!("{:<46} {:>8}", collection.label, collection.balances.len());
        for rung in LADDER {
            let count = collection
                .balances
                .iter()
                .filter(|balance| **balance >= rung)
                .count();
            print!(" {count:>6}");
        }
        println!();
    }

    let wallets = book.wallets.len() as u32;
    let in_two = book
        .wallets
        .values()
        .filter(|wallet| wallet.collections >= 2)
        .count() as u32;
    let in_all = book
        .wallets
        .values()
        .filter(|wallet| wallet.collections as usize >= book.collections.len())
        .count() as u32;
    println!();
    println!("partner reach");
    println!("  wallets            {wallets}");
    println!("  in ≥2 collections  {in_two}");
    println!("  in all of them     {in_all}");
    let rungs: Vec<String> = LADDER
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

    println!();
    println!("partner-only fcfs demand (a partner wallet is one anchor, so turnout × wallets)");
    println!(
        "{:>8} {:>11} {:>9} {:>9} {:>10}",
        "turnout", "identities", "at 1 each", "at cap", "vs fcfs"
    );
    for share in TURNOUTS {
        let identities = (f64::from(wallets) * share).round() as u32;
        let demand = identities * policy.per_identity;
        let oversub = f64::from(demand) / f64::from(reserves.fcfs.max(1));
        println!(
            "{:>7.1}% {identities:>11} {identities:>9} {demand:>9} {oversub:>9.2}x",
            share * 100.0
        );
    }
    println!(
        "note        one mint per partner identity fills the FCFS reserve at a turnout of {:.1}%",
        f64::from(reserves.fcfs) * 100.0 / f64::from(wallets.max(1))
    );
    println!("            of the pool; above that an entitlement is a race, not a reward.");
    Ok(())
}

/// What a partner holding is worth toward the guaranteed phase.
///
/// The rule the simulation enforces: **a partner holding can buy achievements, never the
/// Perp**. A subject with `current == 0` is not holding one at the read, so it is outside the
/// band whatever `--partner-promote` says — which is also why partner turnout does not appear
/// here.
fn print_partner_promotion(
    counts: &[Counts],
    policy: Allocation,
    reserves: Reserves,
    promote: u32,
) {
    let ever = counts.iter().filter(|subject| subject.peak >= 1).count() as u32;
    let holders = counts.iter().filter(|subject| subject.current >= 1).count() as u32;
    let at_threshold = counts
        .iter()
        .filter(|subject| subject.core >= policy.gtd_core)
        .count() as u32;
    let gtd = counts
        .iter()
        .filter(|subject| subject.enabled && (subject.promoted || subject.core >= policy.gtd_core))
        .count() as u32;
    let band = counts
        .iter()
        .filter(|subject| {
            subject.enabled && subject.core >= promote && subject.core < policy.gtd_core
        })
        .count() as u32;

    println!();
    println!("partner promotion (a partner holding buys achievements — never the Perp)");
    println!(
        "  perps      {holders} of {} subjects hold one now · {ever} ever did",
        counts.len()
    );
    println!(
        "  gtd        {gtd} at core≥{} · {} mints at cap {}",
        policy.gtd_core,
        gtd * policy.per_identity,
        policy.per_identity
    );
    if at_threshold > gtd {
        println!(
            "  gated      {} of {at_threshold} clear the threshold and hold none at the read",
            at_threshold - gtd
        );
    }
    if promote >= policy.gtd_core {
        println!("  band       none — --partner-promote {promote} is not below the GTD threshold");
        return;
    }
    println!(
        "  band       {band} subjects at core {promote}..{} — the only ones promotion moves",
        policy.gtd_core.saturating_sub(1)
    );
    if band == 0 {
        println!("  note        inert: no subject in that band has ever held one");
        return;
    }

    println!();
    println!(
        "{:>8} {:>9} {:>11} {:>10} {:>10}",
        "overlap", "promoted", "gtd pool", "mints", "headroom"
    );
    for share in OVERLAPS {
        let promoted = (f64::from(band) * share).round() as u32;
        let pool = gtd + promoted;
        let mints = pool * policy.per_identity;
        let headroom = i64::from(reserves.gtd) - i64::from(mints);
        println!(
            "{:>7.0}% {promoted:>9} {pool:>11} {mints:>10} {headroom:>+10}",
            share * 100.0
        );
    }
    println!("note        overlap is the share of those {band} that also holds a partner asset.");
    println!("            It cannot be measured before bonding, so it is swept. Turnout is");
    println!("            absent by construction: a partner with no Perp cannot reach GTD.");
}

fn load_partners(paths: &[String]) -> Result<PartnerBook, String> {
    let mut book = PartnerBook {
        collections: Vec::new(),
        wallets: BTreeMap::new(),
    };
    for path in paths {
        let text =
            std::fs::read_to_string(path).map_err(|error| format!("read {path}: {error}"))?;
        let mut balances = Vec::new();
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
            balances.push(held.balance);
            let wallet = book.wallets.entry(held.address).or_default();
            wallet.collections += 1;
            wallet.assets += held.balance;
        }
        book.collections.push(PartnerCollection {
            label: label(path),
            balances,
        });
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
