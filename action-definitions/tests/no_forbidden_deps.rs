//! The crate's purity, asserted rather than intended.
//!
//! This crate is consumed from a Cloudflare worker, a wasm-bindgen frontend,
//! a **macroquad app** (where wasm-bindgen can never run — miniquad's `gl.js`
//! has no wasm-bindgen glue, and that is permanent), a native verifier
//! binary and eventually a wasip2 mitos module. One transitive dependency on
//! any of the names below breaks at least one of those, and it breaks at
//! link time in a downstream repo rather than here — which is why the check
//! lives in this crate's own test suite.

use std::process::Command;

/// Names that must never appear in the normal dependency tree.
const FORBIDDEN: &[&str] = &[
    "wasm-bindgen",
    "wasm-bindgen-futures",
    "js-sys",
    "web-sys",
    "worker",
    "tokio",
    "reqwest",
    "getrandom",
];

#[test]
fn the_dependency_tree_stays_pure() {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--package",
            "action-definitions",
            // Normal deps only: build-dependencies (the proc-macro's syn and
            // friends) compile for the host and never reach the artifact,
            // and dev-dependencies are test-only.
            "--edges",
            "normal",
            "--prefix",
            "none",
            "--no-dedupe",
        ])
        .output()
        .expect("cargo tree");

    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let tree = String::from_utf8_lossy(&output.stdout);
    let mut found = Vec::new();
    for line in tree.lines() {
        // Lines are "name v1.2.3" with --prefix none.
        let Some(name) = line.split_whitespace().next() else {
            continue;
        };
        if FORBIDDEN.contains(&name) {
            found.push(name.to_string());
        }
    }
    found.sort();
    found.dedup();

    assert!(
        found.is_empty(),
        "forbidden dependencies reached the tree: {found:?}\n\
         This crate must build for wasm32-unknown-unknown under macroquad, \
         where wasm-bindgen cannot run.\n\nfull tree:\n{tree}"
    );
}
