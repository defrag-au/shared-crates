//! Where the app crate and miniquad's `java/` directory actually are, from
//! `cargo metadata`.
//!
//! miniquad's Java has to come from the version this app links, not from "the
//! newest on disk" or a copy committed somewhere: the JNI entry points in the
//! `.so` and the `native` declarations in `QuadNative.java` are an unversioned
//! contract, and a mismatch is an `UnsatisfiedLinkError` at launch. So walk the
//! resolve graph from the app crate and take the one miniquad it reaches. Two
//! versions is an error rather than a guess.
//!
//! The same walk as `ui/macroquad-bundle/src/metadata.rs`, which does it for
//! wasm32. Kept here rather than shared because the web bundler's answer is a
//! page's scripts and this one's is an APK's Java; the crate that packages an
//! APK should not have to load the crate that writes HTML.

use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The target `cargo metadata` is filtered by. Both ABIs are in the graph for
/// the purposes of dependencies; the primary one is enough to walk it.
pub const PRIMARY_TARGET: &str = "aarch64-linux-android";

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

/// What the packager needs to know about the workspace.
#[derive(Debug)]
pub struct Resolved {
    /// Where cargo writes artifacts (honours `CARGO_TARGET_DIR`).
    pub target_dir: PathBuf,
    /// The app crate's directory.
    pub crate_dir: PathBuf,
    /// The miniquad version the app links.
    pub miniquad: String,
    /// That version's `java/` directory.
    pub miniquad_java: PathBuf,
}

/// Resolve `package` and the miniquad it links, from `cwd`.
pub fn resolve(cwd: &Path, package: &str, target: &str) -> Result<Resolved, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--filter-platform",
            target,
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
    let metadata: Metadata =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("parse cargo metadata: {e}"))?;
    walk(&metadata, package)
}

fn walk(metadata: &Metadata, package: &str) -> Result<Resolved, String> {
    let by_id: HashMap<&str, &Package> = metadata
        .packages
        .iter()
        .map(|p| (p.id.as_str(), p))
        .collect();
    let root = metadata
        .workspace_members
        .iter()
        .filter_map(|id| by_id.get(id.as_str()))
        .find(|p| p.name == package)
        .ok_or_else(|| {
            format!(
                "`{package}` is not a workspace member here — run mq-apk from the app's workspace root"
            )
        })?;
    let resolve = metadata
        .resolve
        .as_ref()
        .ok_or("cargo metadata returned no resolve graph")?;
    let nodes: HashMap<&str, &Node> = resolve.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // Normal dependencies only: build and dev dependencies never reach the
    // shared object.
    let mut seen: HashSet<&str> = HashSet::from([root.id.as_str()]);
    let mut queue = VecDeque::from([root.id.as_str()]);
    while let Some(id) = queue.pop_front() {
        let Some(node) = nodes.get(id) else { continue };
        for dep in &node.deps {
            if dep.dep_kinds.iter().any(|kind| kind.kind.is_none()) && seen.insert(dep.pkg.as_str())
            {
                queue.push_back(dep.pkg.as_str());
            }
        }
    }

    let mut miniquad: Vec<&Package> = seen
        .iter()
        .filter_map(|id| by_id.get(*id).copied())
        .filter(|p| p.name == "miniquad")
        .collect();
    miniquad.sort_by(|a, b| a.version.cmp(&b.version));
    let found = match miniquad.as_slice() {
        [] => {
            return Err(format!(
                "`{package}` does not link miniquad — nothing to package"
            ));
        }
        [one] => *one,
        many => {
            return Err(format!(
                "miniquad resolves to {} versions ({}) — a build links one",
                many.len(),
                many.iter()
                    .map(|p| p.version.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    };

    let dir = |p: &Package| {
        p.manifest_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf()
    };
    Ok(Resolved {
        target_dir: metadata.target_directory.clone(),
        crate_dir: dir(root),
        miniquad: found.version.clone(),
        miniquad_java: dir(found).join("java"),
    })
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

    /// app → macroquad 0.4 → miniquad 0.4.11; the workspace's other member
    /// links miniquad 0.4.10; the app also has a build dependency that links
    /// miniquad by another version.
    fn fixture() -> Metadata {
        let json = format!(
            r#"{{"packages":[{},{},{},{},{},{}],
                "workspace_members":["app 0.1.0","other 0.1.0"],
                "target_directory":"/t",
                "resolve":{{"nodes":[
                  {{"id":"app 0.1.0","deps":[{},{}]}},
                  {{"id":"macroquad 0.4.0","deps":[{}]}},
                  {{"id":"other 0.1.0","deps":[{}]}},
                  {{"id":"miniquad 0.4.11","deps":[]}},
                  {{"id":"miniquad 0.4.10","deps":[]}},
                  {{"id":"miniquad 0.4.9","deps":[]}}
                ]}}}}"#,
            pkg("app", "0.1.0"),
            pkg("other", "0.1.0"),
            pkg("macroquad", "0.4.0"),
            pkg("miniquad", "0.4.11"),
            pkg("miniquad", "0.4.10"),
            pkg("miniquad", "0.4.9"),
            dep("macroquad 0.4.0", "null"),
            dep("miniquad 0.4.9", "\"build\""),
            dep("miniquad 0.4.11", "null"),
            dep("miniquad 0.4.10", "null"),
        );
        serde_json::from_str(&json).expect("fixture parses")
    }

    #[test]
    fn takes_the_miniquad_this_app_links() {
        let resolved = walk(&fixture(), "app").expect("walk");
        assert_eq!(resolved.miniquad, "0.4.11");
        assert_eq!(
            resolved.miniquad_java,
            Path::new("/src/miniquad-0.4.11/java")
        );
        assert_eq!(resolved.crate_dir, Path::new("/src/app-0.1.0"));
        assert_eq!(resolved.target_dir, Path::new("/t"));
    }

    #[test]
    fn build_dependencies_do_not_count() {
        // app's build dependency reaches 0.4.9; that is not the Java to ship.
        let resolved = walk(&fixture(), "app").expect("walk");
        assert_ne!(resolved.miniquad, "0.4.9");
    }

    #[test]
    fn two_versions_reached_is_an_error_naming_both() {
        let mut metadata = fixture();
        let app = metadata
            .resolve
            .as_mut()
            .expect("resolve")
            .nodes
            .iter_mut()
            .find(|n| n.id == "app 0.1.0")
            .expect("app node");
        app.deps
            .push(serde_json::from_str(&dep("miniquad 0.4.10", "null")).expect("dep"));
        let err = walk(&metadata, "app").expect_err("ambiguous");
        assert!(err.contains("0.4.10"), "{err}");
        assert!(err.contains("0.4.11"), "{err}");
    }

    #[test]
    fn a_package_that_is_not_a_member_is_named() {
        let err = walk(&fixture(), "nope").expect_err("not a member");
        assert!(err.contains("nope"), "{err}");
        assert!(err.contains("workspace root"), "{err}");
    }

    #[test]
    fn a_package_without_miniquad_is_an_error() {
        let json = format!(
            r#"{{"packages":[{}],"workspace_members":["app 0.1.0"],
                "target_directory":"/t",
                "resolve":{{"nodes":[{{"id":"app 0.1.0","deps":[]}}]}}}}"#,
            pkg("app", "0.1.0")
        );
        let metadata: Metadata = serde_json::from_str(&json).expect("parses");
        let err = walk(&metadata, "app").expect_err("no miniquad");
        assert!(err.contains("does not link miniquad"), "{err}");
    }
}
