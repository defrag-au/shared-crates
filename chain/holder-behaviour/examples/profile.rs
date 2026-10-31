//! Opens a published bundle and prints one profile.
//!
//! This is the read path a game or a page uses: fetch the file once, open it with no
//! allocation for the index, find one account, and derive progress from the stored lots.
//! It exists so the published artefact can be checked without the stream that built it.
//!
//! ```sh
//! cargo run -p holder-behaviour --example profile -- \
//!     .tmp/perps.bundle <credential-hex> \
//!     --catalogue ~/code/defrag/cnft.dev-workers/config/achievements/perps.toml
//! ```

mod support;

use std::process::ExitCode;

use chains::ChainRef;
use holder_behaviour::{
    Assessment, BUNDLE_COVERAGE, Bundle, BundleStore, EarnedSince, Progress, Slot, evaluate_row,
};
use support::catalogue::LoadedCatalogue;

struct Args {
    path: String,
    key: String,
    catalogue: Option<String>,
    now: Option<Slot>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let mut path = None;
    let mut key = None;
    let mut catalogue = None;
    let mut now = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalogue" => {
                catalogue = Some(args.next().ok_or("--catalogue needs a path".to_string())?)
            }
            "--now" => {
                let value = args.next().ok_or("--now needs a slot".to_string())?;
                now = Some(
                    value
                        .parse()
                        .map_err(|_| "--now takes a slot".to_string())?,
                )
            }
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other if path.is_none() => path = Some(other.to_string()),
            other if key.is_none() => key = Some(other.to_string()),
            other => return Err(format!("unexpected argument {other}")),
        }
    }
    Ok(Args {
        path: path.ok_or("give the path to a bundle")?,
        key: key.ok_or("give the account key to look up")?,
        catalogue,
        now,
    })
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("profile: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let bytes =
        std::fs::read(&args.path).map_err(|error| format!("read {}: {error}", args.path))?;
    let bundle = Bundle::open(&bytes).map_err(|error| format!("open {}: {error}", args.path))?;

    println!(
        "bundle      {} · {} bytes · v{}",
        args.path,
        bytes.len(),
        holder_behaviour::BUNDLE_VERSION
    );
    println!(
        "built-at    {} · holder cursor {} · market cursor {}",
        bundle.built_at(),
        bundle.holder_cursor(),
        bundle.market_cursor()
    );
    println!(
        "catalogue   {:016x} · {} achievements · {} subjects",
        bundle.catalogue_digest(),
        bundle.achievement_count(),
        bundle.subject_count()
    );

    let Some(view) = bundle
        .find(&args.key)
        .map_err(|error| format!("find {}: {error}", args.key))?
    else {
        return Err(format!(
            "{}: not in this bundle — keys are credential hex, not stake1…",
            args.key
        ));
    };

    println!();
    println!("account     {}", view.key_str());
    println!("earned      {}", view.earned().count());
    let lots: Vec<_> = view.lots().collect();
    let open = lots.iter().filter(|lot| lot.disposed_at.is_none()).count();
    println!("lots        {} · {open} open", lots.len());

    let Some(path) = &args.catalogue else {
        for entry in view.earned() {
            println!(
                "            #{:<3} at {}",
                entry.achievement, entry.valid_from
            );
        }
        return Ok(());
    };

    let catalogue = LoadedCatalogue::load(path)?;
    let definitions = catalogue.definitions();
    let store = BundleStore::new(view, ChainRef::cardano_mainnet());
    let at = Assessment {
        now: args.now.unwrap_or(bundle.holder_cursor()),
        holder_coverage: BUNDLE_COVERAGE,
    };
    let outcomes =
        evaluate_row(&store, &at, &definitions).map_err(|error| format!("evaluate: {error}"))?;

    println!();
    for outcome in &outcomes {
        let state = match outcome.progress {
            Progress::Earned { since } => match since {
                EarnedSince::Exact(slot) => format!("earned at {slot}"),
                EarnedSince::AsAt(slot) => format!("earned as at {slot}"),
            },
            Progress::Pending {
                have,
                need,
                eta_slot,
            } => match eta_slot {
                Some(eta) => format!("pending {have}/{need} · eta {eta}"),
                None => format!("pending {have}/{need}"),
            },
            Progress::Missed => continue,
            Progress::Unavailable(gap) => format!("unavailable: {gap:?}"),
        };
        println!("            {:<16} {state}", outcome.id);
    }

    Ok(())
}
