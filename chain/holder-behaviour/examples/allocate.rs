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
//!     --gtd-score 60 --fcfs-score 25 --fcfs-flavour 3 \
//!     --gtd-reserve 3000 --fcfs-reserve 1500 --public-reserve 1500
//! ```
//!
//! What counts is the pipeline the design doc names: core achievements are never filtered; a
//! flavour achievement above `rarity_floor` does not count; a subject keeps its
//! `count_rarest` rarest remaining flavour achievements. Each counted achievement is worth the
//! points the catalogue gives it, and a phase threshold is a sum of those points rather than a
//! count of achievements.
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
    Allocation, Bundle, Counts, Lot, Phase, Rung, Seasoning, StatusPredicate, StatusView,
    catalogue_digest, grants, held_at, peak_concurrent, phase, score, seasoned_at, slots,
    status_holds,
};
use support::catalogue::{LoadedCatalogue, Plan, Status};

const DEFAULT_GTD_SCORE: u32 = 60;
const DEFAULT_FCFS_SCORE: u32 = 25;
const DEFAULT_FCFS_FLAVOUR: u32 = 3;
const DEFAULT_CAP: u32 = 3;
const DEFAULT_FLOOR: f64 = 0.02;
const DEFAULT_RAREST: u32 = 3;
const DEFAULT_GTD_RESERVE: u32 = 3_000;
const DEFAULT_FCFS_RESERVE: u32 = 1_500;
const DEFAULT_PUBLIC_RESERVE: u32 = 1_500;
/// The score step the GTD frontier walks in. A threshold moves the population in jumps the
/// catalogue's weights decide, so a sweep finer than a weight would only repeat itself.
/// A sweep step that gives about forty rows whatever scale the weights are on: a catalogue
/// priced in thousands would otherwise print seven thousand lines.
fn sweep_step(ceiling: u32) -> u32 {
    const NICE: [u32; 10] = [1, 2, 5, 10, 25, 50, 100, 250, 500, 1_000];
    let wanted = (ceiling / 40).max(1);
    NICE.iter()
        .copied()
        .find(|step| *step >= wanted)
        .unwrap_or(wanted)
}
/// The shortest frontier shown when `--sweep-score` is not given.
const DEFAULT_SWEEP_SCORE: u32 = 100;

