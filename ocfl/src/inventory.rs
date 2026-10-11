//! The OCFL object inventory — the JSON that tracks an object's history and
//! current state (§3.5) — plus its digest sidecar (§3.6) and the structural
//! validation this crate enforces.
//!
//! The inventory is the one file that changes between versions: the root copy
//! must equal the newest version directory's copy, and its `head` names that
//! version. Everything else in an object is immutable once written.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::digest::{DigestAlgorithm, digest_eq};
use crate::time::has_rfc3339_shape;

/// The inventory `type` URI for OCFL 1.1 — the spec requires the exact string.
pub const INVENTORY_TYPE_1_1: &str = "https://ocfl.io/1.1/spec/#inventory";
/// The inventory filename (§3.5).
pub const INVENTORY_FILENAME: &str = "inventory.json";
/// The object-root conformance declaration (§3.2).
pub const OBJECT_DECL_FILENAME: &str = "0=ocfl_object_1.1";
/// Its textual contents (the declaration value plus `\n`, per NAMASTE).
pub const OBJECT_DECL_CONTENTS: &str = "ocfl_object_1.1\n";
/// The storage-root conformance declaration (§4.2).
pub const ROOT_DECL_FILENAME: &str = "0=ocfl_1.1";
/// Its textual contents.
pub const ROOT_DECL_CONTENTS: &str = "ocfl_1.1\n";
/// The content directory name when `contentDirectory` is absent (§3.3.1).
pub const DEFAULT_CONTENT_DIRECTORY: &str = "content";

/// The version directory name for a version number — `v1`, `v2`, … (the
/// spec's recommended non-padded form).
pub fn version_dir(number: u32) -> String {
    format!("v{number}")
}

/// The version number for a `vN` directory name, if it is well formed
/// (padding allowed by the spec; consistency is checked in
/// [`Inventory::validate`]).
pub fn version_number(dir: &str) -> Option<u32> {
    let digits = dir.strip_prefix('v')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The sidecar filename for an inventory digest (§3.6).
pub fn sidecar_name(algorithm: DigestAlgorithm) -> String {
    format!("{INVENTORY_FILENAME}.{}", algorithm.as_str())
}

/// Sidecar contents: `DIGEST inventory.json` (whitespace-separated; the newline
/// is ours — the spec shows the line form).
pub fn sidecar_contents(digest: &str) -> String {
    format!("{digest} {INVENTORY_FILENAME}\n")
}

/// Parse a sidecar's digest, accepting exactly the documented line shape.
pub fn parse_sidecar(text: &str) -> Option<String> {
    let mut parts = text.split_whitespace();
    let digest = parts.next()?;
    let name = parts.next()?;
    if name != INVENTORY_FILENAME {
        return None;
    }
    Some(digest.to_string())
}

/// OCFL path rules in one place — content paths (§3.5.2) and logical paths
/// (§3.5.3.1) share them: `/`-separated, no empty, `.` or `..` elements, and
/// no leading or trailing slash.
pub(crate) fn check_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("is empty".to_string());
    }
    if path.starts_with('/') || path.ends_with('/') {
        return Err("has a leading or trailing slash".to_string());
    }
    for element in path.split('/') {
        if element.is_empty() {
            return Err("has an empty path element (//)".to_string());
        }
        if element == "." || element == ".." {
            return Err("has a `.` or `..` path element".to_string());
        }
    }
    Ok(())
}

/// First pair of paths where one equals or is a path-segment ancestor of the
/// other — forbidden both for content paths and for a version's logical paths
/// ("unique and non-conflicting").
pub(crate) fn path_conflict<'a>(paths: impl Iterator<Item = &'a str>) -> Option<(String, String)> {
    let all: Vec<&str> = paths.collect();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for path in &all {
        if !seen.insert(path) {
            return Some(((*path).to_string(), (*path).to_string()));
        }
    }
    for path in &all {
        for (at, byte) in path.bytes().enumerate() {
            if byte == b'/' {
                let ancestor = &path[..at];
                if seen.contains(ancestor) {
                    return Some(((*path).to_string(), ancestor.to_string()));
                }
            }
        }
    }
    None
}

/// Structural problems found in an inventory. `errors` violate OCFL `MUST`
/// requirements; `warnings` are `SHOULD`-level deviations.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Problems {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Problems {
    /// True when no `MUST`-level violation was found (warnings allowed).
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.errors.push(message.into());
    }

    pub fn warning(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }
}

