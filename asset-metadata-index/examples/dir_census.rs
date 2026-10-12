//! Census over a published directory: **how many candidate chunks does a
//! resolve fan out to?**
//!
//! Run against a build's directory — the live `directory-v2/`, or the v1-layout
//! `directory/` that `resolve_sample` still writes beside it (read by nothing):
//!
//!     cargo run -p asset-metadata-index --example dir_census -- <dir>
//!
//! The gate-2 census priced slices — records per `(chunk, policy)` — but never
//! the candidate count, chunks per policy, which is what a resolve fans out
//! to. The full build then produced one policy at **137 candidates**, over the
//! runtime's measured ~90-subrequest reliability boundary (see
//! `ASSET_METADATA_INDEX.md`, the readiness note), and the probe refuses
//! batches over 64. This counts the population so the boundary is a size, not
//! an anecdote.

use std::fs;
use std::process::ExitCode;

use asset_metadata_index::{DirectoryView, POLICY_ID_LEN};

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn main() -> ExitCode {
    let Some(dir) = std::env::args().nth(1) else {
        eprintln!("usage: dir_census <directory-dir>");
        return ExitCode::FAILURE;
    };

    let mut paths: Vec<_> = match fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|extension| extension == "bin"))
            .collect(),
        Err(error) => {
            eprintln!("{dir}: {error}");
            return ExitCode::FAILURE;
        }
    };
    paths.sort();

    let mut shards = 0usize;
    let mut failures = 0usize;
    let mut widest: Vec<(usize, [u8; POLICY_ID_LEN])> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();
    for path in &paths {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("{}: {error}", path.display());
                failures += 1;
                continue;
            }
        };
        let view = match DirectoryView::parse(&bytes) {
            Ok(view) => view,
            Err(error) => {
                eprintln!("{}: {error}", path.display());
                failures += 1;
                continue;
            }
        };
        // `entries()` is the inspect path and returns owned entries, chunks
        // included — exactly the candidate list a resolve walks.
        match view.entries() {
            Ok(entries) => {
                shards += 1;
                for entry in entries {
                    let count = entry.candidates.len();
                    widest.push((count, entry.policy_id));
                    counts.push(count);
                }
            }
            Err(error) => {
                eprintln!("{}: {error}", path.display());
                failures += 1;
            }
        }
    }

    if counts.is_empty() {
        eprintln!("no policy entries under {dir} ({failures} unreadable shard(s))");
        return ExitCode::FAILURE;
    }
    counts.sort_unstable();
    let policies = counts.len();
    let total: usize = counts.iter().sum();
    let at = |fraction: f64| {
        let index = ((policies as f64 * fraction) as usize).min(policies - 1);
        counts[index]
    };

    println!("shards {shards} · policies {policies} · candidates Σ {total}");
    println!(
        "candidates per policy: mean {:.2} · p50 {} · p90 {} · p99 {} · p99.9 {} · max {}",
        total as f64 / policies as f64,
        at(0.50),
        at(0.90),
        at(0.99),
        at(0.999),
        counts[policies - 1]
    );
    for bound in [1, 4, 8, 16, 32, 64, 128] {
        let over = counts.iter().filter(|count| **count > bound).count();
        println!("policies over {bound:>3} candidates: {over}");
    }

    widest.sort_unstable_by_key(|(count, _)| std::cmp::Reverse(*count));
    println!("widest (candidates · policy):");
    for (count, policy) in widest.iter().take(10) {
        println!("  {count:>4} · {}", hex(policy));
    }
    if failures > 0 {
        println!("({failures} shard(s) unreadable — the census covers the rest)");
    }
    ExitCode::SUCCESS
}
