//! Which crates a package actually links for wasm32, and where each one's
//! source is — from `cargo metadata`.
//!
//! The JS half of the miniquad stack must come from the same version as the
//! Rust half just linked: the plugin protocol is untyped and unversioned, so
//! a gl.js from one miniquad beside a wasm built against another is a blank
//! canvas with no error. "Newest on disk" and "the copy committed last year"
//! both drift (cnft.dev-workers shipped miniquad 0.4.10's gl.js beside a
//! 0.4.11 wasm). So: walk the resolve graph from the package being bundled,
//! and take each crate's file from the version that graph reaches. A crate
//! reached at two versions is an error, because a page can load only one.

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
    resolve: Option<Resolve>,
    target_directory: PathBuf,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
    id: String,
    manifest_path: PathBuf,
    /// `registry+…` for crates.io, `git+…` for a git dependency or `[patch]`,
    /// absent for a path dependency.
    #[serde(default)]
    source: Option<String>,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    deps: Vec<NodeDep>,
}

#[derive(Deserialize)]
struct NodeDep {
    pkg: String,
    dep_kinds: Vec<DepKind>,
}

#[derive(Deserialize)]
struct DepKind {
    /// `None` for a normal dependency; "build" or "dev" otherwise.
    kind: Option<String>,
}

/// The crates one package links when built for wasm32.
#[derive(Debug)]
pub struct Linked {
    /// Where cargo writes artifacts (honours `CARGO_TARGET_DIR`).
    pub target_dir: PathBuf,
    /// Crate name → every (version, source directory) the package reaches.
    crates: BTreeMap<String, Vec<(String, PathBuf)>>,
    /// Crate name → the version of each copy that comes from crates.io.
    registry: BTreeMap<String, Vec<String>>,
}

/// The last macroquad release with the glyph-atlas doubling bug; the fix
/// (`8d602d3`) landed after it. See the `[patch.crates-io]` in shared-crates'
/// root `Cargo.toml`.
pub const LAST_UNFIXED_MACROQUAD: (u64, u64, u64) = (0, 4, 16);

/// What [`Linked::macroquad_patch`] found.
#[derive(Debug, PartialEq, Eq)]
pub enum PatchCheck {
    /// macroquad comes from git (the `[patch]`), a path, or a fixed release.
    Fine,
    /// macroquad comes from a crates.io release that still has the bug.
    Unpatched {
        version: String,
        /// The package links `macroquad-gamekit`, which requires the patch.
        gamekit: bool,
    },
}