struct Args {
    bundle: String,
    catalogue: String,
    gtd_score: Option<u32>,
    fcfs_score: Option<u32>,
    fcfs_rarest: Option<u32>,
    cap: Option<u32>,
    floor: Option<f64>,
    rarest: Option<u32>,
    gtd_reserve: u32,
    fcfs_reserve: u32,
    public_reserve: u32,
    sweep_score: Option<u32>,
    sweep_flavour: u32,
    /// Measured holder sets, one `holder_census --out` JSONL per partner collection.
    partners: Vec<String>,
    /// The score a partner holding buys. Never buys the Perp itself.
    partner_promote: u32,
    /// Model without the catalogue's holding gate.
    ignore_gate: bool,
    /// How many identities to move into the guaranteed phase when the promoting status is a
    /// live one this bundle cannot read.
    assume_promoted: u32,
    out: Option<String>,
    /// One subject's key: print what scoring made of its row and what the policy gives it,
    /// instead of the population report.
    explain: Option<String>,
    /// The share a holding keeps once sold. See `Allocation::past_share`.
    past_share: Option<f64>,
    /// Units held now that the guarantee needs. See `Allocation::gtd_min_held`.
    gtd_min_held: Option<u32>,
    /// The ceiling per identity; zero removes it.
    max_per_identity: Option<u32>,
    /// The seasoning a ladder unit needs to count in full, and what a fresher one counts at.
    season_days: Option<u32>,
    season_share: Option<f64>,
    /// Achievement ids whose held-now standing promotes, standing in for a live trait read.
    promote_held: Vec<String>,
    /// Perps listed for sale now, as `<stake credential hex> <count>` lines: taken off each
    /// seller's holding, to model a rule that counts only what is not for sale. A Cardano listing
    /// keeps the seller's stake credential, so the walk still reads a listed Perp as held.
    listings: Option<String>,
    /// A jostle scenario: every holder over the guarantee's score holding fewer than this buys
    /// fresh Perps up to it. Zero: nobody does.
    top_up: u32,
    /// Token balances now, as `<stake credential hex> <units>` lines (the holder map's `top`).
    token_balances: Option<String>,
    /// Keys whose token balance is not a holder's: a staking contract, a treasury.
    token_exclude: Vec<String>,
    /// What the ladder economics are priced at.
    perpling_usd: f64,
    floor_ada: f64,
    fcfs_usd: f64,
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
    fcfs_by_score: u32,
    fcfs_by_flavour: u32,
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
        "policy      gtd score≥{} · fcfs score≥{} or flavour≥{} · floor {:.1}% · rarest {} · cap {}",
        policy.gtd_score,
        policy.fcfs_score,
        policy.fcfs_rarest,
        policy.rarity_floor * 100.0,
        policy.count_rarest,
        policy.per_identity
    );
    println!(
        "holdings    past share {} · gtd needs {} held now · ceiling {}",
        policy
            .past_share
            .map_or("off (every holding as if held)".to_string(), |share| {
                format!("{:.0}%", share * 100.0)
            }),
        policy.gtd_min_held,
        match policy.max_per_identity {
            0 => "none".to_string(),
            cap => cap.to_string(),
        }
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

    // The lapsed door is keyed on an achievement, which the bundle answers by index.
    let lapsed = catalogue_plan
        .and_then(|plan| plan.lapsed.as_deref())
        .and_then(|id| catalogue.ids().position(|other| other == id))
        .map(|index| index as u16);
    // The partner gate is the smallest holding any partner status asks for.
    let partner_gate = catalogue_plan
        .map(|plan| {
            plan.partners
                .iter()
                .filter_map(|id| match catalogue.status(id).map(|s| &s.predicate) {
                    Some(StatusPredicate::HoldsOf { at_least, .. }) => Some(*at_least as u32),
                    _ => None,
                })
                .min()
                .unwrap_or(1)
        })
        .unwrap_or(1);
    if let Some(key) = &args.explain {
        return explain_subject(&bundle, &catalogue, policy, &gate, lapsed, key);
    }

    let mut counts = counts_for(
        &bundle,
        &catalogue,
        policy,
        &gate,
        &evaluable_promotions,
        lapsed,
    )?;
    let promote_held = args
        .promote_held
        .iter()
        .map(|id| {
            catalogue
                .ids()
                .position(|other| other == id)
                .map(|index| index as u16)
                .ok_or_else(|| format!("--promote-held {id}: not in the catalogue"))
        })
        .collect::<Result<Vec<u16>, String>>()?;
    apply_holdings(&bundle, &catalogue, &mut counts, policy, &promote_held)?;
    if let Some(path) = &args.listings {
        apply_listings(&bundle, &mut counts, path)?;
    }
    if let Some(path) = &args.token_balances {
        apply_token(&bundle, &mut counts, path, &args.token_exclude, policy)?;
    }
    if args.top_up > 0 {
        // Fresh units: the live count rises, the seasoned count does not.
        let mut buyers = 0;
        let mut bought = 0;
        for subject in counts.iter_mut() {
            if subject.enabled && subject.score >= policy.gtd_score && subject.current < args.top_up
            {
                bought += args.top_up - subject.current;
                subject.current = args.top_up;
                buyers += 1;
            }
        }
        println!(
            "top-up      {buyers} holders over the line buy {bought} Perps to hold {}",
            args.top_up
        );
    }
    if args.assume_promoted > 0 {
        apply_assumed_promotions(&mut counts, policy, args.assume_promoted);
    }
    let total = counts.len() as u32;
    // The frontier's ceiling: the caller's, or far enough past the population that the score at
    // which the guaranteed phase stops filling its reserve is always in the table.
    let ceiling = args.sweep_score.unwrap_or_else(|| {
        let top = counts
            .iter()
            .map(|subject| subject.score)
            .max()
            .unwrap_or(0);
        top.max(DEFAULT_SWEEP_SCORE)
    });
    let step = sweep_step(ceiling);
    let max_score = ceiling.div_ceil(step) * step;

    // A ladder policy is a different shape: mints follow the holding and the score adds rungs,
    // so the flat-cap frontiers below would answer a question this policy does not ask.
    if !policy.holding.is_empty() {
        print_ladder_allocation(
            &counts,
            policy,
            reserves,
            args.assume_promoted,
            &args.partners,
            partner_gate,
        )?;
        print_past_holdings(&bundle, &catalogue, policy)?;
        print_cohorts(&bundle, &catalogue, policy)?;
        print_ladder_economics(policy, args.perpling_usd, args.floor_ada, args.fcfs_usd);
        print_ladder(&counts, policy);
        print_partners(&args.partners, policy, reserves)?;
        print_caveats();
        return Ok(());
    }

    let tally = tally(&counts, policy);
    print_tally(total, &counts, policy, reserves, &tally, promote_bonus);
    print_gtd_frontier(&counts, policy, reserves, max_score, step);
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
            max_score,
            step,
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
    let mut gtd_score = None;
    let mut fcfs_score = None;
    let mut fcfs_rarest = None;
    let mut cap = None;
    let mut floor = None;
    let mut rarest = None;
    let mut gtd_reserve = DEFAULT_GTD_RESERVE;
    let mut fcfs_reserve = DEFAULT_FCFS_RESERVE;
    let mut public_reserve = DEFAULT_PUBLIC_RESERVE;
    let mut sweep_score = None;
    let mut sweep_flavour = 6;
    let mut partners = Vec::new();
    let mut partner_promote = 1;
    let mut ignore_gate = false;
    let mut assume_promoted = 0;
    let mut out = None;
    let mut explain = None;
    let mut past_share = None;
    let mut gtd_min_held = None;
    let mut max_per_identity = None;
    let mut season_days = None;
    let mut season_share = None;
    let mut promote_held = Vec::new();
    let mut listings = None;
    let mut top_up = 0;
    let mut token_balances = None;
    let mut token_exclude = Vec::new();
    let mut perpling_usd = 100.0;
    let mut floor_ada = 60.0;
    let mut fcfs_usd = 6.0;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "--catalogue" => catalogue = Some(next(&mut args, "--catalogue")?),
            "--gtd-score" => gtd_score = Some(number(&mut args, "--gtd-score")?),
            "--fcfs-score" => fcfs_score = Some(number(&mut args, "--fcfs-score")?),
            "--fcfs-flavour" => fcfs_rarest = Some(number(&mut args, "--fcfs-flavour")?),
            "--cap" => cap = Some(number(&mut args, "--cap")?),
            "--floor" => floor = Some(decimal(&mut args, "--floor")?),
            "--rarest" => rarest = Some(number(&mut args, "--rarest")?),
            "--gtd-reserve" => gtd_reserve = number(&mut args, "--gtd-reserve")?,
            "--fcfs-reserve" => fcfs_reserve = number(&mut args, "--fcfs-reserve")?,
            "--public-reserve" => public_reserve = number(&mut args, "--public-reserve")?,
            "--sweep-score" => sweep_score = Some(number(&mut args, "--sweep-score")?),
            "--sweep-flavour" => sweep_flavour = number(&mut args, "--sweep-flavour")?,
            "--partners" => partners.push(next(&mut args, "--partners")?),
            "--partner-promote" => partner_promote = number(&mut args, "--partner-promote")?,
            "--ignore-gate" => ignore_gate = true,
            "--assume-promoted" => assume_promoted = number(&mut args, "--assume-promoted")?,
            "--out" => out = Some(next(&mut args, "--out")?),
            "--explain" => explain = Some(next(&mut args, "--explain")?),
            "--past-share" => past_share = Some(decimal(&mut args, "--past-share")?),
            "--gtd-min-held" => gtd_min_held = Some(number(&mut args, "--gtd-min-held")?),
            "--max-per-identity" => {
                max_per_identity = Some(number(&mut args, "--max-per-identity")?)
            }
            "--season-days" => season_days = Some(number(&mut args, "--season-days")?),
            "--season-share" => season_share = Some(decimal(&mut args, "--season-share")?),
            "--promote-held" => promote_held.push(next(&mut args, "--promote-held")?),
            "--listings" => listings = Some(next(&mut args, "--listings")?),
            "--top-up" => top_up = number(&mut args, "--top-up")?,
            "--token-balances" => token_balances = Some(next(&mut args, "--token-balances")?),
            "--token-exclude" => token_exclude.push(next(&mut args, "--token-exclude")?),
            "--perpling-usd" => perpling_usd = decimal(&mut args, "--perpling-usd")?,
            "--floor-ada" => floor_ada = decimal(&mut args, "--floor-ada")?,
            "--fcfs-usd" => fcfs_usd = decimal(&mut args, "--fcfs-usd")?,
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other if bundle.is_none() => bundle = Some(other.to_string()),
            other => return Err(format!("unexpected argument {other}")),
        }
    }

    Ok(Args {
        bundle: bundle.ok_or("give the path to a bundle")?,
        catalogue: catalogue.ok_or("--catalogue is required")?,
        gtd_score,
        fcfs_score,
        fcfs_rarest,
        cap,
        floor,
        rarest,
        gtd_reserve,
        fcfs_reserve,
        public_reserve,
        sweep_score,
        sweep_flavour,
        partners,
        partner_promote,
        ignore_gate,
        assume_promoted,
        out,
        explain,
        past_share,
        gtd_min_held,
        max_per_identity,
        season_days,
        season_share,
        promote_held,
        listings,
        top_up,
        token_balances,
        token_exclude,
        perpling_usd,
        floor_ada,
        fcfs_usd,
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
    println!("  --gtd-score N       score for GTD (catalogue, else {DEFAULT_GTD_SCORE})");
    println!("  --fcfs-score N      score for FCFS (catalogue, else {DEFAULT_FCFS_SCORE})");
    println!("  --fcfs-flavour N    rare flavour achievements for FCFS (catalogue, else 3)");
    println!("  --cap N             mints per identity (catalogue, else 3)");
    println!("  --floor F           rarity floor as a share (catalogue, else 0.02)");
    println!("  --rarest N          flavour achievements that count (catalogue, else 3)");
    println!("  --gtd-reserve N     GTD supply (default {DEFAULT_GTD_RESERVE})");
    println!("  --fcfs-reserve N    FCFS supply (default {DEFAULT_FCFS_RESERVE})");
    println!("  --public-reserve N  public supply (default {DEFAULT_PUBLIC_RESERVE})");
    println!(
        "  --sweep-score N     highest GTD score threshold shown, in about forty steps (default \
                 {DEFAULT_SWEEP_SCORE}, or the top score if higher)"
    );
    println!("  --sweep-flavour N   highest FCFS flavour threshold shown (default 6)");
    println!("  --partners <path>   a partner holder set from `holder_census --out`, repeatable");
    println!("  --partner-promote N score a partner holding buys (default 1)");
    println!("  --ignore-gate       model without the catalogue's holding requirement");
    println!("  --assume-promoted N treat N identities as holding a live-only promoting status");
    println!("  --out <path.csv>    write the threshold grid");
    println!("  --past-share F      share a holding keeps once sold, 0..=1 (catalogue, else off)");
    println!("  --gtd-min-held N    units held now the guarantee needs (catalogue, else 0)");
    println!("  --max-per-identity N  ceiling per identity; 0 removes it (catalogue, else 0)");
    println!("  --explain <key>     one subject's rows, standings and mints");
    println!("  --season-days N     days a ladder unit needs to count in full; 0 turns it off");
    println!("  --season-share F    what a fresher unit counts at (default 0.5)");
    println!("  --promote-held ID   a held-now achievement that promotes, repeatable");
    println!("  --perpling-usd F --floor-ada F --fcfs-usd F   ladder economics (100 · 60 · 6)");
}

fn resolve_policy(args: &Args, from_catalogue: Option<Allocation>) -> Allocation {
    let base = from_catalogue.unwrap_or(Allocation {
        gtd_score: DEFAULT_GTD_SCORE,
        fcfs_score: DEFAULT_FCFS_SCORE,
        fcfs_rarest: DEFAULT_FCFS_FLAVOUR,
        per_identity: DEFAULT_CAP,
        rarity_floor: DEFAULT_FLOOR,
        count_rarest: DEFAULT_RAREST,
        ..Allocation::default()
    });
    Allocation {
        gtd_score: args.gtd_score.unwrap_or(base.gtd_score),
        fcfs_score: args.fcfs_score.unwrap_or(base.fcfs_score),
        fcfs_rarest: args.fcfs_rarest.unwrap_or(base.fcfs_rarest),
        per_identity: args.cap.unwrap_or(base.per_identity),
        rarity_floor: args.floor.unwrap_or(base.rarity_floor),
        count_rarest: args.rarest.unwrap_or(base.count_rarest),
        past_share: args.past_share.or(base.past_share),
        gtd_min_held: args.gtd_min_held.unwrap_or(base.gtd_min_held),
        max_per_identity: args.max_per_identity.unwrap_or(base.max_per_identity),
        seasoning: match (args.season_days, args.season_share, base.seasoning) {
            (None, None, base) => base,
            (Some(0), _, _) => None,
            (days, share, base) => Some(Seasoning {
                days: days.or(base.map(|s| s.days)).unwrap_or(90),
                share: share.or(base.map(|s| s.share)).unwrap_or(0.5),
            }),
        },
        // The ladders, bonuses and first-come doors are the catalogue's: a flag sweeps the
        // thresholds, not the shape of the allocation.
        ..base
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
            .score
            .cmp(&counts[*left].score)
            .then(left.cmp(right))
    });
    let mut promoted = 0;
    for index in order {
        if promoted == count {
            break;
        }
        // Only subjects the promotion can actually move: already in the guarantee is not a
        // promotion, and a gated-out subject has no profile to spend it on.
        if counts[index].enabled
            && counts[index].score < policy.gtd_score
            && !counts[index].promoted
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
    lapsed: Option<u16>,
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
            |index| catalogue.role_at(usize::from(index)),
            |index| catalogue.points_at(usize::from(index)),
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
        let once =
            lapsed.is_some_and(|index| subject.earned().any(|earned| earned.achievement == index));
        subjects.push(
            scored
                .counts(peak_concurrent(&lots, cursor), current, enabled, promoted)
                .lapsed(once),
        );
    }
    Ok(subjects)
}