/// An OCFL object inventory (§3.5). Serialization writes exactly the keys the
/// spec describes — "MUST NOT contain any keys not described" — so semantics
/// like roles, choices and signatures live in *content files*, never here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    /// Object identifier — stable across every version, `SHOULD` be a URI.
    pub id: String,
    /// The spec URI; always [`INVENTORY_TYPE_1_1`] for objects this crate writes.
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "digestAlgorithm")]
    pub digest_algorithm: DigestAlgorithm,
    /// The newest version directory name (`vN` with the highest number).
    pub head: String,
    /// Content directory name within each version; `None` = `content`.
    #[serde(
        rename = "contentDirectory",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub content_directory: Option<String>,
    /// Content-addressed file index: digest → content paths (object-root
    /// relative). Cumulative — entries from every version stay.
    #[serde(default)]
    pub manifest: BTreeMap<String, Vec<String>>,
    /// Version directory name → version block. Serialized in map order; the
    /// spec gives entry order no significance.
    #[serde(default)]
    pub versions: BTreeMap<String, Version>,
}

/// One version's block (§3.5.3.1): when, who, why, and the logical state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    /// RFC 3339 timestamp with a timezone; second granularity minimum.
    pub created: String,
    /// Freeform rationale for the version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// The agent that created the version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    /// Logical state: digest → logical paths.
    pub state: BTreeMap<String, Vec<String>>,
}

/// The agent recorded on a version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub name: String,
    /// `SHOULD` be a URI (a `mailto:` or a personal identifier URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

impl Inventory {
    /// An empty inventory — no versions yet. Not valid until the first version
    /// commits (OCFL objects contain one or more versions, §3.3).
    pub fn new(id: impl Into<String>, digest_algorithm: DigestAlgorithm) -> Self {
        Self {
            id: id.into(),
            r#type: INVENTORY_TYPE_1_1.to_string(),
            digest_algorithm,
            head: String::new(),
            content_directory: None,
            manifest: BTreeMap::new(),
            versions: BTreeMap::new(),
        }
    }

    /// The designated content directory name.
    pub fn content_dir(&self) -> &str {
        self.content_directory
            .as_deref()
            .unwrap_or(DEFAULT_CONTENT_DIRECTORY)
    }

    /// Version directory names in ascending numeric order.
    pub fn version_dirs(&self) -> Vec<String> {
        let mut dirs: Vec<String> = self.versions.keys().cloned().collect();
        dirs.sort_by_key(|dir| version_number(dir).unwrap_or(u32::MAX));
        dirs
    }

    /// The newest version: its directory name and block.
    pub fn latest(&self) -> Option<(String, &Version)> {
        let dir = self.version_dirs().pop()?;
        let version = self.versions.get(&dir)?;
        Some((dir, version))
    }

    /// Resolve `version`'s logical state: logical path → content path, using
    /// the manifest's stored path for each digest. `None` when the version is
    /// unknown or its state references a digest the manifest lacks.
    pub fn resolve_state(&self, version: &str) -> Option<BTreeMap<String, String>> {
        let block = self.versions.get(version)?;
        let mut out = BTreeMap::new();
        for (digest, logicals) in &block.state {
            let content = self
                .manifest
                .get(digest)
                .or_else(|| {
                    self.manifest
                        .iter()
                        .find(|(key, _)| digest_eq(key, digest))
                        .map(|(_, paths)| paths)
                })?
                .first()?;
            for logical in logicals {
                out.insert(logical.clone(), content.clone());
            }
        }
        Some(out)
    }

    /// Validate the inventory against OCFL 1.1's structural rules.
    pub fn validate(&self) -> Problems {
        let mut p = Problems::default();

        if self.id.is_empty() {
            p.error("id is empty");
        }
        if self.r#type != INVENTORY_TYPE_1_1 {
            p.error(format!(
                "type is {:?}, expected {INVENTORY_TYPE_1_1:?}",
                self.r#type
            ));
        }
        if let Some(dir) = &self.content_directory
            && (dir.is_empty() || dir.contains('/') || dir == "." || dir == "..")
        {
            p.error(format!(
                "contentDirectory {dir:?} is not a single, safe path element"
            ));
        }

        // Version directories: names, numbering, and naming consistency.
        let dirs = self.version_dirs();
        if dirs.is_empty() {
            p.error("inventory has no versions");
        }
        let mut expected = 1u32;
        for dir in &dirs {
            match version_number(dir) {
                Some(number) => {
                    if number != expected {
                        p.error(format!(
                            "version {dir:?} breaks the contiguous 1.. sequence (expected v{expected})"
                        ));
                    }
                    expected = number + 1;
                }
                None => p.error(format!(
                    "{dir:?} is not a well-formed version directory name"
                )),
            }
        }
        // One naming convention across the object: plain (`v1`) or zero-padded
        // with a consistent width (`v0001`, `v0002`, …).
        let padding: BTreeSet<Option<usize>> = dirs
            .iter()
            .filter_map(|dir| {
                let number = version_number(dir)?;
                let plain_len = 1 + number.to_string().len();
                Some(if dir.len() > plain_len {
                    Some(dir.len())
                } else {
                    None
                })
            })
            .collect();
        if padding.len() > 1 {
            p.error("version directories mix padded and unpadded names");
        }

