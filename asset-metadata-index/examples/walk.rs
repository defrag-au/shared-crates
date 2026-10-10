//! A native walk over a published build on disk — the isolated harness.
//!
//! The files are the objects: `<root>/current`, `<root>/directory/XX.bin`,
//! `<root>/segments/NNNNN.bin` (an empty prefix). No network, no worker:
//! `std::fs` implements the same [`RangeSource`] the edge will.
//!
//! ```text
//! cargo run -p asset-metadata-index --example walk -- <root> list <chunk> [--resolvable]
//! cargo run -p asset-metadata-index --example walk -- <root> resolve <policy-hex> <name-hex>
//! ```
//!
//! `list --resolvable` prints only the policies from that segment whose every
//! directory-listed chunk is present locally — the entries a full `resolve`
//! can run against without fetching anything else.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs::File;
use std::future::Future;
use std::io::{Read as _, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use asset_metadata_index::{
    DirectoryView, POLICY_ID_LEN, RangeSource, Read, Record, SegmentView, directory_key,
    resolve_asset, segment_key,
};

const PREFIX: &str = "";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome: Result<(), Box<dyn std::error::Error>> = match args.as_slice() {
        [root, mode, chunk, rest @ ..] if mode == "list" => list(
            Path::new(root),
            chunk,
            rest.iter().any(|flag| flag == "--resolvable"),
        ),
        [root, mode, policy, name] if mode == "resolve" => resolve(Path::new(root), policy, name),
        _ => {
            eprintln!(
                "usage:\n  walk <root> list <chunk> [--resolvable]\n  walk <root> resolve <policy-hex> <name-hex>"
            );
            std::process::exit(2);
        }
    };
    if let Err(error) = outcome {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn list(root: &Path, chunk: &str, resolvable_only: bool) -> Result<(), Box<dyn std::error::Error>> {
    let chunk: u16 = chunk.parse()?;
    let bytes = std::fs::read(root.join(segment_key(PREFIX, chunk)))?;
    let view = SegmentView::parse(&bytes)?;
    println!(
        "segment {}: {} records, {} policies",
        view.header.chunk, view.header.record_count, view.header.index_count
    );

    let mut shards: HashMap<u8, Vec<u8>> = HashMap::new();
    let mut shown = 0usize;
    for entry in view.entries()? {
        let policy_id = entry.policy_id;
        let shard = match shards.entry(policy_id[0]) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => entry.insert(std::fs::read(
                root.join(directory_key(PREFIX, policy_id[0])),
            )?),
        };
        let directory = DirectoryView::parse(shard.as_slice())?;
        let chunks = directory.chunks_for(&policy_id)?.unwrap_or_default();
        if resolvable_only {
            let fully_local = chunks
                .iter()
                .all(|chunk| root.join(segment_key(PREFIX, *chunk)).exists());
            if !fully_local {
                continue;
            }
        }
        let records = view
            .records_for(&policy_id)?
            .ok_or_else(|| format!("{} has an index entry but no slice", hex(&policy_id)))?
            .collect::<Result<Vec<_>, _>>()?;
        println!(
            "{}  chunks {chunks:?}  {} records :: {}",
            hex(&policy_id),
            records.len(),
            describe(&records)
        );
        shown += 1;
        if !resolvable_only && shown >= 10 {
            println!("...");
            break;
        }
    }
    Ok(())
}

fn resolve(root: &Path, policy: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let policy_bytes = unhex(policy)?;
    let policy_id: [u8; POLICY_ID_LEN] = policy_bytes.as_slice().try_into().map_err(|_| {
        format!(
            "policy id must be {} bytes ({} hex chars), got {}",
            POLICY_ID_LEN,
            POLICY_ID_LEN * 2,
            policy_bytes.len()
        )
    })?;
    let name = unhex(name)?;

    let source = Files {
        root: root.to_path_buf(),
    };
    match block_on(resolve_asset(&source, PREFIX, &policy_id, &name)) {
        Ok(None) => {
            println!("not covered by this build");
            Ok(())
        }
        Ok(Some(asset)) => {
            let standing = if asset.burned { "burned" } else { "active" };
            println!("{standing} · {:?} · {:?}", asset.family, asset.event);
            println!(
                "name       {} ({})",
                hex(&asset.name),
                String::from_utf8_lossy(&asset.name)
            );
            println!("image      {}", String::from_utf8_lossy(&asset.image));
            println!("media-type {}", String::from_utf8_lossy(&asset.media_type));
            println!("media      {} link(s)", asset.media.len());
            println!("payload    {} bytes", asset.payload.len());
            println!(
                "provenance chunk {} offset {} (body {} B, aux {} + {} B)",
                asset.provenance.chunk,
                asset.provenance.body_offset,
                asset.provenance.body_len,
                asset.provenance.aux_offset,
                asset.provenance.aux_len
            );
            println!("as of      chunk {}", asset.as_of_chunk);
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn describe(records: &[Record]) -> String {
    records
        .iter()
        .map(|record| {
            format!(
                "{:?} {:?} {} ({})",
                record.family,
                record.event,
                hex(&record.name),
                String::from_utf8_lossy(&record.name)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `std::fs` as a [`RangeSource`] — the whole point of the harness.
struct Files {
    root: PathBuf,
}

impl RangeSource for Files {
    type Error = std::io::Error;

    async fn fetch(&self, reads: &[Read]) -> Vec<Result<Vec<u8>, std::io::Error>> {
        reads
            .iter()
            .map(|read| read_one(&self.root, read))
            .collect()
    }
}

fn read_one(root: &Path, read: &Read) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(root.join(&read.key))?;
    file.seek(SeekFrom::Start(read.offset))?;
    match read.len {
        None => {
            let mut out = Vec::new();
            file.read_to_end(&mut out)?;
            Ok(out)
        }
        Some(len) => {
            let mut out = vec![0u8; len as usize];
            file.read_exact(&mut out)?;
            Ok(out)
        }
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, Waker};

    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn unhex(input: &str) -> Result<Vec<u8>, String> {
    let bytes = input.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err(format!("odd hex length: {}", bytes.len()));
    }
    bytes
        .chunks(2)
        .map(|pair| {
            let pair =
                std::str::from_utf8(pair).map_err(|_| "hex input is not ASCII".to_string())?;
            u8::from_str_radix(pair, 16).map_err(|error| format!("bad hex {pair:?}: {error}"))
        })
        .collect()
}