/// What the bundle can say about a subject's holding beyond its count, applied over
/// [`counts_for`]'s answer in the same subject order.
///
/// - **Seasoned units**, from the lots' arrival times measured against the reading, when the
///   policy weighs the ladder by tenure.
/// - **Promotions held now**: the bundle carries no traits, so a Leader status cannot be asked of
///   it, but a held-now achievement that names one (`the-leader`, `multiverse`) is the same fact
///   read at the snapshot. It stands in for the live trait read the claim would make.
fn apply_holdings(
    bundle: &Bundle<'_>,
    catalogue: &LoadedCatalogue,
    counts: &mut [Counts],
    policy: Allocation,
    promote_held: &[u16],
) -> Result<(), String> {
    let now = bundle.built_at();
    for (subject, counts) in bundle.subjects().zip(counts.iter_mut()) {
        let subject = subject.map_err(|error| format!("read subject: {error}"))?;
        counts.granted = grants(
            subject
                .earned()
                .map(|earned| (earned.achievement, earned.standing, earned.held)),
            |index| catalogue.grant_at(usize::from(index)),
        );
        if let Some(seasoning) = policy.seasoning {
            let lots: Vec<Lot> = subject.lots().collect();
            counts.seasoned = seasoned_at(&lots, now, seasoning.days);
        }
        if counts.enabled
            && subject.earned().any(|earned| {
                earned.standing == holder_behaviour::Standing::Now
                    && promote_held.contains(&earned.achievement)
            })
        {
            counts.promoted = true;
        }
    }
    Ok(())
}

