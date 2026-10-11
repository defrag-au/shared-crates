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
//! cargo run -p asset-metadata-index --example derive_directory -- \
//!     delta <segments-dir> <directory-dir> [--pointer <manifest-file>]
//! ```
//!
//! `--compare` requires `v1` — a v2 directory is not expected to match the
//! published bytes. For the full corpus this holds every record name in
//! memory (~19 M names, ~1 GB) and reads ~9.5 GiB of segments; run it on the
//! box, where both are local.
//!
//! The `delta` mode is the daily sweep: it folds the segments that sealed
//! since the directory's own coverage into an existing v2 directory, in
//! place, reading **only those segments** — the corpus is never re-read.
//! Touched shards merge ([`patch_shard`]), every shard's coverage claim
//! advances, and the manifest is rewritten last.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use asset_metadata_index::{
    DIRECTORY_V1, DIRECTORY_V2, DirectoryCandidate, DirectoryEntry, DirectoryHeader,
    DirectoryShard, FilterBuilder, NETWORK_MAINNET, POLICY_ID_LEN, Pointer, SegmentView,
    might_contain, patch_shard, records,
};

type Tree = BTreeMap<[u8; POLICY_ID_LEN], BTreeMap<u16, Vec<Vec<u8>>>>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match args.first().map(String::as_str) {
        Some("delta") => run_delta(&args[1..]),
        _ => run_full(&args),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run_full(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut args = args.iter();
    let Some(segments) = args.next().map(PathBuf::from) else {
        return Err(usage().into());
    };
    let Some(out) = args.next().map(PathBuf::from) else {
        return Err(usage().into());
    };
    let Some(format) = args.next() else {
        return Err(usage().into());
    };
    let version = match format.as_str() {
        "v1" => DIRECTORY_V1,
        "v2" => DIRECTORY_V2,
        _ => return Err(usage().into()),
    };
    let mut compare = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--compare" => match args.next() {
                Some(dir) => compare = Some(PathBuf::from(dir)),
                None => return Err(usage().into()),
            },
            _ => return Err(usage().into()),
        }
    }
    if compare.is_some() && version != DIRECTORY_V1 {
        return Err(
            "--compare reconciles against the published v1 directory — derive v1 to do it".into(),
        );
    }
    run(&segments, &out, version, compare.as_deref())
}

fn usage() -> String {
    concat!(
        "usage: derive_directory <segments-dir> <out-dir> <v1|v2> [--compare <published-directory-dir>]\n",
        "       derive_directory delta <segments-dir> <directory-dir> [--pointer <manifest-file>]",
    )
    .to_string()
}

