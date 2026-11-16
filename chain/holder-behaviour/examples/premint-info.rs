//! The public information page for a premint: the launch, how the guaranteed and first-come
//! allowances are earned, and the two points tables — written from the catalogue itself, so the
//! page and the rule cannot drift apart.
//!
//! ```text
//! cargo run --release -p holder-behaviour --example premint-info -- \
//!     --catalogue ../cnft.dev-workers/config/achievements/perps.toml \
//!     --bundle .tmp/perps-tracks-now.bundle \
//!     --gtd-claimed 1178 --fcfs-claimed 492 \
//!     --out .tmp/premint-info.html
//! ```
//!
//! The bundle is optional and only adds how many holders have each achievement. The timeline,
//! prices and dates come from the catalogue's `[launch]`, and the allowances and points tables
//! from `[allocation]` and the entries — nothing on the page is transcribed by hand. The claimed
//! figures are the modelling tool's (`allocate`), passed in rather than recomputed: they need the
//! listings and the token reads that tool owns.

mod support;

use std::fmt::Write as _;
use std::process::ExitCode;

use holder_behaviour::{
    Allocation, Bundle, Grouping, Launch, LaunchPhase, Maturity, PhaseKind, Track,
    catalogue_digest, holding_mints,
};
use support::catalogue::LoadedCatalogue;

/// The loci in reading order, as the page names them.
const LOCI: [(&str, &str); 5] = [
    ("provenance", "Origin"),
    ("conviction", "Resolve"),
    ("curation", "Lineage"),
    ("market", "Hustle"),
    ("stake", "Coin"),
];

struct Args {
    catalogue: String,
    bundle: Option<String>,
    out: String,
    gtd_claimed: Option<u32>,
    fcfs_claimed: Option<u32>,
}

