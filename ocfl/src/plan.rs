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

use crate::digest::DigestAlgorithm;
use crate::inventory::{
    Inventory, User, Version, check_path, path_conflict, version_dir, version_number,
};
use crate::pathmap::PathMapError;

/// The desired logical state for a version: logical path → file bytes.
pub type LogicalState = BTreeMap<String, Vec<u8>>;

#[derive(Debug, Error)]
pub enum PlanError {
    #[error("logical path {0:?} is not a valid OCFL path: {1}")]
    BadLogicalPath(String, String),
    #[error("logical paths conflict: {0:?} and {1:?}")]
    ConflictingLogicalPaths(String, String),
    #[error("content path mapping: {0}")]
    PathMap(#[from] PathMapError),
    #[error("mapped content path {0:?} is not a valid OCFL path: {1}")]
    BadContentPath(String, String),
    #[error("mapped content paths {0:?} and {1:?} conflict")]
    ConflictingContentPaths(String, String),
    #[error("mapped content path {0:?} would hold both {1:?} and {2:?}")]
    ContentPathCollision(String, String, String),
    #[error("fixity algorithm {0} is not one this client can compute")]
    UnsupportedFixityAlgorithm(String),
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

/// Plan the next version of `current` for `state` — the identity path mapping
/// (each logical path is its own content path).
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
    plan_version_with(
        current,
        state,
        &|logical| Ok(logical.to_string()),
        created,
        message,
        user,
    )
}

/// Plan the next version of `current` for `state`, mapping each logical path to
/// its content path with `map` — the identity above, or
/// `0011-direct-clean-path-layout`'s [`crate::DirectClean::clean`].
///
/// The mapping is consulted only for content that is actually written: a digest
/// already in the manifest keeps its stored path and is never re-mapped. A
/// mapping that would place two differently-digested files at one content path
/// (which `0011` permits when `encodeUTF` is off) is rejected — the spec
/// requires the generating software to raise in that case.
pub fn plan_version_with(
    current: &Inventory,
    state: &LogicalState,
    map: &dyn Fn(&str) -> Result<String, PathMapError>,
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
    // Mapped path → the (digest, logical path) that claimed it: collisions
    // between different content are an error, not an overwrite.
    let mut targets: BTreeMap<String, (String, String)> = BTreeMap::new();
    for (logical, bytes) in state {
        if let Err(why) = check_path(logical) {
            return Err(PlanError::BadLogicalPath(logical.clone(), why));
        }
        let digest = algorithm.digest(bytes);
        if stored.insert(digest.to_ascii_lowercase()) {
            let mapped = map(logical)?;
            if let Err(why) = check_path(&mapped) {
                return Err(PlanError::BadContentPath(mapped, why));
            }
            if let Some((other_digest, other_logical)) = targets.get(&mapped)
                && *other_digest != digest.to_ascii_lowercase()
            {
                return Err(PlanError::ContentPathCollision(
                    mapped,
                    other_logical.clone(),
                    logical.clone(),
                ));
            }
            targets.insert(
                mapped.clone(),
                (digest.to_ascii_lowercase(), logical.clone()),
            );
            let content_path = format!("{dir}/{content_dir}/{mapped}");
            inventory
                .manifest
                .insert(digest.clone(), vec![content_path.clone()]);
            writes.push((content_path, bytes.clone()));
        }
        state_map.entry(digest).or_default().push(logical.clone());
    }
    if let Some((a, b)) = path_conflict(targets.keys().map(String::as_str)) {
        return Err(PlanError::ConflictingContentPaths(a, b));
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

/// Record fixity values for the files a version writes — the `fixity` block
/// (§3.5.4): algorithm → digest → content paths. Existing entries stay; the
/// spec neither requires nor forbids carrying fixity forward, and extending in
/// place is deterministic. `size` is a valid fixity "algorithm" (0009).
pub fn record_fixity(
    inventory: &mut Inventory,
    writes: &[(String, Vec<u8>)],
    algorithms: &[DigestAlgorithm],
) -> Result<(), PlanError> {
    for algorithm in algorithms {
        for (content_path, bytes) in writes {
            let digest = algorithm
                .digest(bytes)
                .ok_or_else(|| PlanError::UnsupportedFixityAlgorithm(algorithm.to_string()))?;
            let block = inventory.fixity.entry(algorithm.clone()).or_default();
            let paths = block.entry(digest).or_default();
            if !paths.contains(content_path) {
                paths.push(content_path.clone());
            }
        }
    }
    for block in inventory.fixity.values_mut() {
        for paths in block.values_mut() {
            paths.sort();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digest::{ContentAlgorithm, DigestAlgorithm};

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

    #[test]
    fn a_path_map_derives_content_paths_from_logical_ones() {
        let mapper = crate::DirectClean::new(crate::DirectCleanConfig::default()).unwrap();
        let inventory = Inventory::new("urn:test:0001", algorithm());
        let plan = plan_version_with(
            &inventory,
            &state(&[("a:b.txt", b"1"), ("plain.txt", b"2")]),
            &|logical| mapper.clean(logical),
            "2026-10-11T00:00:00Z",
            None,
            None,
        )
        .unwrap();
        let writes: Vec<&str> = plan.writes.iter().map(|(path, _)| path.as_str()).collect();
        assert!(writes.contains(&"v1/content/a_b.txt"), "{writes:?}");
        assert!(writes.contains(&"v1/content/plain.txt"), "{writes:?}");
        let resolved = plan.inventory.resolve_state("v1").unwrap();
        assert_eq!(resolved["a:b.txt"], "v1/content/a_b.txt");
        assert!(plan.inventory.validate().is_ok());
    }

    #[test]
    fn a_non_injective_map_that_collides_is_rejected() {
        let mapper = crate::DirectClean::new(crate::DirectCleanConfig::default()).unwrap();
        let inventory = Inventory::new("urn:test:0001", algorithm());
        let error = plan_version_with(
            &inventory,
            &state(&[("a:b", b"one"), ("a_b", b"two")]),
            &|logical| mapper.clean(logical),
            "2026-10-11T00:00:00Z",
            None,
            None,
        )
        .unwrap_err();
        assert!(
            matches!(error, PlanError::ContentPathCollision(_, _, _)),
            "{error:?}"
        );
        // The same map with identical content is fine: one write, two logicals.
        let plan = plan_version_with(
            &inventory,
            &state(&[("a:b", b"same"), ("a_b", b"same")]),
            &|logical| mapper.clean(logical),
            "2026-10-11T00:00:00Z",
            None,
            None,
        )
        .unwrap();
        assert_eq!(plan.writes.len(), 1);
        let resolved = plan.inventory.resolve_state("v1").unwrap();
        assert_eq!(resolved["a:b"], resolved["a_b"]);
    }

    #[test]
    fn fixity_values_are_recorded_for_written_content() {
        let inventory = Inventory::new("urn:test:0001", algorithm());
        let mut plan = plan_version(
            &inventory,
            &state(&[("a.txt", b"alpha")]),
            "2026-10-11T00:00:00Z",
            None,
            None,
        )
        .unwrap();
        record_fixity(
            &mut plan.inventory,
            &plan.writes,
            &[DigestAlgorithm::Md5, DigestAlgorithm::Sha1],
        )
        .unwrap();
        let digest = DigestAlgorithm::Md5.digest(b"alpha").unwrap();
        assert_eq!(
            plan.inventory.fixity[&DigestAlgorithm::Md5][&digest],
            ["v1/content/a.txt"]
        );
        assert_eq!(plan.inventory.fixity.len(), 2);
        assert!(plan.inventory.validate().is_ok());

        let error = record_fixity(
            &mut plan.inventory,
            &plan.writes,
            &[DigestAlgorithm::Other("xxh3-128".into())],
        )
        .unwrap_err();
        assert!(
            matches!(error, PlanError::UnsupportedFixityAlgorithm(_)),
            "{error:?}"
        );
    }
}