        // Head names the highest version.
        if self.head.is_empty() {
            p.error("head is empty");
        } else if !self.versions.contains_key(&self.head) {
            p.error(format!(
                "head {:?} is not a version directory in the inventory",
                self.head
            ));
        } else if dirs.last().map(String::as_str) != Some(self.head.as_str()) {
            p.error(format!("head {:?} is not the highest version", self.head));
        }

        // Manifest: well-formed digests (uniquely, case-insensitively), valid
        // paths under a version's content directory, unique and non-conflicting.
        let content_dir = self.content_dir();
        let known_dirs: BTreeSet<&str> = dirs.iter().map(String::as_str).collect();
        let mut lower_digests: BTreeSet<String> = BTreeSet::new();
        for digest in self.manifest.keys() {
            if !self.digest_algorithm.is_well_formed(digest) {
                p.error(format!(
                    "manifest digest {digest:?} is not a well-formed {} digest",
                    self.digest_algorithm
                ));
            }
            if !lower_digests.insert(digest.to_ascii_lowercase()) {
                p.error(format!(
                    "manifest digest {digest:?} appears more than once (case-insensitively)"
                ));
            }
        }
        let mut manifest_paths: Vec<&str> = Vec::new();
        for (digest, paths) in &self.manifest {
            if paths.is_empty() {
                p.error(format!("manifest digest {digest:?} has no content paths"));
            }
            for path in paths {
                if let Err(why) = check_path(path) {
                    p.error(format!("content path {path:?} {why}"));
                    continue;
                }
                let mut elements = path.split('/');
                let dir = elements.next().unwrap_or_default();
                let content = elements.next().unwrap_or_default();
                if !known_dirs.contains(dir) {
                    p.error(format!(
                        "content path {path:?} does not start with a version directory"
                    ));
                } else if content != content_dir {
                    p.error(format!(
                        "content path {path:?} does not sit under the content directory {content_dir:?}"
                    ));
                }
                manifest_paths.push(path);
            }
        }
        if let Some((a, b)) = path_conflict(manifest_paths.iter().copied()) {
            p.error(format!("content paths conflict: {a:?} and {b:?}"));
        }