struct Row {
    title: String,
    statement: String,
    axis: String,
    points: u32,
    holders: Option<(u32, u32)>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("premint-info: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let catalogue = LoadedCatalogue::load(&args.catalogue)?;
    let policy = catalogue
        .plan()
        .map(|plan| plan.allocation)
        .ok_or_else(|| format!("{} has no [allocation] block", args.catalogue))?;
    let bytes = match &args.bundle {
        Some(path) => Some(std::fs::read(path).map_err(|error| format!("read {path}: {error}"))?),
        None => None,
    };
    let bundle = match &bytes {
        Some(bytes) => {
            let bundle = Bundle::open(bytes).map_err(|error| format!("open bundle: {error}"))?;
            if bundle.catalogue_digest() != catalogue_digest(catalogue.ids()) {
                return Err("the bundle was built from a different catalogue".to_string());
            }
            Some(bundle)
        }
        None => None,
    };

    let mut tables: [(Track, Vec<Row>); 2] =
        [(Track::Bloodline, Vec::new()), (Track::Clout, Vec::new())];
    for index in 0..catalogue.len() {
        let points = catalogue.points_at(index).unwrap_or(0);
        // A grant-only entry (a Leader piece) earns its slots elsewhere on the page.
        if points == 0 {
            continue;
        }
        let row = Row {
            title: catalogue.title_at(index).unwrap_or_default().to_string(),
            statement: catalogue
                .statement_at(index)
                .unwrap_or_default()
                .to_string(),
            axis: catalogue.axis_at(index).unwrap_or_default().to_string(),
            points,
            holders: bundle.as_ref().and_then(|bundle| {
                u16::try_from(index)
                    .ok()
                    .and_then(|index| bundle.rarity(index))
                    .map(|rarity| (rarity.holders_with, rarity.population))
            }),
        };
        let track = catalogue.track_at(index);
        if let Some((_, rows)) = tables.iter_mut().find(|(t, _)| *t == track) {
            rows.push(row);
        }
    }
    let leaders: Vec<(String, holder_behaviour::Grant)> = (0..catalogue.len())
        .filter_map(|index| {
            catalogue
                .grant_at(index)
                .filter(|grant| grant.per_piece && grant.gtd > 0)
                .map(|grant| {
                    (
                        catalogue.title_at(index).unwrap_or_default().to_string(),
                        grant,
                    )
                })
        })
        .collect();
    let launch = catalogue
        .launch()
        .ok_or_else(|| format!("{} has no [launch] block", args.catalogue))?;
    if launch.phases.is_empty() {
        return Err(format!("{}: [launch] carries no phases", args.catalogue));
    }

    let html = render(&args, policy, launch, &tables, &leaders);
    std::fs::write(&args.out, html).map_err(|error| format!("write {}: {error}", args.out))?;
    eprintln!("premint-info: wrote {}", args.out);
    Ok(())
}

fn render(
    args: &Args,
    policy: Allocation,
    launch: &Launch,
    tables: &[(Track, Vec<Row>); 2],
    leaders: &[(String, holder_behaviour::Grant)],
) -> String {
    let mut html = String::new();
    let total: u32 = launch.phases.iter().map(|phase| phase.mints).sum();
    let snapshot = date(launch.snapshot_at);
    let _ = write!(
        html,
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Perplings premint — how allocation works</title>
<style>{STYLE}</style></head>
<body><main class="viz-root">
<header>
  <p class="kicker">Perplings · premint</p>
  <h1>How the allocation works</h1>
  <p class="lede">{lede} The snapshot is on <b>{snapshot}</b>.</p>
</header>
"#,
        lede = match policy.bonus_fcfs {
            true =>
                "Holding Perps earns <b>GTD</b> free mints; the two points tables earn <b>FCFS</b> access.",
            false =>
                "Holding Perps earns <b>GTD</b> free mints; <b>Bloodline</b> adds to it and <b>Clout</b> earns <b>FCFS</b> access.",
        },
    );

    // ── The timeline: the catalogue's phases, then public ───────────────────────────────────
    let tiers: String = launch
        .phases
        .iter()
        .enumerate()
        .map(|(index, phase)| phase_tier(index, phase, launch.og_fcfs_target, policy.bonus_fcfs))
        .collect();
    let phases = launch.phases.len();
    let public = match launch.public_price_cents {
        Some(cents) => format!(
            " When the last phase ends, public access opens for whatever remains, at {}.",
            money(cents)
        ),
        None => " When the last phase ends, public access opens for whatever remains.".to_string(),
    };
    let _ = write!(
        html,
        r#"<section><h2>The mint, in {phases} phases</h2><div class="tiers">
{tiers}</div>
<p class="note">{total} mints across the {phases} phases.{public}</p></section>
"#,
        total = thousands(total),
    );

    // ── Where the OG allocation stands ───────────────────────────────────────────────────────
    let og_gtd = launch
        .phases
        .iter()
        .find(|phase| phase.kind == PhaseKind::Gtd && phase.audience.is_none())
        .map_or(0, |phase| phase.mints);
    let og_fcfs = launch.og_fcfs_target.unwrap_or(0);
    if args.gtd_claimed.is_some() || args.fcfs_claimed.is_some() {
        let bar = |claimed: Option<u32>, of: u32, class: &str, label: &str| -> String {
            match claimed {
                None => String::new(),
                Some(claimed) => {
                    let width = (f64::from(claimed) / f64::from(of.max(1)) * 100.0).min(100.0);
                    format!(
                        r#"<div class="stand"><span class="what">{label}</span><span class="track"><span class="fill {class}" style="width:{width:.1}%"></span></span><span class="n">{} of {}</span></div>"#,
                        thousands(claimed),
                        thousands(of)
                    )
                }
            }
        };
        let _ = write!(
            html,
            r#"<section><h2>Where OG allocation stands</h2>{}{}
<p class="note">An estimate from today's holdings, if every eligible holder takes everything they've earned. It moves as people buy, list, link wallets and earn.</p></section>
"#,
            bar(args.gtd_claimed, og_gtd, "bloodline", "GTD"),
            bar(args.fcfs_claimed, og_fcfs, "clout", "FCFS"),
        );
    }

    // ── Guaranteed ───────────────────────────────────────────────────────────────────────────
    let ladder_points = [5u32, 10, 20, 30, 50, 100, 150, 250, 500];
    let ladder: String = ladder_points
        .iter()
        .map(|units| {
            format!(
                "<tr><td>{units}</td><td class=\"num\">{}</td></tr>",
                holding_mints(&policy, *units)
            )
        })
        .collect();
    let token: String = policy
        .token
        .map(|rule| {
            [3u64, 10, 20, 40, 60, 80, 100, 140]
                .iter()
                .map(|m| {
                    format!(
                        "<tr><td>{m}M</td><td class=\"num\">{}</td></tr>",
                        rule.slots(m * 1_000_000)
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let lp = policy.token.map_or(100, |rule| rule.lp_percent);
    let early = policy
        .token
        .and_then(|rule| rule.early)
        .map_or(String::new(), |early| match early.at_least % 1_000_000 {
            0 => format!(
                " Holding {} Perps brings the first slot down to {}M.",
                early.perps,
                early.at_least / 1_000_000
            ),
            _ => format!(
                " Holding {} Perps brings the first slot down to {} units.",
                early.perps, early.at_least
            ),
        });
    let leader_rows: String = leaders
        .iter()
        .map(|(title, grant)| {
            format!(
                "<li><b>{}</b> — {} GTD for every piece you hold.</li>",
                esc(title),
                grant.gtd
            )
        })
        .collect();
    let maturity = match policy.seasoning {
        Some(s) if s.maturing_days == s.mature_days => format!(
            "A Perp counts in full once it has been in your wallet for <b>{} days</b>; before that it counts {:.0}%. Buy by <b>{}</b> to count in full at the snapshot. Moving Perps to another wallet starts the clock again.",
            s.mature_days,
            s.share(Maturity::New) * 100.0,
            date(launch.mature_by(Some(s)))
        ),
        Some(s) => format!(
            "A Perp counts in full once it has been in your wallet for <b>{} days</b>; from {} days it counts {:.0}%, before that {:.0}%. Buy by <b>{}</b> to count in full at the snapshot. Moving Perps to another wallet starts the clock again.",
            s.mature_days,
            s.maturing_days,
            s.share(Maturity::Maturing) * 100.0,
            s.share(Maturity::New) * 100.0,
            date(launch.mature_by(Some(s)))
        ),
        None => "Every Perp held at the snapshot counts.".to_string(),
    };
    let bloodline_table = points_table(
        "Bloodline",
        "Made in the lab, and never left. Where you came from, and how long you stayed.",
        "bloodline",
        if policy.bonus_fcfs { "FCFS" } else { "GTD" },
        &policy.bonus,
        policy.bonus_tail,
        &tables[0].1,
    );
    let _ = write!(
        html,
        r#"<section class="track-section"><h2>GTD — free mints</h2>
<p>{sources} You need <b>{min} Perps</b> for GTD (a Leader or $PERP qualifies on its own).</p>
<div class="cols">
<div class="card"><h3>Holding Perps</h3><table><thead><tr><th>Perps held</th><th class="num">GTD</th></tr></thead><tbody>{ladder}</tbody></table>
<p class="note">Every rung is the Perps held now; the table reads the published ladder. Listed Perps don't count — unlist them before the snapshot.</p></div>
<div class="card"><h3>Holding $PERP</h3><table><thead><tr><th>$PERP held</th><th class="num">GTD</th></tr></thead><tbody>{token}</tbody></table>
<p class="note">Wallet, staked and locked $PERP count in full; $PERP in a liquidity pool counts at {lp}%. No Perp needed.{early}</p></div>
</div>
<div class="card"><h3>Leaders</h3><ul>{leader_rows}</ul><p class="note">A Leader earns GTD whatever else you hold.</p></div>
<div class="card"><h3>Maturity</h3><p>{maturity}</p></div>
"#,
        min = policy.gtd_min_held,
        sources = match policy.bonus_fcfs {
            true =>
                "GTD mints come from what you hold now: the Perps in your wallet, your Leaders, and your $PERP.",
            false =>
                "GTD mints come from what can't be bought the night before: the Perps you hold, your Leaders, your $PERP, and your Bloodline.",
        },
    );
    if policy.bonus_fcfs {
        html.push_str("</section>\n");
    } else {
        html.push_str(&bloodline_table);
        let _ = writeln!(
            html,
            r#"<p class="note">Under {} Perps held, Bloodline earns FCFS mints instead.</p></section>"#,
            policy.gtd_min_held
        );
    }

    // ── FCFS ─────────────────────────────────────────────────────────────────────────────────
    let small_holder = match (policy.holder_fcfs, policy.gtd_min_held) {
        (0, _) | (_, 0) => String::new(),
        (mints, min) => format!(
            " Holders with fewer than {min} Perps also get {mints} FCFS from what they hold."
        ),
    };
    let fcfs_intro = match policy.bonus_fcfs {
        true => {
            "FCFS mints come from the two points tables, each on its own: <b>Bloodline</b> — the mint and time held — and <b>Clout</b> — what you've collected and done. The guarantee is what you hold; both tables are a chase."
        }
        false => {
            "FCFS mints come from <b>Clout</b>: what you've collected and done — the sets, the rare finds, the factions, the market. It's the part you can still chase before the snapshot."
        }
    };
    let _ = write!(
        html,
        r#"<section class="track-section"><h2><span class="chip bloodline"></span><span class="chip clout"></span>FCFS — early access</h2>
<p>{fcfs_intro}{small_holder}</p>
"#,
    );
    if policy.bonus_fcfs {
        html.push_str(&bloodline_table);
    }
    html.push_str(&points_table(
        "Clout",
        "What you did with them. The collection, the factions, the market, the Coin hunt.",
        "clout",
        "FCFS",
        &policy.clout,
        policy.clout_tail,
        &tables[1].1,
    ));
    html.push_str("</section>\n");

    // ── Wallets ──────────────────────────────────────────────────────────────────────────────
    let wallets = match (policy.grouping, policy.gtd_min_held) {
        (Grouping::AsOne, min) if min > 0 => format!(
            "All your linked wallets are read as one holder — their Perps are summed, so wallets that each hold fewer than {min} Perps can reach GTD together, and a shared achievement counts once. Your mints go to that Robinhood wallet."
        ),
        (Grouping::AsOne, _) => "All your linked wallets are read as one holder — their Perps are summed and a shared achievement counts once. Your mints go to that Robinhood wallet.".to_string(),
        (Grouping::BestOf, min) if min > 0 => format!(
            "Each wallet earns on its own and the best reading of the group wins, so linking never costs you anything — and wallets that each hold fewer than {min} Perps can combine to reach GTD. Your mints go to that Robinhood wallet."
        ),
        (Grouping::BestOf, _) => "Each wallet earns on its own and the best reading of the group wins, so linking never costs you anything. Your mints go to that Robinhood wallet.".to_string(),
    };
    let _ = write!(
        html,
        r#"<section><h2>Your wallets</h2>
<div class="card"><p><b>Link all your Cardano wallets to one Robinhood wallet.</b> {wallets}</p></div>
</section>
</main></body></html>
"#,
    );
    html
}

fn tier(number: &str, name: &str, size: &str, who: &str, class: &str) -> String {
    format!(
        r#"<div class="tier {class}"><span class="no">{number}</span><b>{}</b><span class="size">{size}</span><span class="who">{}</span></div>"#,
        esc(name),
        esc(who)
    )
}

/// One phase of the catalogue's launch, as a tier card.
fn phase_tier(index: usize, phase: &LaunchPhase, og_fcfs: Option<u32>, bonus_fcfs: bool) -> String {
    let class = match (phase.kind, phase.audience.is_some()) {
        (PhaseKind::Gtd, false) => "bloodline",
        (PhaseKind::Gtd, true) => "neutral",
        (PhaseKind::Fcfs, _) => "clout",
    };
    let size = match phase.price_cents.filter(|cents| *cents > 0) {
        Some(cents) => format!("{} at {}", thousands(phase.mints), money(cents)),
        None => format!("{} free mints", thousands(phase.mints)),
    };
    let who = match (phase.kind, &phase.audience) {
        (PhaseKind::Gtd, Some(audience)) => format!("For {audience}."),
        (PhaseKind::Gtd, None) => match bonus_fcfs {
            true => "For Perps and $PERP holders, earned by what you hold.".to_string(),
            false => "For Perps and $PERP holders, earned by what you hold and your Bloodline."
                .to_string(),
        },
        (PhaseKind::Fcfs, _) => {
            let tables = match bonus_fcfs {
                true => "Bloodline and Clout",
                false => "Clout",
            };
            match og_fcfs.filter(|target| *target > 0) {
                Some(target) => format!(
                    "While it lasts. About {} for OG holders, earned by {tables}.",
                    thousands(target)
                ),
                None => format!("While it lasts, earned by {tables}."),
            }
        }
    };
    tier(
        &format!("Phase {}", index + 1),
        &phase.name,
        &size,
        &who,
        class,
    )
}

/// US cents as a price: `$8`, `$8.50`.
fn money(cents: u32) -> String {
    match cents % 100 {
        0 => format!("${}", cents / 100),
        rest => format!("${}.{rest:02}", cents / 100),
    }
}

/// A unix second as a calendar date, UTC.
fn date(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn points_table(
    name: &str,
    gist: &str,
    class: &str,
    earns: &str,
    rungs: &holder_behaviour::Rungs,
    tail: Option<holder_behaviour::Tail>,
    rows: &[Row],
) -> String {
    let mut html = String::new();
    let steps: Vec<String> = rungs
        .as_slice()
        .iter()
        .map(|rung| {
            format!(
                r#"<li><span class="at">{}</span><span>+{} {earns}</span></li>"#,
                thousands(rung.at_least),
                rung.mints
            )
        })
        .collect();
    let beyond = tail.map_or(String::new(), |tail| {
        format!(
            "<p class=\"note\">One more {earns} for every {} points beyond the last step.</p>",
            thousands(tail.every)
        )
    });
    let max = rows.iter().map(|row| row.points).max().unwrap_or(1).max(1);
    let _ = write!(
        html,
        r#"<div class="card"><h3>{name} points</h3><p class="gist">{gist}</p>
<ol class="steps {class}">{steps}</ol>{beyond}
<table class="points"><thead><tr><th>Achievement</th><th class="num">Points</th><th class="num">Holders</th></tr></thead><tbody>
"#,
        steps = steps.join(""),
    );
    for (axis, locus) in LOCI {
        let mut group: Vec<&Row> = rows.iter().filter(|row| row.axis == axis).collect();
        if group.is_empty() {
            continue;
        }
        group.sort_by(|a, b| b.points.cmp(&a.points).then(a.title.cmp(&b.title)));
        let _ = writeln!(
            html,
            r#"<tr class="locus"><th colspan="3">{locus}</th></tr>"#
        );
        for row in group {
            let width = f64::from(row.points) / f64::from(max) * 100.0;
            let holders = match row.holders {
                Some((_, 0)) => "coming soon".to_string(),
                Some((with, population)) => format!(
                    "{:.0}%",
                    f64::from(with) / f64::from(population.max(1)) * 100.0
                ),
                None => String::new(),
            };
            let _ = writeln!(
                html,
                r#"<tr><td><b>{}</b><span class="stmt">{}</span></td><td class="num"><div class="pts"><span class="bar"><span class="fill {class}" style="width:{width:.1}%"></span></span><span>{}</span></div></td><td class="num muted">{holders}</td></tr>"#,
                esc(&row.title),
                esc(&row.statement),
                thousands(row.points)
            );
        }
    }
    html.push_str(
        r#"</tbody></table><p class="note">Holders: the share of past and present Perps holders who have earned it. Rare traits and sets count while you hold them.</p></div>
"#,
    );
    html
}

const STYLE: &str = r#"
.viz-root{color-scheme:light;--surface-1:#fcfcfb;--surface-2:#f2f1ee;--rule:#e2e0db;--text-primary:#0b0b0b;--text-secondary:#52514e;--text-muted:#7a7873;--bloodline:#2a78d6;--clout:#eb6834;--track:#ebe9e4}
@media (prefers-color-scheme:dark){:root:where(:not([data-theme="light"])) .viz-root{color-scheme:dark;--surface-1:#1a1a19;--surface-2:#232321;--rule:#34342f;--text-primary:#f0efec;--text-secondary:#c3c2b7;--text-muted:#8f8e86;--bloodline:#3987e5;--clout:#d95926;--track:#2c2c29}}
:root[data-theme="dark"] .viz-root{color-scheme:dark;--surface-1:#1a1a19;--surface-2:#232321;--rule:#34342f;--text-primary:#f0efec;--text-secondary:#c3c2b7;--text-muted:#8f8e86;--bloodline:#3987e5;--clout:#d95926;--track:#2c2c29}
html,body{margin:0;background:var(--surface-1)}
.viz-root{font:15px/1.55 ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif;color:var(--text-primary);background:var(--surface-1);max-width:920px;margin:0 auto;padding:32px 22px 56px}
.kicker{margin:0;font-size:12px;letter-spacing:.08em;text-transform:uppercase;color:var(--text-muted)}
h1{font-size:30px;margin:4px 0 8px}.lede{color:var(--text-secondary);margin:0 0 28px;font-size:16px}
section{margin:0 0 36px}h2{font-size:21px;margin:0 0 12px;display:flex;align-items:center}h3{font-size:16px;margin:0 0 8px}
.chip{display:inline-block;width:12px;height:12px;border-radius:3px;margin-right:8px}
.chip.bloodline,.fill.bloodline{background:var(--bloodline)}.chip.clout,.fill.clout{background:var(--clout)}
.tiers{display:grid;grid-template-columns:repeat(auto-fit,minmax(190px,1fr));gap:10px}
.tier{background:var(--surface-2);border-radius:12px;padding:14px;display:flex;flex-direction:column;gap:4px;border-top:4px solid var(--rule)}
.tier.bloodline{border-top-color:var(--bloodline)}.tier.clout{border-top-color:var(--clout)}
.tier .no{font-size:12px;color:var(--text-muted)}.tier .size{font-size:18px;font-weight:600}.tier .who{color:var(--text-secondary);font-size:13px}
.stand{display:grid;grid-template-columns:110px 1fr 120px;align-items:center;gap:12px;margin:0 0 8px}
.stand .what{font-weight:600}.stand .n{text-align:right;font-variant-numeric:tabular-nums;color:var(--text-secondary)}
.track,.bar{display:block;height:10px;background:var(--track);border-radius:5px;overflow:hidden}.bar{height:8px}
.fill{display:block;height:100%;border-radius:0 4px 4px 0}
.cols{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:12px}
.card{background:var(--surface-2);border-radius:12px;padding:14px 16px;margin:0 0 12px}
.card ul{margin:0;padding-left:18px}.gist{color:var(--text-secondary);margin:0 0 10px}
.note{font-size:13px;color:var(--text-muted);margin:8px 0 0}
table{width:100%;border-collapse:collapse}th{text-align:left;font-size:12px;color:var(--text-muted);font-weight:600;padding:6px 6px;border-bottom:1px solid var(--rule)}
td{padding:7px 6px;border-bottom:1px solid var(--rule);vertical-align:middle}.num{text-align:right;font-variant-numeric:tabular-nums}
td b{display:block}.stmt{display:block;color:var(--text-secondary);font-size:13px}.muted{color:var(--text-muted);font-size:13px}
tr.locus th{padding-top:14px;font-size:13px;color:var(--text-primary)}
.pts{display:grid;grid-template-columns:90px 52px;align-items:center;gap:8px;justify-content:end;font-weight:600}
.steps{list-style:none;margin:0 0 12px;padding:0;display:flex;flex-wrap:wrap;gap:8px}
.steps li{background:var(--surface-1);border-radius:999px;padding:4px 12px;font-size:13px;border:1px solid var(--rule)}
.steps .at{font-weight:700;margin-right:6px}
.steps.bloodline li{border-color:var(--bloodline)}.steps.clout li{border-color:var(--clout)}
@media (max-width:560px){.stand{grid-template-columns:80px 1fr 90px}.pts{grid-template-columns:60px 48px}}
"#;

fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let mut parsed = Args {
        catalogue: String::new(),
        bundle: None,
        out: String::new(),
        gtd_claimed: None,
        fcfs_claimed: None,
    };
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().ok_or_else(|| format!("{flag} needs a value"));
        let number = |text: String, flag: &str| -> Result<u32, String> {
            text.parse()
                .map_err(|_| format!("{flag} takes a number, not {text}"))
        };
        match arg.as_str() {
            "--catalogue" => parsed.catalogue = value("--catalogue")?,
            "--bundle" => parsed.bundle = Some(value("--bundle")?),
            "--out" => parsed.out = value("--out")?,
            "--gtd-claimed" => {
                parsed.gtd_claimed = Some(number(value("--gtd-claimed")?, "--gtd-claimed")?);
            }
            "--fcfs-claimed" => {
                parsed.fcfs_claimed = Some(number(value("--fcfs-claimed")?, "--fcfs-claimed")?);
            }
            "-h" | "--help" => {
                println!(
                    "premint-info --catalogue <toml> [--bundle <bundle>] --out <html> \
                     [--gtd-claimed N] [--fcfs-claimed N]"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unexpected argument {other}")),
        }
    }
    if parsed.catalogue.is_empty() || parsed.out.is_empty() {
        return Err("--catalogue and --out are required".to_string());
    }
    Ok(parsed)
}
