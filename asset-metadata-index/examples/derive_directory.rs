//! Derive the directory from published segments.
//!
//! `directory/{shard}.bin` maps a policy to the chunks holding its records —
//! and a v2 directory adds a name filter per candidate. Both are derivable
//! from the segments alone, which is what lets v2 land without rebuilding the
//! corpus: run this over the published `segments/`, reconcile the v1 output
//! against the published directory **first** (the reconciliation is the
//! validation — byte for byte, every shard), then emit v2 from the same pass.
//!
//! ```text
//! cargo run -p asset-metadata-index --example derive_directory -- \
//!     <segments-dir> <out-dir> <v1|v2> [--compare <published-directory-dir>]
//! ```
//!
//! `--compare` requires `v1` — a v2 directory is not expected to match the
//! published bytes. For the full corpus this holds every record name in
//! memory (~19 M names, ~1 GB) and reads ~9.5 GiB of segments; run it on the
//! box, where both are local.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use asset_metadata_index::{
    DIRECTORY_V1, DIRECTORY_V2, DirectoryCandidate, DirectoryEntry, DirectoryShard, FilterBuilder,
    NETWORK_MAINNET, POLICY_ID_LEN, SegmentView, might_contain, records,
};

type Tree = BTreeMap<[u8; POLICY_ID_LEN], BTreeMap<u16, Vec<Vec<u8>>>>;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(segments) = args.next().map(PathBuf::from) else {
        return usage();
    };
    let Some(out) = args.next().map(PathBuf::from) else {
        return usage();
    };
    let Some(format) = args.next() else {
        return usage();
    };
    let version = match format.as_str() {
        "v1" => DIRECTORY_V1,
        "v2" => DIRECTORY_V2,
        _ => return usage(),
    };
    let mut compare = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--compare" => match args.next() {
                Some(dir) => compare = Some(PathBuf::from(dir)),
                None => return usage(),
            },
            _ => return usage(),
        }
    }
    if compare.is_some() && version != DIRECTORY_V1 {
        eprintln!("--compare reconciles against the published v1 directory — derive v1 to do it");
        return ExitCode::FAILURE;
    }

    match run(&segments, &out, version, compare.as_deref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: derive_directory <segments-dir> <out-dir> <v1|v2> [--compare <published-directory-dir>]"
    );
    ExitCode::FAILURE
}

