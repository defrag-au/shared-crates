//! A review page for a catalogue's two points tables: every achievement in its table, by locus,
//! with its weight, who has earned it, and how many current holders each threshold reaches.
//!
//! ```text
//! cargo run --release -p holder-behaviour --example points-review -- \
//!     --catalogue ../cnft.dev-workers/config/achievements/perps.toml \
//!     --bundle .tmp/perps-tracks-now.bundle \
//!     --out .tmp/points-review.html
//! ```
//!
//! The bundle is optional: without it the page is the catalogue alone, with no holder counts.
//! Everything is read through the same loader and scoring rule the simulation and the worker use,
//! so a number on this page is the rule's, not a copy of it.

mod support;

use std::fmt::Write as _;
use std::process::ExitCode;

use holder_behaviour::{Allocation, Bundle, Rungs, Track, catalogue_digest, held_at, score_tracks};
use support::catalogue::LoadedCatalogue;

/// The loci in the order the page reads them, with the names the page uses.
const LOCI: [(&str, &str, &str); 5] = [
    ("provenance", "Origin", "the mint"),
    ("conviction", "Resolve", "time held"),
    ("curation", "Lineage", "the collection and the factions"),
    ("market", "Hustle", "the market"),
    ("stake", "Coin", "PERP COIN"),
];

struct Args {
    catalogue: String,
    bundle: Option<String>,
    out: String,
    /// Count every subject the bundle has, held now or not: what a threshold could reach if every
    /// past holder came back — the bound a ceiling is read against.
    everyone: bool,
}

/// One achievement as the page shows it.
struct Row {
    id: String,
    title: String,
    statement: String,
    axis: String,
    role: String,
    points: u32,
    /// `(holders_with, population)`; population 0 is unmeasured.
    rarity: Option<(u32, u32)>,
    grant: Option<holder_behaviour::Grant>,
}

/// Current holders reaching each rung of one table.
struct Reach {
    rungs: Vec<(u32, u32, u32)>, // (at_least, mints, holders reaching)
    holders: u32,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("points-review: {error}");
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
            let digest = catalogue_digest(catalogue.ids());
            if bundle.catalogue_digest() != digest {
                return Err(format!(
                    "the bundle was built from catalogue {:016x}, but {} digests to {digest:016x}",
                    bundle.catalogue_digest(),
                    args.catalogue
                ));
            }
            Some(bundle)
        }
        None => None,
    };

    let mut tables: [(Track, Vec<Row>); 2] =
        [(Track::Bloodline, Vec::new()), (Track::Clout, Vec::new())];
    for index in 0..catalogue.len() {
        let row = Row {
            id: catalogue.id_at(index).unwrap_or_default().to_string(),
            title: catalogue.title_at(index).unwrap_or_default().to_string(),
            statement: catalogue
                .statement_at(index)
                .unwrap_or_default()
                .to_string(),
            axis: catalogue.axis_at(index).unwrap_or_default().to_string(),
            role: catalogue.role_at(index).unwrap_or_default().to_string(),
            points: catalogue.points_at(index).unwrap_or(0),
            rarity: bundle.as_ref().and_then(|bundle| {
                u16::try_from(index)
                    .ok()
                    .and_then(|index| bundle.rarity(index))
                    .map(|rarity| (rarity.holders_with, rarity.population))
            }),
            grant: catalogue.grant_at(index),
        };
        let track = catalogue.track_at(index);
        if let Some((_, rows)) = tables.iter_mut().find(|(t, _)| *t == track) {
            rows.push(row);
        }
    }

    let reach = match &bundle {
        Some(bundle) => Some(reach(bundle, &catalogue, policy, args.everyone)?),
        None => None,
    };

    let html = render(
        &args,
        &catalogue,
        policy,
        &tables,
        reach.as_ref(),
        bundle.as_ref(),
    );
    std::fs::write(&args.out, html).map_err(|error| format!("write {}: {error}", args.out))?;
    eprintln!("points-review: wrote {}", args.out);
    Ok(())
}

