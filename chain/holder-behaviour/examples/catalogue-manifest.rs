//! Writes the manifest a bundle consumer needs beside the bundle.
//!
//! A bundle records a rarity row per achievement **index**, and never the id or the role: it is
//! a projection of a catalogue, not a copy of it. That is enough for the tool that built it, and
//! not enough for anything else — a site answering one holder cannot tell a core achievement
//! from a flavour one, so it cannot score anyone.
//!
//! So this writes the missing half as JSON: the catalogue's order, each entry's role, and the
//! digest the bundle must carry. A consumer reads the manifest, then the bundle, and refuses
//! unless the two digests agree — which is the whole point of the bundle storing one.
//!
//! ```sh
//! cargo run --release -p holder-behaviour --example catalogue-manifest -- \
//!     --catalogue ~/code/defrag/cnft.dev-workers/config/achievements/perps.toml \
//!     --campaign perplings-premint \
//!     --bundle .tmp/perps-status.bundle \
//!     --out .tmp/catalogue-manifest.json
//! ```
//!
//! `--bundle` is optional but worth passing: given one, this checks the digest and the count
//! before writing, so a manifest can never be published against a bundle it does not describe.

mod support;

use std::process::ExitCode;

use holder_behaviour::{Allocation, Bundle, catalogue_digest};
use serde::Serialize;
use support::catalogue::LoadedCatalogue;

#[derive(Serialize)]
struct Manifest<'a> {
    campaign: &'a str,
    /// Where the consumer should read the bundle, relative to the bucket's root.
    bundle: &'a str,
    /// The bundle's FNV-1a change detector, as hex: a `u64` does not survive JSON's `f64`.
    catalogue_digest: String,
    /// When the bundle this was checked against was built, in unix seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    bundle_built_at: Option<u64>,
    /// The thresholds the consumer must score with, so it cannot be configured with its own.
    allocation: Allocation,
    achievements: Vec<Entry<'a>>,
}

#[derive(Serialize)]
struct Entry<'a> {
    index: u32,
    id: &'a str,
    role: &'a str,
}

struct Args {
    catalogue: String,
    campaign: String,
    bundle: Option<String>,
    /// The bucket root both keys sit under — the same value the worker's `BUNDLE_PREFIX` carries.
    prefix: String,
    /// Overrides where the bundle is expected, independently of `prefix`.
    key: Option<String>,
    out: Option<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("catalogue-manifest: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let catalogue = LoadedCatalogue::load(&args.catalogue)?;
    let allocation = catalogue
        .plan()
        .ok_or_else(|| {
            format!(
                "{} has no [allocation] block, so there is nothing to estimate",
                args.catalogue
            )
        })?
        .allocation;

    let digest = catalogue_digest(catalogue.ids());
    let manifest_key = format!("{}/{}/current.json", args.prefix, args.campaign);
    let manifest_key = manifest_key.replace("//", "/");
    let bundle_key = args
        .key
        .unwrap_or_else(|| format!("{}/{}/current.bundle", args.prefix, args.campaign));

    let mut built_at = None;
    if let Some(path) = &args.bundle {
        let bytes = std::fs::read(path).map_err(|error| format!("read {path}: {error}"))?;
        let bundle = Bundle::open(&bytes).map_err(|error| format!("open {path}: {error}"))?;
        if bundle.catalogue_digest() != digest {
            return Err(format!(
                "{path} carries catalogue digest {:016x}, but {} digests to {digest:016x} — \
                 one of them is the wrong file",
                bundle.catalogue_digest(),
                args.catalogue
            ));
        }
        if usize::from(bundle.achievement_count()) != catalogue.len() {
            return Err(format!(
                "{path} scores {} achievements, but {} defines {}",
                bundle.achievement_count(),
                args.catalogue,
                catalogue.len()
            ));
        }
        built_at = Some(bundle.built_at());
    }

    let achievements: Vec<Entry<'_>> = (0..catalogue.len())
        .map(|index| Entry {
            index: index as u32,
            id: catalogue.id_at(index).unwrap_or_default(),
            role: catalogue.role_at(index).unwrap_or_default(),
        })
        .collect();

    let manifest = Manifest {
        campaign: &args.campaign,
        bundle: &bundle_key,
        catalogue_digest: format!("{digest:016x}"),
        bundle_built_at: built_at,
        allocation,
        achievements,
    };

    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|error| format!("encode manifest: {error}"))?;
    match &args.out {
        Some(path) => std::fs::write(path, format!("{json}\n"))
            .map_err(|error| format!("write {path}: {error}"))?,
        None => println!("{json}"),
    }

    eprintln!(
        "catalogue-manifest: {} achievements · digest {:016x}\n  put this at {manifest_key}\n  bundle at   {bundle_key}",
        catalogue.len(),
        digest,
    );
    Ok(())
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let mut parsed = Args {
        catalogue: String::new(),
        campaign: String::new(),
        bundle: None,
        prefix: "bundles".to_string(),
        key: None,
        out: None,
    };
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--catalogue" => parsed.catalogue = next(&mut args, "--catalogue")?,
            "--campaign" => parsed.campaign = next(&mut args, "--campaign")?,
            "--bundle" => parsed.bundle = Some(next(&mut args, "--bundle")?),
            "--prefix" => parsed.prefix = next(&mut args, "--prefix")?,
            "--key" => parsed.key = Some(next(&mut args, "--key")?),
            "--out" => parsed.out = Some(next(&mut args, "--out")?),
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other:?} — try --help")),
        }
    }
    if parsed.catalogue.is_empty() {
        return Err("--catalogue is required".into());
    }
    if parsed.campaign.is_empty() {
        return Err("--campaign is required".into());
    }
    Ok(parsed)
}

fn next(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} needs a value after it"))
}

fn print_help() {
    println!(
        "catalogue-manifest — write the catalogue order a bundle consumer needs

usage
  catalogue-manifest --catalogue <perps.toml> --campaign <id> [--bundle <file>]
                     [--prefix <root>] [--key <bundle key>] [--out <file>]

  --catalogue   the frozen catalogue the bundle was built against
  --campaign    the campaign id the consumer is configured with
  --bundle      a bundle to check against before writing (optional, recommended)
  --prefix      the bucket root both keys sit under; the same value the worker's
                BUNDLE_PREFIX carries. Default `bundles`
  --key         where the consumer should read the bundle; overrides the one
                `--prefix` derives
  --out         write the JSON here instead of stdout"
    );
}
