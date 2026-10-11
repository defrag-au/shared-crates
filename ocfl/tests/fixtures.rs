//! The OCFL community fixture suite — `https://github.com/OCFL/ocfl-fixtures`
//! — run against this crate when a checkout is reachable.
//!
//! Set `OCFL_FIXTURES` to the checkout (absolute, or relative to this crate)
//! and run:
//!
//! ```sh
//! OCFL_FIXTURES=.tmp/ocfl-fixtures cargo test -p ocfl --test fixtures -- --nocapture
//! ```
//!
//! Without the variable the test reports what it skipped and passes: the suite
//! is a checkout, not a crates.io dependency, so a clean `cargo test` stays
//! hermetic. CI runs this test as its own job against a pinned revision of the
//! fixtures repo (`.github/workflows/ci.yaml`, `ocfl-fixtures`) — a red build
//! there means this validator drifted, not that upstream moved. To rehearse
//! locally the way CI runs it:
//!
//! ```sh
//! git clone --depth 1 https://github.com/OCFL/ocfl-fixtures .tmp/ocfl-fixtures
//! OCFL_FIXTURES=.tmp/ocfl-fixtures cargo test -p ocfl --test fixtures -- --nocapture
//! ```
//!
//! The `1.0` tree is not run — this crate speaks OCFL 1.1 by design and
//! rejects 1.0 conformance declarations.

use std::path::{Path, PathBuf};

use ocfl::fs::Object;

fn fixtures_root() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("OCFL_FIXTURES")?);
    let path = if path.is_absolute() {
        path
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
    };
    path.is_dir().then_some(path)
}

fn object_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    paths
}

fn rejected(path: &Path) -> bool {
    match Object::open(path) {
        Err(_) => true,
        Ok(object) => !object.verify().is_ok(),
    }
}

#[test]
fn the_ocfl_1_1_community_fixtures_pass() {
    let Some(root) = fixtures_root() else {
        eprintln!(
            "OCFL_FIXTURES is not set to an ocfl-fixtures checkout — the community fixture suite was skipped"
        );
        return;
    };
    let base = root.join("1.1");
    let mut failures = Vec::new();
    let mut counts = (0usize, 0usize, 0usize);

    for path in object_dirs(&base.join("good-objects")) {
        counts.0 += 1;
        match Object::open(&path) {
            Ok(object) => {
                let report = object.verify();
                if !report.is_ok() {
                    failures.push(format!(
                        "{}: a good object was rejected:\n{:#?}",
                        path.display(),
                        report.issues
                    ));
                }
            }
            Err(error) => failures.push(format!(
                "{}: a good object failed to open: {error}",
                path.display()
            )),
        }
    }
    for path in object_dirs(&base.join("warn-objects")) {
        counts.1 += 1;
        if rejected(&path) {
            failures.push(format!(
                "{}: a warn object was rejected (it must pass with warnings at most)",
                path.display()
            ));
        }
    }
    for path in object_dirs(&base.join("bad-objects")) {
        counts.2 += 1;
        if !rejected(&path) {
            failures.push(format!("{}: a bad object was accepted", path.display()));
        }
    }

    eprintln!(
        "fixtures: {} good, {} warn, {} bad — {} mismatch(es)",
        counts.0,
        counts.1,
        counts.2,
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} fixture mismatch(es):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(counts.0 > 0 && counts.2 > 0, "the fixture tree looks empty");
}