/// Every subject holding a Perp at the bundle's cursor (or every subject, with `everyone`), scored
/// in both tables, against each rung. The counts go to stderr too, for a reader without a browser.
fn reach(
    bundle: &Bundle<'_>,
    catalogue: &LoadedCatalogue,
    policy: Allocation,
    everyone: bool,
) -> Result<(Reach, Reach), String> {
    let cursor = bundle.holder_cursor();
    let mut scores: Vec<(u32, u32)> = Vec::new();
    for subject in bundle.subjects() {
        let subject = subject.map_err(|error| format!("read subject: {error}"))?;
        let lots: Vec<holder_behaviour::Lot> = subject.lots().collect();
        if !everyone && held_at(&lots, cursor) == 0 {
            continue;
        }
        let pieces = holder_behaviour::pieces_now(
            subject
                .earned()
                .map(|earned| (earned.achievement, earned.standing, earned.held)),
        );
        let scored = score_tracks(
            bundle,
            &subject.key_str(),
            subject
                .earned()
                .map(|earned| (earned.achievement, earned.standing)),
            |index| catalogue.role_at(usize::from(index)),
            catalogue.priced(&pieces),
            |index| catalogue.track_at(usize::from(index)),
            policy,
        )
        .map_err(|error| error.to_string())?;
        scores.push((scored.bloodline.points, scored.clout.points));
    }
    let holders = scores.len() as u32;
    let table = |rungs: &Rungs, pick: fn(&(u32, u32)) -> u32| Reach {
        rungs: rungs
            .as_slice()
            .iter()
            .map(|rung| {
                let n = scores.iter().filter(|s| pick(s) >= rung.at_least).count() as u32;
                (rung.at_least, rung.mints, n)
            })
            .collect(),
        holders,
    };
    let (bloodline, clout) = (table(&policy.bonus, |s| s.0), table(&policy.clout, |s| s.1));
    for (name, reach) in [("bloodline", &bloodline), ("clout", &clout)] {
        let rungs: Vec<String> = reach
            .rungs
            .iter()
            .map(|(at, mints, n)| format!("{at}:{n}×{mints}"))
            .collect();
        let slots: u32 = reach.rungs.iter().map(|(_, mints, n)| mints * n).sum();
        eprintln!(
            "points-review: {name} over {holders} subjects · {} · {slots} slots",
            rungs.join(" ")
        );
    }
    Ok((bloodline, clout))
}