/// Read token balances onto the subjects that hold them, and add a subject for every holder the
/// Perps bundle has never seen: the token is its own door, so a token-only wallet is in the
/// population this models.
fn apply_token(
    bundle: &Bundle<'_>,
    counts: &mut Vec<Counts>,
    path: &str,
    exclude: &[String],
    policy: Allocation,
) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("read {path}: {error}"))?;
    let mut balances: BTreeMap<String, u64> = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (key, amount) = line
            .split_once(' ')
            .ok_or_else(|| format!("{path}: `{line}` is not `<key> <units>`"))?;
        if exclude.iter().any(|excluded| excluded == key) {
            continue;
        }
        let amount: u64 = amount
            .trim()
            .parse()
            .map_err(|_| format!("{path}: `{line}` has no amount"))?;
        *balances.entry(key.to_string()).or_default() += amount;
    }
    let mut seen = 0;
    for (subject, counts) in bundle.subjects().zip(counts.iter_mut()) {
        let subject = subject.map_err(|error| format!("read subject: {error}"))?;
        if let Some(amount) = balances.remove(&subject.key_str()) {
            counts.token = amount;
            seen += 1;
        }
    }
    let token_only = balances.len();
    counts.extend(
        balances
            .into_values()
            .map(|amount| Counts::default().token(amount)),
    );
    let worth = |counts: &[Counts]| -> (usize, u32) {
        policy.token.map_or((0, 0), |rule| {
            let slotted: Vec<u32> = counts
                .iter()
                .map(|subject| rule.slots(subject.token))
                .filter(|slots| *slots > 0)
                .collect();
            (slotted.len(), slotted.iter().sum())
        })
    };
    let (holders, slots) = worth(counts);
    let gated: Vec<Counts> = counts
        .iter()
        .filter(|subject| subject.enabled)
        .cloned()
        .collect();
    let (gated_holders, gated_slots) = worth(&gated);
    println!(
        "token       {seen} Perps subjects and {token_only} token-only wallets hold it · {holders} reach a slot · {slots} guaranteed ({gated_holders} holding a Perp now · {gated_slots}) · {} excluded",
        exclude.len()
    );
    Ok(())
}

/// Take each seller's listed Perps off its holding, seasoned ones last, and shut the gate on a
/// subject whose every Perp is for sale.
fn apply_listings(bundle: &Bundle<'_>, counts: &mut [Counts], path: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("read {path}: {error}"))?;
    let mut listed: BTreeMap<String, u32> = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (key, count) = line
            .split_once(' ')
            .ok_or_else(|| format!("{path}: `{line}` is not `<key> <count>`"))?;
        let count: u32 = count
            .trim()
            .parse()
            .map_err(|_| format!("{path}: `{line}` has no count"))?;
        *listed.entry(key.to_string()).or_default() += count;
    }
    let (mut sellers, mut units) = (0, 0);
    for (subject, counts) in bundle.subjects().zip(counts.iter_mut()) {
        let subject = subject.map_err(|error| format!("read subject: {error}"))?;
        let Some(count) = listed.get(&subject.key_str()) else {
            continue;
        };
        let taken = (*count).min(counts.current);
        counts.current -= taken;
        // A seller lists its newest first as often as its oldest; taking the fresh ones first is
        // the reading kindest to it, and the one a rule would have to beat.
        let fresh = counts.current + taken - counts.seasoned.min(counts.current + taken);
        counts.seasoned = counts
            .seasoned
            .min(counts.current + taken)
            .saturating_sub(taken.saturating_sub(fresh));
        if counts.current == 0 {
            counts.enabled = false;
        }
        sellers += 1;
        units += taken;
    }
    println!("listings    {units} listed Perps taken off {sellers} sellers' holdings");
    Ok(())
}

/// What buying into each step of the holding ladder returns, for a holder buying fresh units
/// off the floor: how many it takes at the seasoning's share, what the step adds, and the ADA
/// price below which the Perplings are worth more than the Perps cost.
///
/// The cost is the floor paid, not the floor lost: a Perp bought for a snapshot can be sold again
/// after it, so this is the worst case for the buyer and the best case for the rule.
fn print_ladder_economics(policy: Allocation, perpling_usd: f64, floor_ada: f64, fcfs_usd: f64) {
    let share = policy
        .seasoning
        .map_or(1.0, |seasoning| seasoning.share.clamp(0.0, 1.0));
    let rungs = policy.holding.as_slice();
    if rungs.is_empty() || share <= 0.0 {
        return;
    }
    println!();
    println!(
        "ladder economics · a Perpling worth ${perpling_usd:.0} · Perps at {floor_ada:.0} ADA · fresh units at {:.0}%",
        share * 100.0
    );
    println!(
        "{:>9} {:>10} {:>8} {:>16} {:>16}",
        "step", "buy fresh", "adds", "gtd: worth if ADA<", "fcfs: if ADA<"
    );
    let mut previous = Rung {
        at_least: 0,
        mints: 0,
    };
    for rung in rungs {
        // The first unit always counts as one (a holder is never below the first rung); every
        // unit after it counts at the share.
        let units = f64::from(rung.at_least - previous.at_least);
        let buy = match previous.at_least {
            0 => 1.0 + ((units - 1.0) / share).ceil(),
            _ => (units / share).ceil(),
        };
        let adds = rung.mints - previous.mints;
        // Free in the guarantee; first come pays its price for each one.
        let gtd = perpling_usd * f64::from(adds) / (buy * floor_ada);
        let fcfs = (perpling_usd - fcfs_usd) * f64::from(adds) / (buy * floor_ada);
        println!(
            "{:>9} {buy:>10.0} {adds:>8} {:>15} {:>15}",
            format!("{}→{}", previous.at_least, rung.at_least),
            format!("${gtd:.2}"),
            format!("${fcfs:.2}")
        );
        previous = *rung;
    }
}