impl Linked {
    /// Whether the macroquad this package links has the glyph-atlas fix.
    ///
    /// A `[patch]` only applies in the workspace that builds, so a game in its
    /// own workspace that forgets to copy it silently gets the bug back: one
    /// run in twelve ended with a 256 MB font atlas.
    pub fn macroquad_patch(&self) -> PatchCheck {
        let gamekit = self.crates.contains_key("macroquad-gamekit");
        let unfixed = self
            .registry
            .get("macroquad")
            .into_iter()
            .flatten()
            .find(|v| semver(v).is_some_and(|v| v <= LAST_UNFIXED_MACROQUAD));
        match unfixed {
            Some(version) => PatchCheck::Unpatched {
                version: version.clone(),
                gamekit,
            },
            None => PatchCheck::Fine,
        }
    }
    /// The source directory of `name`, if the package links it at all.
    pub fn crate_dir(&self, name: &str) -> Result<Option<&Path>, String> {
        match self.crates.get(name).map(Vec::as_slice) {
            None | Some([]) => Ok(None),
            Some([(_, dir)]) => Ok(Some(dir)),
            Some(many) => Err(format!(
                "{name} is linked at {} versions ({}); a page can load only one copy of its JS",
                many.len(),
                many.iter()
                    .map(|(v, _)| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

/// Run `cargo metadata` in `cwd` and resolve what `package` links for wasm32.
pub fn linked(cwd: &Path, package: &str) -> Result<Linked, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--filter-platform",
            "wasm32-unknown-unknown",
        ])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("run cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let meta: Metadata =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("parse cargo metadata: {e}"))?;
    walk(&meta, package)
}

fn walk(meta: &Metadata, package: &str) -> Result<Linked, String> {
    let by_id: HashMap<&str, &Package> = meta.packages.iter().map(|p| (p.id.as_str(), p)).collect();
    let root = meta
        .workspace_members
        .iter()
        .filter_map(|id| by_id.get(id.as_str()))
        .find(|p| p.name == package)
        .ok_or_else(|| format!("no workspace member named `{package}` here"))?;
    let resolve = meta
        .resolve
        .as_ref()
        .ok_or("cargo metadata returned no resolve graph")?;
    let nodes: HashMap<&str, &Node> = resolve.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // Normal dependencies only: build and dev dependencies are compiled for
    // the host and never reach the wasm.
    let mut seen: HashSet<&str> = HashSet::from([root.id.as_str()]);
    let mut queue = VecDeque::from([root.id.as_str()]);
    while let Some(id) = queue.pop_front() {
        let Some(node) = nodes.get(id) else { continue };
        for dep in &node.deps {
            let normal = dep.dep_kinds.iter().any(|k| k.kind.is_none());
            if normal && seen.insert(dep.pkg.as_str()) {
                queue.push_back(dep.pkg.as_str());
            }
        }
    }

    let mut crates: BTreeMap<String, Vec<(String, PathBuf)>> = BTreeMap::new();
    let mut registry: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for id in seen {
        let Some(p) = by_id.get(id) else { continue };
        if p.source
            .as_deref()
            .is_some_and(|s| s.starts_with("registry+"))
        {
            registry
                .entry(p.name.clone())
                .or_default()
                .push(p.version.clone());
        }
        let dir = p
            .manifest_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let entry = crates.entry(p.name.clone()).or_default();
        if !entry.iter().any(|(v, _)| *v == p.version) {
            entry.push((p.version.clone(), dir));
        }
    }
    for versions in crates.values_mut() {
        versions.sort();
    }
    for versions in registry.values_mut() {
        versions.sort();
    }
    Ok(Linked {
        target_dir: meta.target_directory.clone(),
        crates,
        registry,
    })
}

/// `major.minor.patch` of a version string, ignoring any pre-release or
/// build suffix.
fn semver(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, version: &str) -> String {
        format!(
            r#"{{"name":"{name}","version":"{version}","id":"{name} {version}","manifest_path":"/src/{name}-{version}/Cargo.toml"}}"#
        )
    }

    fn dep(id: &str, kind: &str) -> String {
        format!(r#"{{"pkg":"{id}","dep_kinds":[{{"kind":{kind}}}]}}"#)
    }

    /// game → macroquad 0.4 → miniquad 0.4.11; game builds with
    /// miniquad-platform (build dep); `other` (another member) → miniquad 0.4.10.
    fn fixture() -> Metadata {
        let json = format!(
            r#"{{"packages":[{},{},{},{},{},{}],
                "workspace_members":["game 0.1.0","other 0.1.0"],
                "target_directory":"/t",
                "resolve":{{"nodes":[
                  {{"id":"game 0.1.0","deps":[{},{}]}},
                  {{"id":"macroquad 0.4.0","deps":[{}]}},
                  {{"id":"other 0.1.0","deps":[{}]}},
                  {{"id":"miniquad 0.4.11","deps":[]}},
                  {{"id":"miniquad 0.4.10","deps":[]}},
                  {{"id":"miniquad-platform 0.1.0","deps":[]}}
                ]}}}}"#,
            pkg("game", "0.1.0"),
            pkg("other", "0.1.0"),
            pkg("macroquad", "0.4.0"),
            pkg("miniquad", "0.4.11"),
            pkg("miniquad", "0.4.10"),
            pkg("miniquad-platform", "0.1.0"),
            dep("macroquad 0.4.0", "null"),
            dep("miniquad-platform 0.1.0", "\"build\""),
            dep("miniquad 0.4.11", "null"),
            dep("miniquad 0.4.10", "null"),
        );
        serde_json::from_str(&json).expect("fixture parses")
    }

    #[test]
    fn takes_the_version_this_package_links_not_the_workspaces_other() {
        let linked = walk(&fixture(), "game").expect("walk");
        let dir = linked
            .crate_dir("miniquad")
            .expect("one version")
            .expect("linked");
        assert_eq!(dir, Path::new("/src/miniquad-0.4.11"));
        assert_eq!(linked.target_dir, Path::new("/t"));
    }

    #[test]
    fn build_dependencies_do_not_count() {
        let linked = walk(&fixture(), "game").expect("walk");
        assert_eq!(linked.crate_dir("miniquad-platform").expect("ok"), None);
    }

    #[test]
    fn two_versions_of_one_crate_is_an_error() {
        let mut linked = walk(&fixture(), "game").expect("walk");
        linked
            .crates
            .get_mut("miniquad")
            .expect("linked")
            .push(("0.4.10".into(), "/x".into()));
        assert!(linked.crate_dir("miniquad").is_err());
    }

    #[test]
    fn an_unknown_package_is_an_error() {
        assert!(walk(&fixture(), "nope").is_err());
    }

    /// game → (gamekit) → macroquad `version`, from `source`.
    fn macroquad_from(version: &str, source: &str, gamekit: bool) -> Linked {
        let mq = format!(
            r#"{{"name":"macroquad","version":"{version}","id":"macroquad {version}","manifest_path":"/src/mq/Cargo.toml","source":{source}}}"#
        );
        let mq_id = format!("macroquad {version}");
        let (kit_pkg, kit_node, game_dep) = if gamekit {
            (
                format!(",{}", pkg("macroquad-gamekit", "0.1.0")),
                format!(
                    r#",{{"id":"macroquad-gamekit 0.1.0","deps":[{}]}}"#,
                    dep(&mq_id, "null")
                ),
                dep("macroquad-gamekit 0.1.0", "null"),
            )
        } else {
            (String::new(), String::new(), dep(&mq_id, "null"))
        };
        let json = format!(
            r#"{{"packages":[{},{mq}{kit_pkg}],
                "workspace_members":["game 0.1.0"],
                "target_directory":"/t",
                "resolve":{{"nodes":[
                  {{"id":"game 0.1.0","deps":[{game_dep}]}},
                  {{"id":"{mq_id}","deps":[]}}{kit_node}
                ]}}}}"#,
            pkg("game", "0.1.0"),
        );
        let meta: Metadata = serde_json::from_str(&json).expect("fixture parses");
        walk(&meta, "game").expect("walk")
    }