fn render(
    args: &Args,
    catalogue: &LoadedCatalogue,
    policy: Allocation,
    tables: &[(Track, Vec<Row>); 2],
    reach: Option<&(Reach, Reach)>,
    bundle: Option<&Bundle<'_>>,
) -> String {
    let mut html = String::new();
    let digest = catalogue_digest(catalogue.ids());
    let source = match bundle {
        Some(bundle) => format!(
            "bundle built {} · {} subjects · catalogue {digest:016x}",
            date(bundle.built_at()),
            bundle.subject_count()
        ),
        None => format!("catalogue {digest:016x} · no bundle, so no holder counts"),
    };
    let bloodline_earns = match policy.bonus_fcfs {
        true => "first come",
        false => "the guarantee",
    };
    let lede = match policy.bonus_fcfs {
        true => {
            "<span class=\"chip bloodline\"></span><b>Bloodline</b> — the mint and time held. <span class=\"chip clout\"></span><b>Clout</b> — the collection, the factions, the market. Both climb <b>first come</b>; the guarantee is what is held."
        }
        false => {
            "<span class=\"chip bloodline\"></span><b>Bloodline</b> — the mint and time held; climbs <b>the guarantee</b>. <span class=\"chip clout\"></span><b>Clout</b> — the collection, the factions, the market; climbs <b>first come</b>."
        }
    };
    let _ = write!(
        html,
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Points tables — review</title>
<style>{STYLE}</style></head>
<body><main class="viz-root">
<header>
  <h1>Points tables</h1>
  <p class="sub">{catalogue} · {source}</p>
  <p class="lede">{lede}</p>
</header>
"#,
        catalogue = esc(&args.catalogue),
    );

    for (position, (track, rows)) in tables.iter().enumerate() {
        let (name, earns) = match track {
            Track::Bloodline => ("Bloodline", bloodline_earns),
            Track::Clout => ("Clout", "first come"),
        };
        let (rungs, tail) = policy.track_ladder(*track);
        let class = track.as_str();
        let total: u32 = rows.iter().map(|row| row.points).sum();
        let max = rows.iter().map(|row| row.points).max().unwrap_or(1).max(1);
        let _ = write!(
            html,
            r#"<section class="table {class}">
<h2><span class="chip {class}"></span>{name} <span class="count">{n} achievements · {total} points in all</span></h2>
"#,
            n = rows.len(),
        );

        // The thresholds: what the points are worth, and how many holders each reaches.
        let reached = reach.map(|both| if position == 0 { &both.0 } else { &both.1 });
        let _ = write!(
            html,
            r#"<div class="rungs"><h3>{name} points → {earns}</h3><ol>"#
        );
        for (i, rung) in rungs.as_slice().iter().enumerate() {
            let (count, of) = reached
                .and_then(|r| r.rungs.get(i).map(|(_, _, n)| (*n, r.holders)))
                .unwrap_or((0, 0));
            let width = if of > 0 {
                count as f64 / of as f64 * 100.0
            } else {
                0.0
            };
            let tip = format!(
                "{count} of {of} current holders reach {} {name} points",
                thousands(rung.at_least)
            );
            let _ = write!(
                html,
                r#"<li data-tip="{tip}"><span class="at">{at}</span><span class="adds">+{mints} {earns}</span><span class="track"><span class="fill {class}" style="width:{width:.1}%"></span></span><span class="n">{count}</span></li>"#,
                tip = esc(&tip),
                at = thousands(rung.at_least),
                mints = rung.mints,
            );
        }
        let of = reached.map_or(0, |r| r.holders);
        let under = match track {
            Track::Bloodline if policy.bonus_fcfs => {
                " Bloodline's slots are first come for every holder; the guarantee is what is held."
                    .to_string()
            }
            Track::Bloodline => format!(
                " Under {} Perps held, bloodline's slots are first come instead.",
                policy.gtd_min_held
            ),
            Track::Clout => String::new(),
        };
        let tail_note = match tail {
            Some(tail) => format!(
                " One more {earns} for every {} points beyond the last step.",
                thousands(tail.every)
            ),
            None => String::new(),
        };
        let population = match reached {
            Some(_) if args.everyone => {
                format!(
                    "of the {of} subjects read.{under} A wallet, not an identity: linked wallets only add.{tail_note}"
                )
            }
            Some(_) => format!(
                "of the {of} holding a Perp at the snapshot.{under} A wallet, not an identity: linked wallets only add.{tail_note}"
            ),
            None => format!("no bundle, so no holder counts.{under}{tail_note}"),
        };
        let _ = write!(
            html,
            r#"</ol><p class="note">Wallets reaching each threshold, {population}</p></div>
<table><thead><tr><th>Achievement</th><th>Earned by</th><th>Role</th><th class="num">Points</th><th class="num">Earned by (of all subjects)</th><th>Note</th></tr></thead><tbody>
"#
        );

        for (axis, locus, gist) in LOCI {
            let mut group: Vec<&Row> = rows.iter().filter(|row| row.axis == axis).collect();
            if group.is_empty() {
                continue;
            }
            group.sort_by(|a, b| b.points.cmp(&a.points).then(a.id.cmp(&b.id)));
            let sum: u32 = group.iter().map(|row| row.points).sum();
            let _ = writeln!(
                html,
                r#"<tr class="locus"><th colspan="6">{locus} <span>— {gist} · {n} · {sum} points</span></th></tr>"#,
                n = group.len(),
                sum = thousands(sum),
            );
            for row in group {
                let width = row.points as f64 / max as f64 * 100.0;
                let (holders, share, unread) = match row.rarity {
                    Some((_, 0)) | None => ("—".to_string(), String::new(), row.rarity.is_some()),
                    Some((with, population)) => (
                        with.to_string(),
                        format!("{:.1}%", with as f64 / population as f64 * 100.0),
                        false,
                    ),
                };
                let mut notes: Vec<String> = Vec::new();
                if let Some(grant) = row.grant {
                    let each = if grant.per_piece { " per piece" } else { "" };
                    if grant.gtd > 0 {
                        notes.push(format!("+{} GTD{each}", grant.gtd));
                    }
                    if grant.fcfs > 0 {
                        notes.push(format!("+{} FCFS{each}", grant.fcfs));
                    }
                }
                if unread {
                    notes.push("not read yet".to_string());
                }
                let tip = format!(
                    "{} · {} points ({}) · {}",
                    row.title,
                    thousands(row.points),
                    name,
                    if share.is_empty() {
                        "no holder count".to_string()
                    } else {
                        format!("{holders} holders, {share} of subjects")
                    }
                );
                let _ = writeln!(
                    html,
                    r#"<tr data-tip="{tip}"><td><b>{title}</b><code>{id}</code></td><td class="stmt">{stmt}</td><td class="role">{role}</td><td class="num"><div class="pts"><span class="bar"><span class="fill {class}" style="width:{width:.1}%"></span></span><span>{points}</span></div></td><td class="num">{holders}<small>{share}</small></td><td class="notes">{notes}</td></tr>"#,
                    tip = esc(&tip),
                    title = esc(&row.title),
                    id = esc(&row.id),
                    stmt = esc(&row.statement),
                    role = esc(&row.role),
                    points = thousands(row.points),
                    notes = esc(&notes.join(" · ")),
                );
            }
        }
        html.push_str("</tbody></table></section>\n");
    }
    html.push_str("</main></body></html>\n");
    html
}