        // Versions: state digests are in the manifest, logical paths obey the
        // path rules, and every manifest digest is used by some state.
        let manifest_lower: BTreeSet<String> = self
            .manifest
            .keys()
            .map(|k| k.to_ascii_lowercase())
            .collect();
        let mut used_lower: BTreeSet<String> = BTreeSet::new();
        for dir in &dirs {
            let version = &self.versions[dir];
            if !has_rfc3339_shape(&version.created) {
                p.warning(format!(
                    "{dir}: created {:?} is not RFC 3339-shaped",
                    version.created
                ));
            }
            let mut logical_paths: Vec<&str> = Vec::new();
            for (digest, paths) in &version.state {
                if !manifest_lower.contains(&digest.to_ascii_lowercase()) {
                    p.error(format!(
                        "{dir}: state digest {digest:?} has no manifest entry"
                    ));
                } else {
                    used_lower.insert(digest.to_ascii_lowercase());
                }
                if paths.is_empty() {
                    p.error(format!(
                        "{dir}: state digest {digest:?} has no logical paths"
                    ));
                }
                for path in paths {
                    if let Err(why) = check_path(path) {
                        p.error(format!("{dir}: logical path {path:?} {why}"));
                        continue;
                    }
                    logical_paths.push(path);
                }
            }
            if let Some((a, b)) = path_conflict(logical_paths.iter().copied()) {
                p.error(format!("{dir}: logical paths conflict: {a:?} and {b:?}"));
            }
        }
        for digest in self.manifest.keys() {
            if !used_lower.contains(&digest.to_ascii_lowercase()) {
                p.error(format!(
                    "manifest digest {digest:?} is not referenced by any version's state"
                ));
            }
        }

        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The spec's minimal object (§5.1), as JSON. The digest is a placeholder
    /// of the right shape (128 hex).
    const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\
         0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn the_spec_minimal_example_parses_and_validates() {
        let minimal = format!(
            r#"{{
          "digestAlgorithm": "sha512",
          "head": "v1",
          "id": "http://example.org/minimal",
          "manifest": {{ "{DIGEST}": [ "v1/content/file.txt" ] }},
          "type": "https://ocfl.io/1.1/spec/#inventory",
          "versions": {{
            "v1": {{
              "created": "2018-10-02T12:00:00Z",
              "message": "One file",
              "state": {{ "{DIGEST}": [ "file.txt" ] }},
              "user": {{ "address": "mailto:alice@example.org", "name": "Alice" }}
            }}
          }}
        }}"#
        );
        let inventory: Inventory = serde_json::from_str(&minimal).unwrap();
        let problems = inventory.validate();
        assert!(problems.is_ok(), "{:#?}", problems);
        assert_eq!(inventory.version_dirs(), ["v1"]);
        assert_eq!(inventory.content_dir(), "content");
        let state = inventory.resolve_state("v1").unwrap();
        assert_eq!(state["file.txt"], "v1/content/file.txt");
    }

    fn two_version_fixture() -> Inventory {
        let digest = |byte: char| byte.to_string().repeat(128);
        let mut inventory = Inventory::new("urn:test:0001", DigestAlgorithm::Sha512);
        let a = digest('a');
        let b = digest('b');
        inventory
            .manifest
            .insert(a.clone(), vec!["v1/content/a.txt".into()]);
        inventory
            .manifest
            .insert(b.clone(), vec!["v2/content/b.txt".into()]);
        inventory.versions.insert(
            "v1".into(),
            Version {
                created: "2026-01-01T00:00:00Z".into(),
                message: None,
                user: None,
                state: BTreeMap::from([(a.clone(), vec!["a.txt".into()])]),
            },
        );
        inventory.versions.insert(
            "v2".into(),
            Version {
                created: "2026-01-02T00:00:00Z".into(),
                message: None,
                user: None,
                state: BTreeMap::from([(a, vec!["a.txt".into()]), (b, vec!["b.txt".into()])]),
            },
        );
        inventory.head = "v2".into();
        inventory
    }

    #[test]
    fn a_consistent_two_version_inventory_validates() {
        let problems = two_version_fixture().validate();
        assert!(problems.is_ok(), "{:#?}", problems);
    }

    #[test]
    fn head_must_name_the_highest_version() {
        let mut inventory = two_version_fixture();
        inventory.head = "v1".into();
        let problems = inventory.validate();
        assert!(
            problems.errors.iter().any(|e| e.contains("highest")),
            "{problems:#?}"
        );
    }

    #[test]
    fn version_numbering_must_be_contiguous() {
        let mut inventory = two_version_fixture();
        let block = inventory.versions.remove("v2").unwrap();
        inventory.versions.insert("v3".into(), block);
        inventory.head = "v3".into();
        let problems = inventory.validate();
        assert!(
            problems.errors.iter().any(|e| e.contains("contiguous")),
            "{problems:#?}"
        );
    }

    #[test]
    fn state_digests_must_have_manifest_entries() {
        let mut inventory = two_version_fixture();
        let unknown = "f".repeat(128);
        inventory
            .versions
            .get_mut("v2")
            .unwrap()
            .state
            .insert(unknown, vec!["c.txt".into()]);
        let problems = inventory.validate();
        assert!(
            problems
                .errors
                .iter()
                .any(|e| e.contains("no manifest entry")),
            "{problems:#?}"
        );
    }

    #[test]
    fn manifest_digests_must_be_referenced_by_some_state() {
        let mut inventory = two_version_fixture();
        inventory
            .manifest
            .insert("d".repeat(128), vec!["v1/content/orphan.txt".into()]);
        let problems = inventory.validate();
        assert!(
            problems.errors.iter().any(|e| e.contains("not referenced")),
            "{problems:#?}"
        );
    }

    #[test]
    fn conflicting_content_paths_are_rejected() {
        let mut inventory = two_version_fixture();
        let c = "c".repeat(128);
        let path = inventory.manifest[&"a".repeat(128)][0].clone();
        inventory
            .manifest
            .insert(c.clone(), vec![format!("{path}/nested.txt")]);
        inventory
            .versions
            .get_mut("v1")
            .unwrap()
            .state
            .insert(c, vec!["nested.txt".into()]);
        let problems = inventory.validate();
        assert!(
            problems.errors.iter().any(|e| e.contains("conflict")),
            "{problems:#?}"
        );
    }

    #[test]
    fn sidecar_round_trips() {
        let digest = "a".repeat(128);
        let text = sidecar_contents(&digest);
        assert_eq!(parse_sidecar(&text), Some(digest));
        assert_eq!(parse_sidecar("garbage"), None);
        assert_eq!(
            sidecar_name(DigestAlgorithm::Sha512),
            "inventory.json.sha512"
        );
    }
}