    const CRATES_IO: &str = r#""registry+https://github.com/rust-lang/crates.io-index""#;
    const PATCHED: &str = r#""git+https://github.com/not-fl3/macroquad?rev=8d602d3f8ebb8a3a582ee15d92983bbb81ee3ef6#8d602d3f""#;

    #[test]
    fn a_gamekit_on_macroquad_0_4_16_from_crates_io_is_unpatched() {
        assert_eq!(
            macroquad_from("0.4.16", CRATES_IO, true).macroquad_patch(),
            PatchCheck::Unpatched {
                version: "0.4.16".into(),
                gamekit: true
            }
        );
        assert_eq!(
            macroquad_from("0.4.14", CRATES_IO, false).macroquad_patch(),
            PatchCheck::Unpatched {
                version: "0.4.14".into(),
                gamekit: false
            }
        );
    }

    #[test]
    fn the_git_patch_a_path_or_a_fixed_release_is_fine() {
        assert_eq!(
            macroquad_from("0.4.16", PATCHED, true).macroquad_patch(),
            PatchCheck::Fine
        );
        assert_eq!(
            macroquad_from("0.4.16", "null", true).macroquad_patch(),
            PatchCheck::Fine
        );
        assert_eq!(
            macroquad_from("0.4.17", CRATES_IO, true).macroquad_patch(),
            PatchCheck::Fine
        );
        assert_eq!(
            macroquad_from("0.5.0-alpha.1", CRATES_IO, true).macroquad_patch(),
            PatchCheck::Fine
        );
    }
}
