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
//! The bundle is optional and only adds how many holders have each achievement. The claimed
//! figures are the modelling tool's (`allocate`), passed in rather than recomputed: they need the
//! listings and the token reads that tool owns.

mod support;

use std::fmt::Write as _;
use std::process::ExitCode;

use holder_behaviour::{Allocation, Bundle, Maturity, Track, catalogue_digest, holding_mints};
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
    og_gtd: u32,
    community_gtd: u32,
    fcfs: u32,
    og_fcfs: u32,
    price: String,
    snapshot: String,
    mature_by: String,
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

    let html = render(&args, policy, &tables, &leaders);
    std::fs::write(&args.out, html).map_err(|error| format!("write {}: {error}", args.out))?;
    eprintln!("premint-info: wrote {}", args.out);
    Ok(())
}

fn render(
    args: &Args,
    policy: Allocation,
    tables: &[(Track, Vec<Row>); 2],
    leaders: &[(String, holder_behaviour::Grant)],
) -> String {
    let mut html = String::new();
    let total = args.og_gtd + args.community_gtd + args.fcfs;
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
  <p class="lede">Holding Perps earns <b>GTD</b> free mints; what you've collected and done earns
  <b>FCFS</b> access. The snapshot is on <b>{snapshot}</b>.</p>
</header>
"#,
        snapshot = esc(&args.snapshot),
    );

    // ── The timeline: three phases, then public ──────────────────────────────────────────────
    let _ = write!(
        html,
        r#"<section><h2>The mint, in three phases</h2><div class="tiers">
{og}{community}{fcfs}</div>
<p class="note">{total} mints across the three phases. GTD that OG Perps holders don't use goes to the community phase. When the FCFS phase ends, public access opens for whatever remains, at {price}.</p></section>
"#,
        og = tier(
            "Phase 1",
            "GTD · OG Perps",
            &format!("{} free mints", thousands(args.og_gtd)),
            "For Perps and $PERP holders, earned by what you hold and your Bloodline.",
            "bloodline",
        ),
        community = tier(
            "Phase 2",
            "GTD · Community",
            &format!("{} free mints", thousands(args.community_gtd)),
            "For the Robinhood partner communities.",
            "neutral",
        ),
        fcfs = tier(
            "Phase 3",
            "FCFS",
            &format!("{} at {}", thousands(args.fcfs), esc(&args.price)),
            &format!(
                "While it lasts. About {} for OG holders, earned by Clout.",
                thousands(args.og_fcfs)
            ),
            "clout",
        ),
        total = thousands(total),
        price = esc(&args.price),
    );

    // ── Where the OG allocation stands ───────────────────────────────────────────────────────
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
            bar(args.gtd_claimed, args.og_gtd, "bloodline", "GTD"),
            bar(args.fcfs_claimed, args.og_fcfs, "clout", "FCFS"),
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
        Some(s) => format!(
            "A Perp counts in full once it has been in your wallet for <b>{} days</b>; from {} days it counts {:.0}%, before that {:.0}%. Buy by <b>{}</b> to count in full at the snapshot. Moving Perps to another wallet starts the clock again.",
            s.mature_days,
            s.maturing_days,
            s.share(Maturity::Maturing) * 100.0,
            s.share(Maturity::New) * 100.0,
            esc(&args.mature_by)
        ),
        None => "Every Perp held at the snapshot counts.".to_string(),
    };
    let _ = write!(
        html,
        r#"<section class="track-section"><h2><span class="chip bloodline"></span>GTD — free mints</h2>
<p>GTD mints come from what can't be bought the night before: the Perps you hold, your Leaders, your $PERP, and your <b>Bloodline</b>. You need <b>{min} Perps</b> for GTD (a Leader or $PERP qualifies on its own).</p>
<div class="cols">
<div class="card"><h3>Holding Perps</h3><table><thead><tr><th>Perps held</th><th class="num">GTD</th></tr></thead><tbody>{ladder}</tbody></table>
<p class="note">One for every 5 Perps up to 50, one for every 10 up to 150, then one for every 25. Listed Perps don't count — unlist them before the snapshot.</p></div>
<div class="card"><h3>Holding $PERP</h3><table><thead><tr><th>$PERP held</th><th class="num">GTD</th></tr></thead><tbody>{token}</tbody></table>
<p class="note">Wallet, staked and locked $PERP count in full; $PERP in a liquidity pool counts at {lp}%. $PERP matures like a Perp. No Perp needed.</p></div>
</div>
<div class="card"><h3>Leaders</h3><ul>{leader_rows}</ul><p class="note">A Leader earns GTD whatever else you hold.</p></div>
<div class="card"><h3>Maturity</h3><p>{maturity}</p></div>
"#,
        min = policy.gtd_min_held,
    );
    html.push_str(&points_table(
        "Bloodline",
        "Made in the lab, and never left. Where you came from, and how long you stayed.",
        "bloodline",
        "GTD",
        &policy.bonus,
        &tables[0].1,
    ));
    let _ = writeln!(
        html,
        r#"<p class="note">Under {} Perps held, Bloodline earns FCFS mints instead.</p></section>"#,
        policy.gtd_min_held
    );

    // ── FCFS ─────────────────────────────────────────────────────────────────────────────────
    html.push_str(
        r#"<section class="track-section"><h2><span class="chip clout"></span>FCFS — early access</h2>
<p>FCFS mints come from <b>Clout</b>: what you've collected and done — the sets, the rare finds, the factions, the market. It's the part you can still chase before the snapshot. Holders with fewer than 5 Perps also get FCFS access from what they hold.</p>
"#,
    );
    html.push_str(&points_table(
        "Clout",
        "What you did with them. The collection, the factions, the market, the Coin hunt.",
        "clout",
        "FCFS",
        &policy.clout,
        &tables[1].1,
    ));
    html.push_str("</section>\n");

    // ── Wallets ──────────────────────────────────────────────────────────────────────────────
    html.push_str(
        r#"<section><h2>Your wallets</h2>
<div class="card"><p><b>Link all your Cardano wallets to one Robinhood wallet.</b> Each wallet earns on its own and they add up, so linking never costs you anything — and wallets with fewer than 5 Perps can combine to reach GTD. Your mints go to that Robinhood wallet.</p></div>
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

fn points_table(
    name: &str,
    gist: &str,
    class: &str,
    earns: &str,
    rungs: &holder_behaviour::Rungs,
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
    let max = rows.iter().map(|row| row.points).max().unwrap_or(1).max(1);
    let _ = write!(
        html,
        r#"<div class="card"><h3>{name} points</h3><p class="gist">{gist}</p>
<ol class="steps {class}">{}</ol>
<table class="points"><thead><tr><th>Achievement</th><th class="num">Points</th><th class="num">Holders</th></tr></thead><tbody>
"#,
        steps.join("")
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
        og_gtd: 2_000,
        community_gtd: 2_000,
        fcfs: 2_000,
        og_fcfs: 1_000,
        price: "$8".to_string(),
        snapshot: "30 November 2026".to_string(),
        mature_by: "22 November 2026".to_string(),
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
            "--og-gtd" => parsed.og_gtd = number(value("--og-gtd")?, "--og-gtd")?,
            "--community-gtd" => {
                parsed.community_gtd = number(value("--community-gtd")?, "--community-gtd")?;
            }
            "--fcfs" => parsed.fcfs = number(value("--fcfs")?, "--fcfs")?,
            "--og-fcfs" => parsed.og_fcfs = number(value("--og-fcfs")?, "--og-fcfs")?,
            "--price" => parsed.price = value("--price")?,
            "--snapshot" => parsed.snapshot = value("--snapshot")?,
            "--mature-by" => parsed.mature_by = value("--mature-by")?,
            "--gtd-claimed" => {
                parsed.gtd_claimed = Some(number(value("--gtd-claimed")?, "--gtd-claimed")?);
            }
            "--fcfs-claimed" => {
                parsed.fcfs_claimed = Some(number(value("--fcfs-claimed")?, "--fcfs-claimed")?);
            }
            "-h" | "--help" => {
                println!(
                    "premint-info --catalogue <toml> [--bundle <bundle>] --out <html> \
                     [--gtd-claimed N] [--fcfs-claimed N] [--snapshot TEXT] [--mature-by TEXT]"
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