fn tally(counts: &[Counts], policy: Allocation) -> Tally {
    let mut tally = Tally::default();
    for subject in counts {
        if !subject.enabled
            && (subject.score >= policy.fcfs_score
                || (policy.fcfs_rarest > 0 && subject.rarest >= policy.fcfs_rarest))
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
                if subject.score >= policy.fcfs_score {
                    tally.fcfs_by_score += 1;
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
    counts: &[Counts],
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
        "fcfs        {} by score · {} by flavour (after the floor and the rarest cap)",
        tally.fcfs_by_score, tally.fcfs_by_flavour
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
    // The band nearest the guarantee. With catalogue weights the step below the threshold is
    // whatever the population holds, rather than always one achievement.
    if let Some(band) = counts
        .iter()
        .map(|subject| subject.score)
        .filter(|score| *score < policy.gtd_score)
        .max()
    {
        let near = counts
            .iter()
            .filter(|subject| subject.score == band)
            .count();
        println!(
            "near-miss   {near} subjects at score {band}, the nearest band below GTD (score {})",
            policy.gtd_score
        );
    }
}

fn print_gtd_frontier(
    counts: &[Counts],
    policy: Allocation,
    reserves: Reserves,
    max_score: u32,
    step: u32,
) {
    let total = counts.len() as u32;
    println!();
    println!("gtd frontier (fcfs and floor fixed)");
    println!(
        "{:>5} {:>8} {:>7} {:>9} {:>10}",
        "score", "gtd", "share", "demand", "headroom"
    );
    for score in (0..=max_score).step_by(step as usize) {
        let candidate = Allocation {
            gtd_score: score,
            ..policy
        };
        let tally = tally(counts, candidate);
        let demand = tally.gtd * policy.per_identity;
        let headroom = i64::from(reserves.gtd) - i64::from(demand);
        println!(
            "{score:>5} {:>8} {:>6.1}% {demand:>9} {headroom:>+10}",
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
    println!("fcfs frontier (gtd score≥{} fixed)", policy.gtd_score);
    println!(
        "{:>7} {:>8} {:>7} {:>9} {:>8}",
        "flavour", "fcfs", "share", "demand", "oversub"
    );
    for flavour in 0..=max_flavour {
        let candidate = Allocation {
            fcfs_rarest: flavour,
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
            "note        the flavour path adds nobody at this max share — all {} are by score",
            chosen.fcfs_by_score
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
        "max share (fcfs at score≥{} or flavour≥{}; lower is stricter)",
        policy.fcfs_score, policy.fcfs_rarest
    );
    println!("{:>9} {:>8} {:>7}", "max share", "fcfs", "share");
    for floor in [0.01, 0.02, 0.05, 0.10, 0.25, 0.50, 1.0] {
        let candidate = Allocation {
            rarity_floor: floor,
            ..policy
        };
        let counts = counts_for(bundle, catalogue, candidate, gate, promotions, None)?;
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
        "rarest cap (fcfs at score≥{} or flavour≥{})",
        policy.fcfs_score, policy.fcfs_rarest
    );
    println!("{:>7} {:>8} {:>7}", "rarest", "fcfs", "share");
    for rarest in 1..=5 {
        let candidate = Allocation {
            count_rarest: rarest,
            ..policy
        };
        let counts = counts_for(bundle, catalogue, candidate, gate, promotions, None)?;
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
    /// The largest balance it holds in any one collection: what a per-collection gate reads.
    largest: u32,
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
/// The rule the simulation enforces: **a partner holding can buy score, never the Perp**. A
/// subject with `current == 0` is not holding one at the read, so it is outside the band
/// whatever `--partner-promote` says — which is also why partner turnout does not appear here.
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
        .filter(|subject| subject.score >= policy.gtd_score)
        .count() as u32;
    let gtd = counts
        .iter()
        .filter(|subject| {
            subject.enabled && (subject.promoted || subject.score >= policy.gtd_score)
        })
        .count() as u32;
    let band = counts
        .iter()
        .filter(|subject| {
            subject.enabled && subject.score >= promote && subject.score < policy.gtd_score
        })
        .count() as u32;

    println!();
    println!("partner promotion (a partner holding buys achievements — never the Perp)");
    println!(
        "  perps      {holders} of {} subjects hold one now · {ever} ever did",
        counts.len()
    );
    println!(
        "  gtd        {gtd} at score≥{} · {} mints at cap {}",
        policy.gtd_score,
        gtd * policy.per_identity,
        policy.per_identity
    );
    if at_threshold > gtd {
        println!(
            "  gated      {} of {at_threshold} clear the threshold and hold none at the read",
            at_threshold - gtd
        );
    }
    if promote >= policy.gtd_score {
        println!("  band       none — --partner-promote {promote} is not below the GTD threshold");
        return;
    }
    println!(
        "  band       {band} subjects at score {promote}..{} — the only ones promotion moves",
        policy.gtd_score.saturating_sub(1)
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

/// A ladder policy against the reserves: who sits on which holding rung, what the score rungs
/// add, the leaders, the ceiling, and the two first-come doors.
///
/// Every mint figure is `holder_behaviour::mints`, the rule the worker answers a holder with,
/// so a number here and a number on a holder's page cannot disagree.
fn print_ladder_allocation(
    counts: &[Counts],
    policy: Allocation,
    reserves: Reserves,
    assumed_leaders: u32,
    partner_paths: &[String],
    partner_gate: u32,
) -> Result<(), String> {
    let gtd: Vec<&Counts> = counts
        .iter()
        .filter(|subject| phase(subject, policy) == Phase::Gtd)
        .collect();

    println!();
    println!(
        "ladder allocation (base by {} · score rungs add · leaders add)",
        match policy.seasoning {
            None => "units held now".to_string(),
            Some(s) => format!(
                "units held now, those under {} days at {:.0}%",
                s.days,
                s.share * 100.0
            ),
        }
    );
    println!(
        "{:>10} {:>11} {:>6} {:>8}",
        "units", "identities", "base", "demand"
    );
    let rungs = policy.holding.as_slice();
    let mut base_total = 0;
    for (index, rung) in rungs.iter().enumerate() {
        let next = rungs.get(index + 1).map_or(u32::MAX, |next| next.at_least);
        let members = gtd
            .iter()
            .filter(|subject| {
                let units = subject.ladder_units(policy);
                units >= rung.at_least && units < next
            })
            .count() as u32;
        let label = match next {
            u32::MAX => format!("{}+", rung.at_least),
            next => format!("{}–{}", rung.at_least, next - 1),
        };
        base_total += members * rung.mints;
        println!(
            "{label:>10} {members:>11} {:>6} {:>8}",
            rung.mints,
            members * rung.mints
        );
    }

    // Where the guaranteed holders' scores fall: what a bonus rung should be read against.
    let mut scores: Vec<u32> = gtd.iter().map(|subject| subject.score).collect();
    scores.sort_unstable();
    let at = |q: f64| {
        scores
            .get(((scores.len() as f64 - 1.0) * q).round() as usize)
            .copied()
            .unwrap_or(0)
    };
    println!();
    println!(
        "scores      p25 {} · p50 {} · p75 {} · p90 {} · p97 {} · max {}",
        at(0.25),
        at(0.5),
        at(0.75),
        at(0.9),
        at(0.97),
        at(1.0)
    );
    println!(
        "{:>10} {:>11} {:>6} {:>8}",
        "score ≥", "identities", "adds", "demand"
    );
    let mut bonus_total = 0;
    for rung in policy.bonus.as_slice() {
        let reached = gtd
            .iter()
            .filter(|subject| subject.score >= rung.at_least)
            .count() as u32;
        bonus_total += reached * rung.mints;
        println!(
            "{:>10} {reached:>11} {:>6} {:>8}",
            rung.at_least,
            rung.mints,
            reached * rung.mints
        );
    }

    // Every gated subject's two counts. Leaders the bundle cannot see are an assumption, bounded
    // by supply; a held Leader is already a grant.
    let leaders = assumed_leaders * policy.promote_bonus;
    // Gated holders, and token holders with or without a Perp: the token is its own door.
    let held: Vec<(&Counts, holder_behaviour::Slots)> = counts
        .iter()
        .filter(|subject| subject.enabled || subject.token > 0)
        .map(|subject| (subject, slots(subject, policy)))
        .collect();
    let granted_gtd: u32 = held.iter().map(|(subject, _)| subject.granted.gtd).sum();
    let granted_fcfs: u32 = held.iter().map(|(subject, _)| subject.granted.fcfs).sum();
    let mut per: Vec<(u32, u32)> = held
        .iter()
        .filter(|(_, slots)| slots.gtd > 0)
        .map(|(_, slots)| (slots.gtd, slots.fcfs))
        .collect();
    per.sort_unstable();
    let gtd_demand: u32 = per.iter().map(|(gtd, _)| gtd).sum::<u32>() + leaders;
    let pick = |q: f64| {
        per.get(((per.len() as f64 - 1.0) * q).round() as usize)
            .map(|(gtd, _)| *gtd)
    };
    println!();
    println!(
        "guaranteed  {} identities · {gtd_demand} slots · ladder {base_total} + score {bonus_total} by phase, grants {granted_gtd}, leaders assumed {leaders}",
        per.len()
    );
    if policy.max_per_identity > 0 {
        let hits = held
            .iter()
            .filter(|(_, slots)| slots.total() >= policy.max_per_identity)
            .count();
        println!(
            "ceiling     {hits} held to {} — the totals already apply it",
            policy.max_per_identity
        );
    }
    println!(
        "per holder  gtd min {} · median {} · p90 {} · max {}",
        pick(0.0).unwrap_or(0),
        pick(0.5).unwrap_or(0),
        pick(0.9).unwrap_or(0),
        pick(1.0).unwrap_or(0)
    );
    let top: Vec<String> = per
        .iter()
        .rev()
        .take(10)
        .map(|(gtd, fcfs)| format!("{gtd}+{fcfs}"))
        .collect();
    println!("top ten     {} (gtd+fcfs)", top.join(" · "));
    println!(
        "gtd         demand {gtd_demand} of {} · headroom {:+}",
        reserves.gtd,
        i64::from(reserves.gtd) - i64::from(gtd_demand)
    );

    // Crossover: a guaranteed holder who also holds a partner. Unknowable before bonding.
    if policy.partner_bonus > 0 {
        println!();
        println!(
            "{:>8} {:>10} {:>8} {:>10}",
            "overlap", "crossover", "adds", "headroom"
        );
        for share in OVERLAPS {
            let crossing = (gtd.len() as f64 * share).round() as u32;
            let adds = crossing * policy.partner_bonus;
            println!(
                "{:>7.0}% {crossing:>10} {adds:>8} {:>+10}",
                share * 100.0,
                i64::from(reserves.gtd) - i64::from(gtd_demand + adds)
            );
        }
    }

    // First come: holders the guarantee does not reach, the lapsed door from the bundle, and the
    // partner door from the censuses.
    let lapsed = counts
        .iter()
        .filter(|subject| !subject.enabled && subject.lapsed)
        .count() as u32;
    let lapsed_demand = lapsed * policy.lapsed_mints;
    let held_fcfs: Vec<&(&Counts, holder_behaviour::Slots)> =
        held.iter().filter(|(_, slots)| slots.fcfs > 0).collect();
    let held_fcfs_demand: u32 = held_fcfs.iter().map(|(_, slots)| slots.fcfs).sum();
    let below_minimum = held_fcfs
        .iter()
        .filter(|(subject, _)| subject.current < policy.gtd_min_held)
        .count();
    let fcfs_demand = held_fcfs_demand + lapsed_demand;
    let nothing = held
        .iter()
        .filter(|(subject, slots)| subject.enabled && slots.total() == 0)
        .count();

    // Everyone shows up and takes everything they are allowed: the upper bound each reserve
    // has to be read against.
    println!();
    println!("everyone shows up (partners excluded: their turnout is the sweep below)");
    println!(
        "{:>10} {:>11} {:>8} {:>9} {:>10}",
        "phase", "identities", "mints", "reserve", "headroom"
    );
    println!(
        "{:>10} {:>11} {gtd_demand:>8} {:>9} {:>+10}",
        "gtd",
        per.len(),
        reserves.gtd,
        i64::from(reserves.gtd) - i64::from(gtd_demand)
    );
    println!(
        "{:>10} {:>11} {held_fcfs_demand:>8}",
        "fcfs held",
        held_fcfs.len()
    );
    println!("{:>10} {lapsed:>11} {lapsed_demand:>8}", "fcfs lapse");
    println!(
        "{:>10} {:>11} {fcfs_demand:>8} {:>9} {:>+10}",
        "fcfs",
        held_fcfs.len() as u32 + lapsed,
        reserves.fcfs,
        i64::from(reserves.fcfs) - i64::from(fcfs_demand)
    );
    println!(
        "grants      {granted_gtd} guaranteed and {granted_fcfs} first-come slots as granted, inside the totals above (a guaranteed grant under the minimum held is counted first come there)"
    );
    println!("nothing     {nothing} holders are allocated no slot: the public phase is theirs");
    if policy.gtd_min_held > 0 {
        println!(
            "note        {below_minimum} holders are first come only because they hold fewer than {}",
            policy.gtd_min_held
        );
    }
    println!();
    println!(
        "lapsed      {lapsed} hold none now and earned the lapsed marker · {lapsed_demand} first-come mints"
    );
    if policy.partner_mints > 0 && !partner_paths.is_empty() {
        let book = load_partners(partner_paths)?;
        let eligible = book
            .wallets
            .values()
            .filter(|wallet| wallet.largest >= partner_gate)
            .count() as u32;
        println!(
            "partners    {eligible} wallets hold ≥{partner_gate} of one partner collection · {} each",
            policy.partner_mints
        );
        println!(
            "{:>8} {:>11} {:>8} {:>9}",
            "turnout", "identities", "fcfs", "vs fcfs"
        );
        for share in TURNOUTS {
            let identities = (f64::from(eligible) * share).round() as u32;
            let demand = lapsed_demand + identities * policy.partner_mints;
            println!(
                "{:>7.1}% {identities:>11} {demand:>8} {:>8.2}x",
                share * 100.0,
                f64::from(demand) / f64::from(reserves.fcfs.max(1))
            );
        }
        println!(
            "note        first come is a race once it passes 1.00x: the reserve does not grow."
        );
    }
    Ok(())
}

/// Who holds what they hold, and how they got it: the question a holding rule has to answer
/// before it can be bought. Every holder now, by size, split by where their open Perps came from
/// — all from the mint, all off the market, or both — with the score each cohort reaches, and
/// how many of each a viability threshold would guarantee.
///
/// "Recent" is an open lot acquired in the last 90 days of the reading: what a run on the floor
/// ahead of a snapshot would look like, measured on the one that already happened.
fn print_cohorts(
    bundle: &Bundle<'_>,
    catalogue: &LoadedCatalogue,
    policy: Allocation,
) -> Result<(), String> {
    const RECENT_SLOTS: u64 = 90 * 86_400;
    const BANDS: [(u32, u32, &str); 6] = [
        (1, 1, "1"),
        (2, 2, "2"),
        (3, 4, "3–4"),
        (5, 9, "5–9"),
        (10, 24, "10–24"),
        (25, u32::MAX, "25+"),
    ];
    const COHORTS: [&str; 3] = ["minted", "mixed", "bought"];
    const THRESHOLDS: [u32; 7] = [0, 5_000, 8_000, 10_000, 12_000, 15_000, 20_000];

    struct Holder {
        held: u32,
        cohort: usize,
        recent: u32,
        score: u32,
    }
    let cursor = bundle.holder_cursor();
    let mut holders: Vec<Holder> = Vec::new();
    for subject in bundle.subjects() {
        let subject = subject.map_err(|error| format!("read subject: {error}"))?;
        let lots: Vec<Lot> = subject.lots().collect();
        let held = held_at(&lots, cursor);
        if held == 0 {
            continue;
        }
        let open: Vec<&Lot> = lots
            .iter()
            .filter(|lot| lot.disposed_at.is_none())
            .collect();
        let minted = open.iter().filter(|lot| lot.from_mint).count();
        let cohort = match (minted, open.len() - minted) {
            (_, 0) => 0,
            (0, _) => 2,
            _ => 1,
        };
        let recent = open
            .iter()
            .filter(|lot| lot.acquired_at + RECENT_SLOTS > cursor)
            .count() as u32;
        let scored = score(
            bundle,
            &subject,
            |index| catalogue.role_at(usize::from(index)),
            |index| catalogue.points_at(usize::from(index)),
            policy,
        )
        .map_err(|error| error.to_string())?;
        holders.push(Holder {
            held,
            cohort,
            recent,
            score: scored.points,
        });
    }

    let median = |scores: &mut Vec<u32>| -> String {
        if scores.is_empty() {
            return "—".to_string();
        }
        scores.sort_unstable();
        scores[scores.len() / 2].to_string()
    };
    println!();
    println!("holders now, by where their open Perps came from (median score in brackets)");
    println!(
        "{:>7} {:>16} {:>16} {:>16} {:>10}",
        "held", "minted", "mixed", "bought", "recent≤90d"
    );
    for (low, high, label) in BANDS {
        let band: Vec<&Holder> = holders
            .iter()
            .filter(|holder| holder.held >= low && holder.held <= high)
            .collect();
        let cells: Vec<String> = (0..COHORTS.len())
            .map(|cohort| {
                let mut scores: Vec<u32> = band
                    .iter()
                    .filter(|holder| holder.cohort == cohort)
                    .map(|holder| holder.score)
                    .collect();
                let count = scores.len();
                format!("{count} ({})", median(&mut scores))
            })
            .collect();
        let recent = band.iter().filter(|holder| holder.recent > 0).count();
        println!(
            "{label:>7} {:>16} {:>16} {:>16} {recent:>10}",
            cells[0], cells[1], cells[2]
        );
    }
    let recent_units: u32 = holders.iter().map(|holder| holder.recent).sum();
    println!(
        "recent      {recent_units} Perps held now arrived in the last 90 days, across {} holders",
        holders.iter().filter(|holder| holder.recent > 0).count()
    );

    println!();
    println!("a viability threshold for the guarantee, holders reaching it by cohort");
    println!(
        "{:>8} {:>8} {:>8} {:>8} {:>8}",
        "score ≥", "minted", "mixed", "bought", "total"
    );
    for threshold in THRESHOLDS {
        let counts: Vec<usize> = (0..COHORTS.len())
            .map(|cohort| {
                holders
                    .iter()
                    .filter(|holder| holder.cohort == cohort && holder.score >= threshold)
                    .count()
            })
            .collect();
        println!(
            "{threshold:>8} {:>8} {:>8} {:>8} {:>8}",
            counts[0],
            counts[1],
            counts[2],
            counts.iter().sum::<usize>()
        );
    }
    Ok(())
}

/// How much of the population's earned holding is past tense, and who carries the most: the
/// question `past_share` is tuned against. A share only matters where there is something sold to
/// discount, and only to a subject the gate still lets in.
fn print_past_holdings(
    bundle: &Bundle<'_>,
    catalogue: &LoadedCatalogue,
    policy: Allocation,
) -> Result<(), String> {
    struct Row {
        key: String,
        held: u32,
        rows: u32,
        full: u32,
        delta: i64,
    }
    let cursor = bundle.holder_cursor();
    let mut carriers: Vec<Row> = Vec::new();
    let (mut listed_rows, mut listed_subjects, mut listed_points) = (0u32, 0u32, 0u32);
    for subject in bundle.subjects() {
        let subject = subject.map_err(|error| format!("read subject: {error}"))?;
        let listed: Vec<u16> = subject
            .earned()
            .filter(|earned| earned.standing == holder_behaviour::Standing::Listed)
            .map(|earned| earned.achievement)
            .collect();
        if !listed.is_empty() {
            listed_subjects += 1;
            listed_rows += listed.len() as u32;
            listed_points += listed
                .iter()
                .filter_map(|index| catalogue.points_at(usize::from(*index)))
                .sum::<u32>();
        }
        let past: Vec<u16> = subject
            .earned()
            .filter(|earned| earned.standing.is_lapsed_holding())
            .map(|earned| earned.achievement)
            .collect();
        if past.is_empty() {
            continue;
        }
        let full: u32 = past
            .iter()
            .filter_map(|index| catalogue.points_at(usize::from(*index)))
            .sum();
        // What the share costs this subject: its score as if every holding were still held,
        // against its score under the policy.
        let score_with = |policy: Allocation| {
            score(
                bundle,
                &subject,
                |index| catalogue.role_at(usize::from(index)),
                |index| catalogue.points_at(usize::from(index)),
                policy,
            )
            .map(|scored| scored.points)
            .map_err(|error| error.to_string())
        };
        let as_held = score_with(Allocation {
            past_share: Some(1.0),
            ..policy
        })?;
        let now = score_with(policy)?;
        let lots: Vec<Lot> = subject.lots().collect();
        carriers.push(Row {
            key: subject.key_str(),
            held: held_at(&lots, cursor),
            rows: past.len() as u32,
            full,
            delta: i64::from(now) - i64::from(as_held),
        });
    }
    let rows: u32 = carriers.iter().map(|row| row.rows).sum();
    let gated = carriers.iter().filter(|row| row.held > 0).count();
    println!();
    println!(
        "past        {} subjects carry {rows} holdings they no longer hold or have listed · {gated} still hold a Perp",
        carriers.len()
    );
    println!(
        "listed      {listed_subjects} subjects hold {listed_rows} holdings only through Perps for sale · {listed_points} points that unlisting restores"
    );
    carriers.sort_by(|left, right| left.delta.cmp(&right.delta).then(left.key.cmp(&right.key)));
    println!(
        "{:>12} {:>5} {:>5} {:>7} {:>8}",
        "subject", "held", "past", "points", "score Δ"
    );
    for row in carriers.iter().take(10) {
        println!(
            "{:>12} {:>5} {:>5} {:>7} {:>+8}",
            &row.key[..row.key.len().min(12)],
            row.held,
            row.rows,
            row.full,
            row.delta
        );
    }
    Ok(())
}

/// One subject under the policy: every earned row with its outcome, the score, the holding
/// read at the bundle's cursor, and the mints. Leaders and partners are live reads this bundle
/// cannot answer, so they are left out and said to be.
fn explain_subject(
    bundle: &Bundle<'_>,
    catalogue: &LoadedCatalogue,
    policy: Allocation,
    gate: &[&Status],
    lapsed: Option<u16>,
    key: &str,
) -> Result<(), String> {
    let subject = bundle
        .find(key)
        .map_err(|error| format!("read {key}: {error}"))?
        .ok_or_else(|| format!("{key} is not a subject in this bundle"))?;
    let trace = holder_behaviour::explain(
        bundle,
        key,
        subject
            .earned()
            .map(|earned| (earned.achievement, earned.standing)),
        |index| catalogue.role_at(usize::from(index)),
        |index| catalogue.points_at(usize::from(index)),
        policy,
    );
    println!();
    println!("subject     {key}");
    println!(
        "  {:<22} {:<8} {:<10} {:>6} {:>6}",
        "achievement", "tense", "outcome", "points", "worth"
    );
    for row in &trace.rows {
        println!(
            "  {:<22} {:<8} {:<10} {:>6} {:>6}",
            catalogue.id_at(usize::from(row.achievement)).unwrap_or("?"),
            row.standing.as_str(),
            row.outcome.as_str(),
            row.points.map_or("—".to_string(), |p| p.to_string()),
            row.worth
        );
    }
    let scored = trace.score.map_err(|error| error.to_string())?;
    let lots: Vec<Lot> = subject.lots().collect();
    let current = held_at(&lots, bundle.holder_cursor());
    let no_traits: [(String, String); 0] = [];
    let view = StatusView {
        collection: bundle.collection(),
        units: u64::from(current),
        traits: &no_traits,
    };
    let enabled = ask_all(gate, &view)?;
    let once = lapsed.is_some_and(|index| subject.earned().any(|e| e.achievement == index));
    let granted = grants(
        subject
            .earned()
            .map(|earned| (earned.achievement, earned.standing, earned.held)),
        |index| catalogue.grant_at(usize::from(index)),
    );
    let seasoned = policy.seasoning.map_or(0, |seasoning| {
        seasoned_at(&lots, bundle.built_at(), seasoning.days)
    });
    let counts = scored
        .clone()
        .counts(
            peak_concurrent(&lots, bundle.holder_cursor()),
            current,
            enabled,
            false,
        )
        .lapsed(once)
        .granted(granted)
        .seasoned(seasoned);
    let ids = |list: &[u16]| {
        list.iter()
            .map(|index| {
                catalogue
                    .id_at(usize::from(*index))
                    .unwrap_or("?")
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    println!("score       {}", counts.score);
    println!("core        {}", ids(&scored.core));
    println!("credits     {}", ids(&scored.credits));
    println!("held        {}", ids(&scored.held));
    println!(
        "held now    {current} · {seasoned} seasoned · ladder reads {} · gate {}",
        counts.ladder_units(policy),
        if enabled { "open" } else { "shut" }
    );
    let granting: Vec<String> = trace
        .rows
        .iter()
        .filter(|row| row.standing != holder_behaviour::Standing::Past)
        .filter_map(|row| {
            catalogue
                .grant_at(usize::from(row.achievement))
                .map(|grant| {
                    format!(
                        "{} {}",
                        catalogue.id_at(usize::from(row.achievement)).unwrap_or("?"),
                        match (grant.gtd, grant.fcfs) {
                            (gtd, 0) => format!("+{gtd} gtd"),
                            (0, fcfs) => format!("+{fcfs} fcfs"),
                            (gtd, fcfs) => format!("+{gtd} gtd +{fcfs} fcfs"),
                        }
                    )
                })
        })
        .collect();
    println!("grants      {}", granting.join(" · "));
    let slots = slots(&counts, policy);
    println!(
        "allocation  {} gtd + {} fcfs = {} (before any live leader or partner read)",
        slots.gtd,
        slots.fcfs,
        slots.total()
    );
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
            wallet.largest = wallet.largest.max(held.balance);
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
    max_score: u32,
    step: u32,
    max_flavour: u32,
) -> Result<(), String> {
    let mut out = String::from(
        "gtd_score,fcfs_score,fcfs_rarest,gtd,fcfs,ungated,gtd_demand,fcfs_demand,gtd_headroom,fcfs_headroom\n",
    );
    for score in (0..=max_score).step_by(step as usize) {
        for flavour in 0..=max_flavour {
            let candidate = Allocation {
                gtd_score: score,
                fcfs_rarest: flavour,
                ..policy
            };
            let tally = tally(counts, candidate);
            let gtd_demand = tally.gtd * policy.per_identity;
            let fcfs_demand = tally.fcfs * policy.per_identity;
            let gtd_headroom = i64::from(reserves.gtd) - i64::from(gtd_demand);
            let fcfs_headroom = i64::from(reserves.fcfs) - i64::from(fcfs_demand);
            writeln!(
                out,
                "{score},{fcfs_score},{flavour},{gtd},{fcfs},{ungated},{gtd_demand},{fcfs_demand},{gtd_headroom},{fcfs_headroom}",
                fcfs_score = candidate.fcfs_score,
                gtd = tally.gtd,
                fcfs = tally.fcfs,
                ungated = tally.ungated
            )
            .map_err(|error| format!("write grid: {error}"))?;
        }
    }
    std::fs::write(path, out).map_err(|error| format!("write {path}: {error}"))
}