const STYLE: &str = r#"
.viz-root{color-scheme:light;--surface-1:#fcfcfb;--surface-2:#f2f1ee;--rule:#e2e0db;--text-primary:#0b0b0b;--text-secondary:#52514e;--text-muted:#7a7873;--bloodline:#2a78d6;--clout:#eb6834;--track:#ebe9e4}
@media (prefers-color-scheme:dark){:root:where(:not([data-theme="light"])) .viz-root{color-scheme:dark;--surface-1:#1a1a19;--surface-2:#232321;--rule:#34342f;--text-primary:#f0efec;--text-secondary:#c3c2b7;--text-muted:#8f8e86;--bloodline:#3987e5;--clout:#d95926;--track:#2c2c29}}
:root[data-theme="dark"] .viz-root{color-scheme:dark;--surface-1:#1a1a19;--surface-2:#232321;--rule:#34342f;--text-primary:#f0efec;--text-secondary:#c3c2b7;--text-muted:#8f8e86;--bloodline:#3987e5;--clout:#d95926;--track:#2c2c29}
html,body{margin:0;background:var(--surface-1)}
.viz-root{font:14px/1.45 ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif;color:var(--text-primary);background:var(--surface-1);max-width:1180px;margin:0 auto;padding:28px 24px 48px}
h1{font-size:24px;margin:0 0 4px}.sub{color:var(--text-muted);margin:0 0 10px;font-size:12px}
.lede{color:var(--text-secondary);margin:0 0 24px}.lede b{color:var(--text-primary)}
.chip{display:inline-block;width:10px;height:10px;border-radius:3px;margin:0 6px 0 2px;vertical-align:0}
.chip.bloodline,.fill.bloodline{background:var(--bloodline)}.chip.clout,.fill.clout{background:var(--clout)}
section.table{margin:0 0 40px}
h2{font-size:19px;margin:0 0 12px;display:flex;align-items:center;gap:2px}
h2 .count{font-size:13px;font-weight:400;color:var(--text-muted);margin-left:10px}
.rungs{background:var(--surface-2);border-radius:10px;padding:12px 16px;margin:0 0 14px}
.rungs h3{font-size:13px;margin:0 0 8px;color:var(--text-secondary);font-weight:600}
.rungs ol{list-style:none;margin:0;padding:0;display:grid;gap:6px}
.rungs li{display:grid;grid-template-columns:70px 120px 1fr 48px;align-items:center;gap:10px;position:relative}
.rungs .at{font-variant-numeric:tabular-nums;font-weight:600}.rungs .adds{color:var(--text-secondary)}
.rungs .n{text-align:right;font-variant-numeric:tabular-nums;color:var(--text-secondary)}
.track,.bar{display:block;height:8px;background:var(--track);border-radius:4px;overflow:hidden}
.fill{display:block;height:100%;border-radius:0 4px 4px 0}
.note{margin:8px 0 0;font-size:12px;color:var(--text-muted)}
table{width:100%;border-collapse:collapse}
thead th{text-align:left;font-size:12px;font-weight:600;color:var(--text-muted);border-bottom:1px solid var(--rule);padding:6px 8px}
th.num,td.num{text-align:right}
tr.locus th{text-align:left;padding:16px 8px 6px;font-size:14px;border-bottom:1px solid var(--rule)}
tr.locus th span{font-weight:400;color:var(--text-muted);font-size:12px}
tbody td{padding:7px 8px;border-bottom:1px solid var(--rule);vertical-align:middle}
tbody tr[data-tip]{position:relative}tbody tr[data-tip]:hover{background:var(--surface-2)}
td b{display:block}td code{font-size:11px;color:var(--text-muted)}
td.stmt{color:var(--text-secondary);max-width:380px}td.role{color:var(--text-muted);font-size:12px}
.pts{display:grid;grid-template-columns:110px 52px;align-items:center;gap:8px;justify-content:end;font-variant-numeric:tabular-nums;font-weight:600}
td.num small{display:block;font-size:11px;color:var(--text-muted);font-weight:400}
td.notes{font-size:12px;color:var(--text-secondary)}
[data-tip]:hover::after{content:attr(data-tip);position:absolute;left:12px;top:100%;z-index:5;background:var(--text-primary);color:var(--surface-1);font-size:12px;padding:5px 8px;border-radius:6px;white-space:nowrap;pointer-events:none}
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

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let (mut catalogue, mut bundle, mut out, mut everyone) = (None, None, None, false);
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().ok_or_else(|| format!("{flag} needs a value"));
        match arg.as_str() {
            "--catalogue" => catalogue = Some(value("--catalogue")?),
            "--bundle" => bundle = Some(value("--bundle")?),
            "--out" => out = Some(value("--out")?),
            "--everyone" => everyone = true,
            "-h" | "--help" => {
                println!(
                    "points-review --catalogue <toml> [--bundle <bundle>] --out <html>\n\
                     A review page for the catalogue's bloodline and clout tables."
                );
                std::process::exit(0);
            }
            other => return Err(format!("unexpected argument {other}")),
        }
    }
    Ok(Args {
        catalogue: catalogue.ok_or("--catalogue is required")?,
        bundle,
        out: out.ok_or("--out is required")?,
        everyone,
    })
}
