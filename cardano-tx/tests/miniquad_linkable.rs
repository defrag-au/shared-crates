//! Guard: `cardano-tx` stays linkable under miniquad.
//!
//! wasm-bindgen can NEVER link under miniquad — the `gl.js` runtime carries no
//! wasm-bindgen glue, and that is permanent rather than a version problem. So a
//! single wasm-bindgen crate anywhere in the tree is not a warning, it is the
//! difference between a macroquad host being able to build a transaction and
//! not.
//!
//! This crate reached 32 of them (measured on this branch, `-e normal` against
//! `wasm32-unknown-unknown`), through two vendor adapters wired in
//! unconditionally:
//!
//! ```text
//! cardano-tx → http-client → worker_stack → worker → wasm-bindgen
//! cardano-tx → maestro     → http-client  → (as above)
//! ```
//!
//! Both are now behind default-on features, so `default-features = false`
//! yields a crate with no HTTP in it at all. This test is what stops the edge
//! growing back: adding an innocuous-looking dependency to the default-free
//! build is exactly the change that would do it, and nothing else would notice
//! until a macroquad host failed to link.
//!
//! # Why a `cargo tree` scrape and not a build
//!
//! Building for `wasm32-unknown-unknown` would prove more, and prove it
//! slowly — and it would still not prove the miniquad half, because the failure
//! is at the *miniquad* link step in a host this workspace does not contain.
//! Resolution is what actually decides the question here: wasm-bindgen either
//! is in the graph or it is not.
//!
//! ⚠️ A NATIVE check is blind to this. The wasm-bindgen crates are behind
//! `cfg(target_arch = "wasm32")`, so `cargo tree` without `--target` reports a
//! clean graph for a crate that is anything but.

use std::process::Command;

/// The feature set a macroquad host actually takes, and what it must not drag
/// in. One row per invariant, so a failure names the configuration rather than
/// leaving someone to work out which one broke.
struct Invariant {
    package: &'static str,
    /// Extra flags after `cargo tree -p <package>`.
    features: &'static [&'static str],
    why: &'static str,
}

const INVARIANTS: &[Invariant] = &[
    Invariant {
        package: "cardano-tx",
        features: &["--no-default-features"],
        why: "transaction BUILDING is pure arithmetic over protocol parameters; \
              a macroquad host takes this crate with no features and must get \
              no HTTP stack with it",
    },
    Invariant {
        package: "macroquad-widgets",
        features: &["--all-features"],
        why: "these widgets render INSIDE miniquad, so every feature they ship \
              (including `chain`) has to stay clear of wasm-bindgen",
    },
];

/// `cargo tree` for one invariant, as lines.
fn tree(inv: &Invariant) -> Result<String, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = Command::new(cargo);
    cmd.args(["tree", "-p", inv.package])
        .args(inv.features)
        // `normal` only: a dev-dependency on wasm-bindgen would not be linked
        // into a host's binary, and flagging one would push someone to weaken a
        // test rather than fix a real edge.
        .args(["-e", "normal"])
        .args(["--target", "wasm32-unknown-unknown"])
        .current_dir(env!("CARGO_MANIFEST_DIR"));

    let out = cmd
        .output()
        .map_err(|e| format!("could not run `cargo tree`: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "`cargo tree -p {}` failed:\n{}",
            inv.package,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
fn the_miniquad_linkable_set_carries_no_wasm_bindgen() {
    for inv in INVARIANTS {
        let tree = match tree(inv) {
            Ok(t) => t,
            Err(e) => panic!("{e}"),
        };

        let offenders: Vec<&str> = tree
            .lines()
            .filter(|l| l.contains("wasm-bindgen"))
            .collect();

        assert!(
            offenders.is_empty(),
            "`{}` {} reaches wasm-bindgen, which can never link under \
             miniquad:\n{}\n\nWhy this matters: {}.\n\nFind the edge with:\n  \
             cargo tree -p {} {} -e normal --target wasm32-unknown-unknown \
             -i wasm-bindgen",
            inv.package,
            inv.features.join(" "),
            offenders.join("\n"),
            inv.why,
            inv.package,
            inv.features.join(" "),
        );
    }
}

/// The negative control.
///
/// Without this, the test above would still pass if the features silently
/// stopped pulling anything at all — a typo'd feature name, a dependency
/// dropped by accident — and it would look like a success. The default build
/// SHOULD reach wasm-bindgen (that is what `http` and `maestro` are for), so
/// asserting it proves the guard is measuring the feature split rather than
/// measuring nothing.
#[test]
fn the_default_build_still_reaches_wasm_bindgen_so_the_guard_means_something() {
    let default = Invariant {
        package: "cardano-tx",
        features: &[],
        why: "",
    };
    let tree = tree(&default).expect("cargo tree failed for the default build");

    assert!(
        tree.contains("wasm-bindgen"),
        "the DEFAULT `cardano-tx` build no longer reaches wasm-bindgen.\n\n\
         That is not a problem in itself — it may be good news — but it means \
         the guard above is no longer proving that `--no-default-features` is \
         what removes it. Either delete this control and say why, or check the \
         `maestro` / `http` features still do what their comments claim."
    );
}