fn run(
    segments: &Path,
    out: &Path,
    version: u8,
    compare: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(segments)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "bin"))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err(format!("no segment objects under {}", segments.display()).into());
    }

    let mut tree: Tree = BTreeMap::new();
    let mut segment_bytes = 0u64;
    let mut through_chunk = 0u16;
    for path in &paths {
        let bytes = fs::read(path)?;
        segment_bytes += bytes.len() as u64;
        let view = SegmentView::parse(&bytes)?;
        through_chunk = through_chunk.max(view.header.chunk);
        for entry in view.entries()? {
            let slice = view
                .slice(&entry.policy_id)?
                .ok_or_else(|| format!("{}: index entry with no slice", path.display()))?;
            let pair_names = tree
                .entry(entry.policy_id)
                .or_default()
                .entry(view.header.chunk)
                .or_default();
            for record in records(slice) {
                pair_names.push(record?.name);
            }
        }
    }

    let policies = tree.len();
    let candidates: usize = tree.values().map(|chunks| chunks.len()).sum();
    let names: usize = tree
        .values()
        .flat_map(|chunks| chunks.values())
        .map(Vec::len)
        .sum();
    println!(
        "segments   {} files · {} · chunks 1068..={through_chunk}",
        paths.len(),
        bytes(segment_bytes)
    );
    println!("policies   {policies} · candidates {candidates} · record names {names}");

    let mut shards: BTreeMap<u8, Vec<u8>> = BTreeMap::new();
    let mut filter_bytes = 0usize;
    let mut checked = 0usize;
    let mut false_negatives = 0usize;
    for shard in 0u8..=255 {
        let entries: Vec<DirectoryEntry> = tree
            .iter()
            .filter(|(policy_id, _)| policy_id[0] == shard)
            .map(|(policy_id, chunks)| DirectoryEntry {
                policy_id: *policy_id,
                candidates: chunks
                    .iter()
                    .map(|(chunk, names)| match version {
                        DIRECTORY_V2 => {
                            let mut builder = FilterBuilder::new(names.len());
                            for name in names {
                                builder.insert(name);
                            }
                            let filter = builder.finish();
                            for name in names {
                                checked += 1;
                                if !might_contain(&filter, name) {
                                    false_negatives += 1;
                                }
                            }
                            filter_bytes += filter.len();
                            DirectoryCandidate {
                                chunk: *chunk,
                                filter,
                            }
                        }
                        _ => DirectoryCandidate {
                            chunk: *chunk,
                            filter: Vec::new(),
                        },
                    })
                    .collect(),
            })
            .collect();
        if entries.is_empty() {
            continue;
        }
        let bytes = DirectoryShard {
            version,
            network: NETWORK_MAINNET,
            shard,
            directory_through_chunk: through_chunk,
            entries,
        }
        .encode()?;
        shards.insert(shard, bytes);
    }

    if let Some(published) = compare {
        let mut mismatches = 0usize;
        for shard in 0u8..=255u8 {
            let published = fs::read(published.join(format!("{shard:02x}.bin"))).ok();
            match (shards.get(&shard), published) {
                (None, None) => {}
                (Some(derived), Some(published)) => {
                    if *derived != published {
                        mismatches += 1;
                        let at = derived.iter().zip(&published).position(|(a, b)| a != b);
                        println!(
                            "shard {shard:02x}: DERIVED {} B vs published {} B, first differing byte {at:?}",
                            derived.len(),
                            published.len()
                        );
                    }
                }
                (Some(derived), None) => {
                    mismatches += 1;
                    println!(
                        "shard {shard:02x}: derived {} B, not published",
                        derived.len()
                    );
                }
                (None, Some(published)) => {
                    mismatches += 1;
                    println!(
                        "shard {shard:02x}: published {} B, derivation holds no entries",
                        published.len()
                    );
                }
            }
        }
        if mismatches > 0 {
            return Err(format!(
                "{mismatches} shard(s) differ from the published directory — do not emit v2 from this derivation"
            )
            .into());
        }
        println!("compare    every shard byte-identical to the published v1 directory");
    }

    if version == DIRECTORY_V2 {
        println!(
            "filters    {} bytes total · no false negatives: {}/{}",
            filter_bytes,
            checked - false_negatives,
            checked
        );
        if false_negatives > 0 {
            return Err(
                format!("{false_negatives} inserted name(s) failed their own filter").into(),
            );
        }
    }

    fs::create_dir_all(out)?;
    let mut total = 0usize;
    let mut max = (0u8, 0usize);
    for (shard, bytes) in &shards {
        fs::write(out.join(format!("{shard:02x}.bin")), bytes)?;
        total += bytes.len();
        if bytes.len() > max.1 {
            max = (*shard, bytes.len());
        }
    }
    println!(
        "wrote      {} shards · {} · max {} (shard {:02x}) → {}",
        shards.len(),
        bytes(total as u64),
        bytes(max.1 as u64),
        max.0,
        out.display()
    );
    Ok(())
}

fn bytes(count: u64) -> String {
    const KIB: u64 = 1 << 10;
    const MIB: u64 = 1 << 20;
    const GIB: u64 = 1 << 30;
    match count {
        n if n >= GIB => format!("{:.1} GiB", n as f64 / GIB as f64),
        n if n >= MIB => format!("{:.1} MiB", n as f64 / MIB as f64),
        n if n >= KIB => format!("{:.1} KiB", n as f64 / KIB as f64),
        n => format!("{n} B"),
    }
}
