//! Pins each JS plugin the kit vendors (`js/`, served by `web::Plugin`) to
//! the file inside the crate version Cargo resolves for this workspace.
//!
//! miniquad's plugin protocol is unversioned: a JS file from one version of
//! quad-snd or gamepads beside the Rust of another loads without complaint
//! and misbehaves. So a bump of either crate fails here until the vendored
//! copy is refreshed — copy the file named in the failure over `js/`.
//!
//! Runs only with the plugins' features on (`cargo test -p macroquad-gamekit
//! --all-features`): that build is what puts their crates' sources on disk,
//! and the test reads them offline. Default-feature CI skips it.

#![cfg(all(feature = "audio", feature = "gamepads", feature = "discord-auth"))]

use std::path::PathBuf;
use std::process::Command;

use macroquad_gamekit::web::Plugin;

/// `cargo metadata` for this crate's workspace with the kit's own features
/// on, so the optional crates are in the graph. Not `--all-features`: that
/// turns on every feature of every workspace crate and needs packages
/// nothing here uses.
fn metadata() -> String {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let out = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--features",
            "macroquad-gamekit/audio,macroquad-gamekit/gamepads,macroquad-gamekit/discord-auth",
            "--offline",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run cargo metadata");
    assert!(
        out.status.success(),
        "cargo metadata failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("metadata is UTF-8")
}

/// Every resolved package named `name`: (version, crate directory).
///
/// A plain scan rather than a JSON parser, so the kit takes no dev-dependency
/// for one test. Package objects open with `"name":…,"version":…` (a
/// dependency entry has `"source"` second), and the package's own
/// `manifest_path` is the next one after its name.
fn packages(meta: &str, name: &str) -> Vec<(String, PathBuf)> {
    let head = format!("{{\"name\":\"{name}\",\"version\":\"");
    meta.match_indices(&head)
        .map(|(at, _)| {
            let rest = &meta[at + head.len()..];
            let version = rest[..rest.find('"').expect("version ends")].to_owned();
            let key = "\"manifest_path\":\"";
            let mp = &rest[rest.find(key).expect("package has a manifest_path") + key.len()..];
            let manifest = PathBuf::from(&mp[..mp.find('"').expect("path ends")]);
            (
                version,
                manifest.parent().expect("manifest dir").to_path_buf(),
            )
        })
        .collect()
}

#[test]
fn vendored_plugins_match_the_resolved_crates() {
    let meta = metadata();
    for plugin in Plugin::ALL {
        let (krate, file) = plugin.origin();
        let found = packages(&meta, krate);
        assert_eq!(
            found.len(),
            1,
            "{krate}: expected exactly one resolved version, found {found:?}"
        );
        let (version, dir) = &found[0];
        let path = dir.join(file);
        let upstream = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{krate} {version}: read {}: {e}", path.display()));
        assert!(
            upstream == plugin.source(),
            "{krate} {version}'s {file} differs from the kit's copy for {plugin:?}.\n\
             Refresh it: cp {} gaming/macroquad-gamekit/js/",
            path.display()
        );
    }
}

#[test]
fn the_gamepads_version_written_into_the_page_is_the_crates() {
    let meta = metadata();
    let found = packages(&meta, "gamepads");
    let (version, _) = &found[0];
    let packed = macroquad_gamekit::web::packed_version(version).expect("semver");
    assert!(
        Plugin::Gamepads.js().contains(&format!("version:{packed}")),
        "stamped gamepads plugin does not declare {version} as {packed}"
    );
}
