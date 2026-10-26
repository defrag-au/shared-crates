//! Write the published stake-service files from the compiled tables.
//!
//! ```sh
//! cargo run -p address-registry --bin publish-stake-services
//! ```
//!
//! Commit what it writes with the change that made it: the crate's
//! `the_published_files_match_the_code` test fails until they agree. See
//! `address_registry::published`.

use std::path::Path;

use address_registry::published::{BIN_PATH, JSON_PATH, StakeServiceTable};

fn main() -> std::io::Result<()> {
    let table = StakeServiceTable::compiled();
    let problems = table.check();
    if !problems.is_empty() {
        eprintln!("the compiled table fails its checks:");
        for p in &problems {
            eprintln!("  {p}");
        }
        std::process::exit(1);
    }
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let json = table.to_json();
    // The binary is the lookup only: keys as bytes, kinds and labels.
    let bytes = table.labels().to_bytes();
    for (rel, data) in [(JSON_PATH, json.as_bytes()), (BIN_PATH, bytes.as_slice())] {
        let path = crate_dir.join(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, data)?;
        println!("wrote {} ({} bytes)", path.display(), data.len());
    }
    println!(
        "{} services, {} exclusions",
        table.services.len(),
        table.exclusions.len()
    );
    Ok(())
}
