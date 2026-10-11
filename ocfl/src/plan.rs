//! Forward-delta planning: what a new version stores versus what it references.
//!
//! This is the pure half of versioning — no filesystem, no clock. The host
//! hands in the desired logical state and a timestamp; the plan says which
//! files to write (only digests not already in the manifest), where, and what
//! the inventory looks like after committing. The `fs` module performs the
//! generic writes; a Worker or any other backend can do the same with its own
//! storage and this same plan.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::inventory::{
    Inventory, User, Version, check_path, path_conflict, version_dir, version_number,
};

/// The desired logical state for a version: logical path → file bytes.
pub type LogicalState = BTreeMap<String, Vec<u8>>;

#[derive(Debug, Error)]
pub enum PlanError {
    #[error("logical path {0:?} is not a valid OCFL path: {1}")]
    BadLogicalPath(String, String),
    #[error("logical paths conflict: {0:?} and {1:?}")]
    ConflictingLogicalPaths(String, String),
}

/// A version ready to write: its new content files and the inventory as it
/// stands after the version commits.
#[derive(Debug)]
pub struct VersionPlan {
    /// Files to write, object-relative content path → bytes. Empty when every
    /// file's digest is already stored by an earlier version.
    pub writes: Vec<(String, Vec<u8>)>,
    /// The version directory name the writes belong under (`vN`).
    pub version_dir: String,
    /// The inventory including this version — the bytes to place at both the
    /// version directory and the object root.
    pub inventory: Inventory,
}

/// Plan the next version of `current` for `state`.
///
/// Forward deltas: a file whose digest is already in the manifest keeps its
/// stored content path (per-file dedup, stored once, referenced by every
/// version that shares it); only new digests produce writes, into this
/// version's content directory. The manifest is cumulative — digests that only
/// earlier versions reference stay listed.
pub fn plan_version(
    current: &Inventory,
    state: &LogicalState,
    created: impl Into<String>,
    message: Option<String>,
    user: Option<User>,
) -> Result<VersionPlan, PlanError> {
    let mut inventory = current.clone();
    let algorithm = inventory.digest_algorithm;
    if let Some((a, b)) = path_conflict(state.keys().map(String::as_str)) {
        return Err(PlanError::ConflictingLogicalPaths(a, b));
    }
    let number = inventory
        .versions
        .keys()
        .filter_map(|dir| version_number(dir))
        .max()
        .unwrap_or(0)
        + 1;
    let dir = version_dir(number);
    let content_dir = inventory.content_dir().to_string();

    let mut stored: BTreeSet<String> = inventory
        .manifest
        .keys()
        .map(|digest| digest.to_ascii_lowercase())
        .collect();
    let mut writes = Vec::new();
    let mut state_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (logical, bytes) in state {
        if let Err(why) = check_path(logical) {
            return Err(PlanError::BadLogicalPath(logical.clone(), why));
        }
        let digest = algorithm.digest(bytes);
        if stored.insert(digest.to_ascii_lowercase()) {
            let content_path = format!("{dir}/{content_dir}/{logical}");
            inventory
                .manifest
                .insert(digest.clone(), vec![content_path.clone()]);
            writes.push((content_path, bytes.clone()));
        }
        state_map.entry(digest).or_default().push(logical.clone());
    }
    for paths in state_map.values_mut() {
        paths.sort();
    }

    let version = Version {
        created: created.into(),
        message,
        user,
        state: state_map,
    };
    inventory.head = dir.clone();
    inventory.versions.insert(dir.clone(), version);
    Ok(VersionPlan {
        writes,
        version_dir: dir,
        inventory,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digest::ContentAlgorithm;

    fn state(files: &[(&str, &[u8])]) -> LogicalState {
        files
            .iter()
            .map(|(path, bytes)| ((*path).to_string(), bytes.to_vec()))
            .collect()
    }

    fn algorithm() -> ContentAlgorithm {
        ContentAlgorithm::Sha512
    }

    #[test]
    fn the_first_version_writes_everything() {
        let inventory = Inventory::new("urn:test:0001", algorithm());
        let plan = plan_version(
            &inventory,
            &state(&[("0001.webp", b"still"), ("pack.json", b"{}")]),
            "2026-10-11T00:00:00Z",
            Some("Minted".into()),
            None,
        )
        .unwrap();
        assert_eq!(plan.version_dir, "v1");
        assert_eq!(plan.writes.len(), 2);
        assert_eq!(plan.inventory.head, "v1");
        let dirs: Vec<&str> = plan
            .inventory
            .manifest
            .values()
            .flatten()
            .map(String::as_str)
            .collect();
        assert!(dirs.contains(&"v1/content/0001.webp"), "{dirs:?}");
        assert!(plan.inventory.validate().is_ok());
    }

    #[test]
    fn an_unchanged_file_is_referenced_not_copied() {
        let inventory = Inventory::new("urn:test:0001", algorithm());
        let first = plan_version(
            &inventory,
            &state(&[("a.txt", b"same"), ("b.txt", b"old")]),
            "2026-10-11T00:00:00Z",
            None,
            None,
        )
        .unwrap();
        let second = plan_version(
            &first.inventory,
            &state(&[("a.txt", b"same"), ("b.txt", b"new")]),
            "2026-10-12T00:00:00Z",
            None,
            None,
        )
        .unwrap();
        assert_eq!(second.version_dir, "v2");
        // Only the changed file is written, under v2.
        assert_eq!(second.writes.len(), 1);
        assert_eq!(second.writes[0].0, "v2/content/b.txt");
        // The unchanged file still resolves to its v1 content path.
        let resolved = second.inventory.resolve_state("v2").unwrap();
        assert_eq!(resolved["a.txt"], "v1/content/a.txt");
        assert_eq!(resolved["b.txt"], "v2/content/b.txt");
        // The manifest is cumulative: both digests remain.
        assert_eq!(second.inventory.manifest.len(), 3);
        assert!(second.inventory.validate().is_ok());
    }

    #[test]
    fn two_logical_paths_with_one_content_get_one_write() {
        let inventory = Inventory::new("urn:test:0001", algorithm());
        let plan = plan_version(
            &inventory,
            &state(&[("a.txt", b"same"), ("deep/b.txt", b"same")]),
            "2026-10-11T00:00:00Z",
            None,
            None,
        )
        .unwrap();
        assert_eq!(plan.writes.len(), 1);
        let resolved = plan.inventory.resolve_state("v1").unwrap();
        assert_eq!(resolved["a.txt"], resolved["deep/b.txt"]);
    }

    #[test]
    fn conflicting_logical_paths_are_rejected() {
        let inventory = Inventory::new("urn:test:0001", algorithm());
        let error = plan_version(
            &inventory,
            &state(&[("a", b"1"), ("a/b", b"2")]),
            "2026-10-11T00:00:00Z",
            None,
            None,
        )
        .unwrap_err();
        assert!(
            matches!(error, PlanError::ConflictingLogicalPaths(_, _)),
            "{error:?}"
        );
    }

    #[test]
    fn planning_is_deterministic() {
        let inventory = Inventory::new("urn:test:0001", algorithm());
        let files = state(&[("b.txt", b"2"), ("a.txt", b"1")]);
        let one = plan_version(&inventory, &files, "2026-10-11T00:00:00Z", None, None).unwrap();
        let two = plan_version(&inventory, &files, "2026-10-11T00:00:00Z", None, None).unwrap();
        assert_eq!(
            serde_json::to_string_pretty(&one.inventory).unwrap(),
            serde_json::to_string_pretty(&two.inventory).unwrap()
        );
    }
}