/// The daily sweep: fold segments newer than the directory's coverage into
/// the v2 directory in place, reading only those segments.
fn run_delta(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut args = args.iter();
    let Some(segments) = args.next().map(PathBuf::from) else {
        return Err(usage().into());
    };
    let Some(directory) = args.next().map(PathBuf::from) else {
        return Err(usage().into());
    };
    let mut pointer_path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pointer" => match args.next() {
                Some(path) => pointer_path = Some(PathBuf::from(path)),
                None => return Err(usage().into()),
            },
            _ => return Err(usage().into()),
        }
    }

    // Every shard, parsed and validated. The cutoff is the minimum coverage
    // claim, so a shard left behind by an interrupted sweep widens the delta
    // rather than hiding chunks from it.
    let mut shard_paths: Vec<PathBuf> = fs::read_dir(&directory)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "bin"))
        .collect();
    shard_paths.sort();
    if shard_paths.is_empty() {
        return Err(format!("no shard objects under {}", directory.display()).into());
    }

    let mut shards: BTreeMap<u8, (Vec<u8>, DirectoryHeader)> = BTreeMap::new();
    let mut cutoff = u16::MAX;
    let mut network = None;
    for path in &shard_paths {
        let bytes = fs::read(path)?;
        let header = DirectoryHeader::parse(&bytes)?;
        if header.version != DIRECTORY_V2 {
            return Err(format!(
                "{} is v{} — the delta derivation patches v2 only",
                path.display(),
                header.version
            )
            .into());
        }
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        let ordinal = u8::from_str_radix(stem, 16)
            .map_err(|_| format!("{}: shard objects are named by hex ordinal", path.display()))?;
        if ordinal != header.shard {
            return Err(
                format!("{}: header says shard {:02x}", path.display(), header.shard).into(),
            );
        }
        if let Some(network) = network {
            if network != header.network {
                return Err(format!(
                    "shard {ordinal:02x} is network {} among shards of network {network}",
                    header.network
                )
                .into());
            }
        } else {
            network = Some(header.network);
        }
        cutoff = cutoff.min(header.directory_through_chunk);
        shards.insert(ordinal, (bytes, header));
    }
    let network = network.expect("at least one shard was read");

    // The segments newer than the cutoff — the only objects this run reads.
    let mut new_segments: Vec<(u16, PathBuf)> = fs::read_dir(&segments)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "bin"))
        .map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.parse::<u16>().ok())
                .map(|chunk| (chunk, path))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| format!("{}: segment objects are named by chunk", segments.display()))?;
    new_segments.retain(|(chunk, _)| *chunk > cutoff);
    new_segments.sort();
    let frontier = new_segments.last().map(|(chunk, _)| *chunk);
    let Some(frontier) = frontier else {
        println!(
            "fresh      directory complete through {cutoff}; no segments newer under {}",
            segments.display()
        );
        return Ok(());
    };
    println!(
        "delta      directory through {cutoff} → {frontier}: {} new segment(s)",
        new_segments.len()
    );

    // The manifest, if given: its built coverage must already admit the
    // segments, and its directory claim must not be ahead of them.
    let pointer = match pointer_path {
        Some(path) => {
            let pointer = Pointer::parse_str(&fs::read_to_string(&path)?)?;
            if pointer.built_through_chunk < frontier {
                return Err(format!(
                    "{}: built_through {} lags the segments ({frontier}) — publish the segments and their pointer first",
                    path.display(),
                    pointer.built_through_chunk
                )
                .into());
            }
            if pointer.directory_through_chunk > frontier {
                return Err(format!(
                    "{}: directory_through {} is ahead of the segments ({frontier}) — refusing to move a claim backwards",
                    path.display(),
                    pointer.directory_through_chunk
                )
                .into());
            }
            Some((path, pointer))
        }
        None => None,
    };

    // Additions: each new segment's `(policy, chunk)` pairs, filters built
    // exactly as the full derivation builds them.
    let mut additions: BTreeMap<u8, BTreeMap<[u8; POLICY_ID_LEN], Vec<DirectoryCandidate>>> =
        BTreeMap::new();
    let mut records_folded = 0u64;
    for (chunk, path) in &new_segments {
        let bytes = fs::read(path)?;
        let view = SegmentView::parse(&bytes)?;
        if view.header.chunk != *chunk {
            return Err(format!(
                "{}: name says chunk {chunk}, header says {}",
                path.display(),
                view.header.chunk
            )
            .into());
        }
        records_folded += u64::from(view.header.record_count);
        for entry in view.entries()? {
            let slice = view
                .slice(&entry.policy_id)?
                .ok_or_else(|| format!("{}: index entry with no slice", path.display()))?;
            let mut names: Vec<Vec<u8>> = Vec::new();
            for record in records(slice) {
                names.push(record?.name);
            }
            let mut builder = FilterBuilder::new(names.len());
            for name in &names {
                builder.insert(name);
            }
            additions
                .entry(entry.policy_id[0])
                .or_default()
                .entry(entry.policy_id)
                .or_default()
                .push(DirectoryCandidate {
                    chunk: *chunk,
                    filter: builder.finish(),
                });
        }
    }

    // Patch every shard: touched ones merge, the rest move their coverage
    // claim by the header's two bytes — their content cannot have changed,
    // because a shard only lists chunks its own policies appear in.
    let mut merged = 0usize;
    let mut created = 0usize;
    let mut bumped = 0usize;
    let mut added = 0usize;
    for shard in 0u8..=255 {
        let incoming: Vec<DirectoryEntry> = additions
            .remove(&shard)
            .map(|policies| {
                policies
                    .into_iter()
                    .map(|(policy_id, candidates)| DirectoryEntry {
                        policy_id,
                        candidates,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let Some((existing, header)) = shards.remove(&shard) else {
            if incoming.is_empty() {
                continue;
            }
            let bytes = DirectoryShard {
                version: DIRECTORY_V2,
                network,
                shard,
                directory_through_chunk: frontier,
                entries: incoming,
            }
            .encode()?;
            fs::write(directory.join(format!("{shard:02x}.bin")), &bytes)?;
            created += 1;
            continue;
        };
        if incoming.is_empty() {
            if header.directory_through_chunk > frontier {
                return Err(format!(
                    "shard {shard:02x} claims coverage {} beyond the segments ({frontier}) — refusing",
                    header.directory_through_chunk
                )
                .into());
            }
            if header.directory_through_chunk < frontier {
                let mut bytes = existing;
                bytes[12..14].copy_from_slice(&frontier.to_le_bytes());
                fs::write(directory.join(format!("{shard:02x}.bin")), &bytes)?;
                bumped += 1;
            }
            continue;
        }
        let patched = patch_shard(&existing, &incoming, frontier)?;
        fs::write(directory.join(format!("{shard:02x}.bin")), &patched.bytes)?;
        merged += 1;
        added += patched.added;
    }

    // The manifest, flipped last: a reader stepping between the two states
    // pays a frontier scan at worst, never a wrong answer.
    if let Some((path, old)) = pointer {
        let published_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let updated = Pointer {
            directory_through_chunk: frontier,
            published_at,
            ..old
        };
        fs::write(&path, updated.encode())?;
        println!(
            "pointer    {}: directory_through {} → {frontier}",
            path.display(),
            old.directory_through_chunk
        );
    } else {
        println!(
            "pointer    not given (--pointer): flip directory_through to {frontier} before readers see this directory"
        );
    }

    println!(
        "patched    {merged} shard(s) merged · {created} created · {bumped} coverage-bumped · {added} candidate(s) added"
    );
    println!(
        "records    {records_folded} folded from {} new segment(s); directory is v2 through {frontier}",
        new_segments.len()
    );
    Ok(())
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
