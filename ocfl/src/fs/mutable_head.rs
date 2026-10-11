//! `0005-mutable-head`: a staged version under
//! `[object-root]/extensions/0005-mutable-head/head` that can be revised
//! in-place until it is committed to an immutable version.
//!
//! The mutable HEAD is a full OCFL version — the same inventory shape, the
//! same content addressing — kept inside the extension directory instead of
//! the object root. Its version number is the root's `head` number plus one
//! and never changes while it lives. Clients that implement the extension
//! treat its inventory as the object's current state, which [`Object::open`]
//! does; an object with an active mutable HEAD cannot receive new root
//! versions ([`ObjectWriter::commit`] refuses).
//!
//! Every mutation is a *revision* (`r1`, `r2`, …), each announced by an
//! atomically-created marker file before it touches anything — a marker that
//! already exists means a concurrent writer won and this one aborts. Content
//! added by a revision lives under `head/content/rN/`, and content the
//! inventory stops referencing is deleted when the revision lands.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::digest::digest_eq;
use crate::inventory::{
    INVENTORY_FILENAME, Inventory, InventoryContext, MUTABLE_HEAD_EXTENSION, User, Version,
    check_path, parse_sidecar, path_conflict, sidecar_name, version_dir, version_number,
};
use crate::plan::{LogicalState, PlanError};

use super::{
    FsError, ObjectWriter, VerifyReport, io, read_inventory_at, walk_content, write_inventory_pair,
};

/// The extension sub-directory holding the staged version.
const HEAD_DIR: &str = "head";
/// The extension sub-directory holding revision markers.
const REVISIONS_DIR: &str = "revisions";

/// Summary of an object's active mutable HEAD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutableHeadInfo {
    /// The version directory the mutable HEAD becomes when committed (`vN`).
    pub version: String,
    /// The newest applied revision (`rN`).
    pub latest_revision: u32,
}

fn extension_dir(object_root: &Path) -> PathBuf {
    object_root.join("extensions").join(MUTABLE_HEAD_EXTENSION)
}

fn head_dir(object_root: &Path) -> PathBuf {
    extension_dir(object_root).join(HEAD_DIR)
}

fn revision_number(name: &str) -> Option<u32> {
    let digits = name.strip_prefix('r')?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// A content path inside the mutable HEAD's own content tree.
pub(super) fn is_mutable_head_path(path: &str) -> bool {
    path.starts_with(&format!("extensions/{MUTABLE_HEAD_EXTENSION}/{HEAD_DIR}/"))
}

/// Claim revision `revision` by creating its marker — atomically, so an
/// existing marker means a concurrent update got there first.
fn claim_marker(revisions: &Path, revision: u32) -> Result<(), FsError> {
    std::fs::create_dir_all(revisions).map_err(|error| io(revisions, error))?;
    let path = revisions.join(format!("r{revision}"));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                FsError::Other(format!(
                    "revision r{revision} already exists — a concurrent update won; this one was aborted"
                ))
            } else {
                io(&path, error)
            }
        })?;
    file.write_all(format!("r{revision}").as_bytes())
        .map_err(|error| io(&path, error))
}

/// What a revision's state needs written: the files to create (content path →
/// bytes) and the version `state` block itself.
type PreparedState = (Vec<(String, Vec<u8>)>, BTreeMap<String, Vec<String>>);

/// Add `state` to `inventory`: return the files that need writing under
/// `content_prefix`, and the version `state` block. Digests already in the
/// manifest are referenced, never rewritten.
fn apply_state(
    inventory: &mut Inventory,
    state: &LogicalState,
    content_prefix: &str,
) -> Result<PreparedState, PlanError> {
    if let Some((a, b)) = path_conflict(state.keys().map(String::as_str)) {
        return Err(PlanError::ConflictingLogicalPaths(a, b));
    }
    let algorithm = inventory.digest_algorithm;
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
            let content_path = format!("{content_prefix}/{logical}");
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
    Ok((writes, state_map))
}

/// Drop manifest and fixity paths under the extension directory whose digest
/// the new state no longer references — a mutable HEAD inventory MUST NOT
/// keep them (§0005), while paths into committed versions always stay.
fn prune_stale_paths(inventory: &mut Inventory, state: &BTreeMap<String, Vec<String>>) {
    let referenced: BTreeSet<String> = state
        .keys()
        .map(|digest| digest.to_ascii_lowercase())
        .collect();
    for (digest, paths) in inventory.manifest.iter_mut() {
        let used = referenced.contains(&digest.to_ascii_lowercase());
        paths.retain(|path| !is_mutable_head_path(path) || used);
    }
    inventory.manifest.retain(|_, paths| !paths.is_empty());
    for digests in inventory.fixity.values_mut() {
        for (digest, paths) in digests.iter_mut() {
            let used = referenced.contains(&digest.to_ascii_lowercase());
            paths.retain(|path| !is_mutable_head_path(path) || used);
        }
        digests.retain(|_, paths| !paths.is_empty());
    }
}

fn write_files(root: &Path, writes: &[(String, Vec<u8>)]) -> Result<(), FsError> {
    for (content_path, bytes) in writes {
        let path = root.join(content_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| io(parent, error))?;
        }
        std::fs::write(&path, bytes).map_err(|error| io(&path, error))?;
    }
    Ok(())
}

/// Delete files under the mutable HEAD content directory that the inventory no
/// longer references, then any directories they leave empty — the final step
/// of a revision (§0005).
fn prune_unreferenced(object_root: &Path, inventory: &Inventory) -> Result<(), FsError> {
    let content = head_dir(object_root).join(inventory.content_dir());
    if !content.is_dir() {
        return Ok(());
    }
    let referenced: BTreeSet<&str> = inventory
        .manifest
        .values()
        .flatten()
        .map(String::as_str)
        .collect();
    let walk = walk_content(&content);
    for relative in &walk.files {
        let content_path = format!(
            "extensions/{MUTABLE_HEAD_EXTENSION}/{HEAD_DIR}/{}/{relative}",
            inventory.content_dir()
        );
        if !referenced.contains(content_path.as_str()) {
            let path = content.join(relative);
            std::fs::remove_file(&path).map_err(|error| io(&path, error))?;
        }
    }
    prune_empty_dirs(&content)
}

/// `remove_dir` only succeeds on an empty directory, so this can never delete
/// content.
fn prune_empty_dirs(dir: &Path) -> Result<(), FsError> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            prune_empty_dirs(&entry.path())?;
            let _ = std::fs::remove_dir(entry.path());
        }
    }
    Ok(())
}

/// Rewrite `extensions/0005-mutable-head/head/` content paths to `vN/` — the
/// commit step of §0005, applied to the manifest and every fixity block.
fn rewrite_mutable_paths(inventory: &mut Inventory, version: &str) {
    let from = format!("extensions/{MUTABLE_HEAD_EXTENSION}/{HEAD_DIR}/");
    let rewrite = |path: &mut String| {
        if let Some(rest) = path.strip_prefix(from.as_str()) {
            *path = format!("{version}/{rest}");
        }
    };
    for paths in inventory.manifest.values_mut() {
        for path in paths {
            rewrite(path);
        }
    }
    for digests in inventory.fixity.values_mut() {
        for paths in digests.values_mut() {
            for path in paths {
                rewrite(path);
            }
        }
    }
}

/// Build and write a claimed mutable HEAD: version `root head + 1`, revision
/// `r1` holding `state`.
fn create_claimed(
    root: &Path,
    extension: &Path,
    inventory: Inventory,
    state: &LogicalState,
    created: String,
    message: Option<String>,
    user: Option<User>,
) -> Result<MutableHead, FsError> {
    let mut inventory = inventory;
    let number = inventory
        .versions
        .keys()
        .filter_map(|dir| version_number(dir))
        .max()
        .unwrap_or(0)
        + 1;
    let version = version_dir(number);
    let content_prefix = format!("extensions/{MUTABLE_HEAD_EXTENSION}/{HEAD_DIR}/content/r1");
    let (writes, state_map) = apply_state(&mut inventory, state, &content_prefix)?;
    inventory.versions.insert(
        version.clone(),
        Version {
            created,
            message,
            user,
            state: state_map,
        },
    );
    inventory.head = version.clone();

    write_files(root, &writes)?;
    let head = head_dir(root);
    let bytes = serde_json::to_vec_pretty(&inventory).map_err(|source| FsError::Parse {
        path: head.join(INVENTORY_FILENAME),
        source,
    })?;
    write_inventory_pair(&head, inventory.digest_algorithm, &bytes)?;

    // The root sidecar's copy at creation time — the evidence a later commit
    // checks the root against.
    let source = root.join(sidecar_name(inventory.digest_algorithm));
    let copy = extension.join(format!(
        "root-inventory.json.{}",
        inventory.digest_algorithm.as_str()
    ));
    let contents = std::fs::read(&source).map_err(|error| io(&source, error))?;
    std::fs::write(&copy, contents).map_err(|error| io(&copy, error))?;

    Ok(MutableHead {
        object_root: root.to_path_buf(),
        inventory,
        version,
        latest_revision: 1,
    })
}

impl ObjectWriter {
    /// Create a mutable HEAD (`0005-mutable-head`): version `head + 1`,
    /// staged under the extension directory, with `state` as its first
    /// revision. The object must already have a committed version — for a
    /// brand-new object, commit an empty `v1` first, as the extension
    /// requires. Further changes go through [`MutableHead::revise`].
    pub fn create_mutable_head(
        &mut self,
        state: &LogicalState,
        created: impl Into<String>,
        message: Option<String>,
        user: Option<User>,
    ) -> Result<MutableHead, FsError> {
        if MutableHead::exists(&self.root) {
            return Err(FsError::Other(
                "the object already has an active mutable HEAD — revise, commit or purge it first"
                    .into(),
            ));
        }
        if self.inventory.versions.is_empty() {
            return Err(FsError::Other(
                "a mutable HEAD needs an existing version — commit an empty v1 first".into(),
            ));
        }
        let root = self.root.clone();
        let extension = extension_dir(&root);
        let revisions = extension.join(REVISIONS_DIR);
        std::fs::create_dir_all(&revisions).map_err(|error| io(&revisions, error))?;
        if let Err(error) = claim_marker(&revisions, 1) {
            // Leave nothing of ours behind. `remove_dir` only succeeds on an
            // empty directory, so a concurrent writer's tree is never touched.
            let _ = std::fs::remove_dir(&revisions);
            let _ = std::fs::remove_dir(&extension);
            return Err(error);
        }
        match create_claimed(
            &root,
            &extension,
            self.inventory.clone(),
            state,
            created.into(),
            message,
            user,
        ) {
            Ok(head) => Ok(head),
            Err(error) => {
                // The marker was ours (the claim succeeded): take the whole
                // extension directory back out.
                let _ = std::fs::remove_dir_all(&extension);
                Err(error)
            }
        }
    }
}

/// An object's active mutable HEAD.
pub struct MutableHead {
    object_root: PathBuf,
    inventory: Inventory,
    version: String,
    latest_revision: u32,
}

impl MutableHead {
    /// Is a mutable HEAD active under `object_root`? Its inventory at
    /// `extensions/0005-mutable-head/head/inventory.json` is the marker.
    pub fn exists(object_root: &Path) -> bool {
        head_dir(object_root).join(INVENTORY_FILENAME).is_file()
    }

    /// Open the active mutable HEAD: parse and validate its inventory, and
    /// find the newest revision.
    pub fn open(object_root: impl Into<PathBuf>) -> Result<Self, FsError> {
        let object_root = object_root.into();
        let head = head_dir(&object_root);
        let (inventory, _) = read_inventory_at(&head)?;
        let problems = inventory.validate_in(InventoryContext::MutableHead);
        if !problems.is_ok() {
            return Err(FsError::Invalid(problems.errors.join("\n  ")));
        }
        let version = inventory.head.clone();
        if version_number(&version).is_none() {
            return Err(FsError::Other(format!(
                "mutable HEAD: head {version:?} is not a version directory name"
            )));
        }
        let latest_revision = latest_revision(&object_root)?
            .ok_or_else(|| FsError::Other("mutable HEAD: no revision marker (r1) exists".into()))?;
        Ok(Self {
            object_root,
            inventory,
            version,
            latest_revision,
        })
    }

    pub fn inventory(&self) -> &Inventory {
        &self.inventory
    }

    /// The version directory this mutable HEAD becomes when committed (`vN`).
    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn latest_revision(&self) -> u32 {
        self.latest_revision
    }

    pub fn info(&self) -> MutableHeadInfo {
        MutableHeadInfo {
            version: self.version.clone(),
            latest_revision: self.latest_revision,
        }
    }

    /// Has the root object been modified since this mutable HEAD was created?
    /// (Compares the sidecar copy taken at creation with the live one.)
    pub fn conflict(&self) -> Result<bool, FsError> {
        let algorithm = self.inventory.digest_algorithm;
        let stored_path = extension_dir(&self.object_root)
            .join(format!("root-inventory.json.{}", algorithm.as_str()));
        let stored =
            std::fs::read_to_string(&stored_path).map_err(|error| io(&stored_path, error))?;
        let current_path = self.object_root.join(sidecar_name(algorithm));
        let current =
            std::fs::read_to_string(&current_path).map_err(|error| io(&current_path, error))?;
        Ok(match (parse_sidecar(&stored), parse_sidecar(&current)) {
            (Some(a), Some(b)) => !digest_eq(&a, &b),
            _ => stored != current,
        })
    }

    /// Apply the next revision: `state` is the full desired logical state of
    /// the mutable HEAD version. Returns the revision number (`rN`).
    pub fn revise(
        &mut self,
        state: &LogicalState,
        created: impl Into<String>,
        message: Option<String>,
        user: Option<User>,
    ) -> Result<u32, FsError> {
        let revision = self.latest_revision + 1;
        let revisions = extension_dir(&self.object_root).join(REVISIONS_DIR);
        claim_marker(&revisions, revision)?;
        match self.apply_revision(revision, state, created.into(), message, user) {
            Ok(()) => {
                self.latest_revision = revision;
                Ok(revision)
            }
            Err(error) => {
                // Undo the claim and this revision's content; earlier
                // revisions are untouched.
                let _ = std::fs::remove_file(revisions.join(format!("r{revision}")));
                let _ = std::fs::remove_dir_all(
                    head_dir(&self.object_root)
                        .join(self.inventory.content_dir())
                        .join(format!("r{revision}")),
                );
                Err(error)
            }
        }
    }

    fn apply_revision(
        &mut self,
        revision: u32,
        state: &LogicalState,
        created: String,
        message: Option<String>,
        user: Option<User>,
    ) -> Result<(), FsError> {
        let content_prefix =
            format!("extensions/{MUTABLE_HEAD_EXTENSION}/{HEAD_DIR}/content/r{revision}");
        let (writes, state_map) = apply_state(&mut self.inventory, state, &content_prefix)?;
        prune_stale_paths(&mut self.inventory, &state_map);
        self.inventory.versions.insert(
            self.version.clone(),
            Version {
                created,
                message,
                user,
                state: state_map,
            },
        );
        self.inventory.head = self.version.clone();

        write_files(&self.object_root, &writes)?;
        let head = head_dir(&self.object_root);
        let bytes =
            serde_json::to_vec_pretty(&self.inventory).map_err(|source| FsError::Parse {
                path: head.join(INVENTORY_FILENAME),
                source,
            })?;
        write_inventory_pair(&head, self.inventory.digest_algorithm, &bytes)?;
        prune_unreferenced(&self.object_root, &self.inventory)
    }

    /// Commit the mutable HEAD to the object root as an immutable version,
    /// returning its directory name. Refuses on a version conflict (the root
    /// changed since creation) or if the version directory already exists.
    pub fn commit(self) -> Result<String, FsError> {
        if self.conflict()? {
            return Err(FsError::Other(
                "version conflict: the root object has changed since the mutable HEAD was created — purge it or resolve the conflict"
                    .into(),
            ));
        }
        let object_root = &self.object_root;
        let version = self.version.clone();
        let target = object_root.join(&version);
        if target.exists() {
            return Err(FsError::Other(format!(
                "version conflict: {} already exists",
                target.display()
            )));
        }
        let mut inventory = self.inventory.clone();
        rewrite_mutable_paths(&mut inventory, &version);
        let problems = inventory.validate();
        if !problems.is_ok() {
            return Err(FsError::Invalid(problems.errors.join("\n  ")));
        }

        let head = head_dir(object_root);
        std::fs::rename(&head, &target).map_err(|error| io(&head, error))?;
        let bytes = serde_json::to_vec_pretty(&inventory).map_err(|source| FsError::Parse {
            path: target.join(INVENTORY_FILENAME),
            source,
        })?;
        if let Err(error) = write_inventory_pair(object_root, inventory.digest_algorithm, &bytes) {
            // Put the version back where it was and leave the root untouched.
            let _ = std::fs::rename(&target, &head);
            return Err(error);
        }
        write_inventory_pair(&target, inventory.digest_algorithm, &bytes)?;
        let extension = extension_dir(object_root);
        std::fs::remove_dir_all(&extension).map_err(|error| io(&extension, error))?;
        Ok(version)
    }

    /// Delete the extension directory, discarding the mutable HEAD (§0005's
    /// purge — also how a version conflict is abandoned).
    pub fn purge(self) -> Result<(), FsError> {
        let extension = extension_dir(&self.object_root);
        std::fs::remove_dir_all(&extension).map_err(|error| io(&extension, error))
    }
}

fn latest_revision(object_root: &Path) -> Result<Option<u32>, FsError> {
    let dir = extension_dir(object_root).join(REVISIONS_DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(&dir, error)),
    };
    let mut latest = None;
    for entry in entries.flatten() {
        if let Some(number) = revision_number(&entry.file_name().to_string_lossy()) {
            latest = Some(latest.map_or(number, |current: u32| current.max(number)));
        }
    }
    Ok(latest)
}

/// Verify an active mutable HEAD against §0005 and the root inventory.
pub(super) fn verify(object_root: &Path, root_inventory: &Inventory, report: &mut VerifyReport) {
    let extension = extension_dir(object_root);
    // The extension directory MUST contain exactly three children.
    match std::fs::read_dir(&extension) {
        Ok(entries) => {
            let copy_name = format!(
                "root-inventory.json.{}",
                root_inventory.digest_algorithm.as_str()
            );
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name != copy_name && name != REVISIONS_DIR && name != HEAD_DIR {
                    report.error(format!(
                        "mutable HEAD: unexpected entry {name:?} in the extension directory"
                    ));
                }
            }
        }
        Err(error) => {
            report.error(format!(
                "mutable HEAD: {} unreadable: {error}",
                extension.display()
            ));
            return;
        }
    }

    // The root must not have changed since the mutable HEAD was created.
    let copy = extension.join(format!(
        "root-inventory.json.{}",
        root_inventory.digest_algorithm.as_str()
    ));
    let stored = std::fs::read_to_string(&copy);
    let current =
        std::fs::read_to_string(object_root.join(sidecar_name(root_inventory.digest_algorithm)));
    match (stored, current) {
        (Ok(stored), Ok(current)) => {
            let same = match (parse_sidecar(&stored), parse_sidecar(&current)) {
                (Some(a), Some(b)) => digest_eq(&a, &b),
                _ => stored == current,
            };
            if !same {
                report.error(
                    "mutable HEAD: version conflict — the root inventory has changed since the mutable HEAD was created",
                );
            }
        }
        (Err(error), _) => report.error(format!("mutable HEAD: {}: {error}", copy.display())),
        (_, Err(error)) => report.error(format!("mutable HEAD: root inventory sidecar: {error}")),
    }

    // Revision markers: r1..rN contiguous, each containing just its own name.
    let revisions = extension.join(REVISIONS_DIR);
    let mut numbers: BTreeSet<u32> = BTreeSet::new();
    match std::fs::read_dir(&revisions) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                match revision_number(&name) {
                    Some(number) => {
                        numbers.insert(number);
                        match std::fs::read_to_string(entry.path()) {
                            Ok(contents) if contents == name => {}
                            Ok(contents) => report.error(format!(
                                "mutable HEAD: revision marker {name:?} contains {contents:?}, expected {name:?}"
                            )),
                            Err(error) => report.error(format!(
                                "mutable HEAD: revision marker {name:?} unreadable: {error}"
                            )),
                        }
                    }
                    None => report.error(format!(
                        "mutable HEAD: unexpected entry {name:?} in the revisions directory"
                    )),
                }
            }
        }
        Err(error) => report.error(format!(
            "mutable HEAD: {} unreadable: {error}",
            revisions.display()
        )),
    }
    if numbers.is_empty() {
        report.error("mutable HEAD: no revision markers — r1 must exist while a HEAD is active");
    } else {
        let highest = numbers.iter().next_back().copied().unwrap_or(0);
        if numbers.iter().copied().collect::<Vec<u32>>() != (1..=highest).collect::<Vec<u32>>() {
            report.error(format!(
                "mutable HEAD: revision numbers must be contiguous from r1; found {numbers:?}"
            ));
        }
    }

    // The head inventory itself.
    let head = head_dir(object_root);
    let (inventory, _) = match read_inventory_at(&head) {
        Ok(loaded) => loaded,
        Err(error) => {
            report.error(format!("mutable HEAD: {error}"));
            return;
        }
    };
    let problems = inventory.validate_in(InventoryContext::MutableHead);
    for message in problems.errors {
        report.error(format!("mutable HEAD inventory: {message}"));
    }
    for message in problems.warnings {
        report.warning(format!("mutable HEAD inventory: {message}"));
    }

    // Version relationship: one above the root's head, sharing every root
    // version unchanged.
    match (
        version_number(&inventory.head),
        version_number(&root_inventory.head),
    ) {
        (Some(mutable), Some(root)) if mutable == root + 1 => {}
        _ => report.error(format!(
            "mutable HEAD: version {:?} must be one greater than the root's {:?}",
            inventory.head, root_inventory.head
        )),
    }
    for (dir, block) in &root_inventory.versions {
        match inventory.versions.get(dir) {
            Some(head_block) if head_block.state == block.state => {}
            Some(_) => report.error(format!(
                "mutable HEAD: version {dir:?}'s state differs from the root inventory"
            )),
            None => report.error(format!(
                "mutable HEAD: version {dir:?} is absent from the mutable HEAD inventory"
            )),
        }
    }

    // Manifest entries pointing into the extension directory must be
    // referenced by the mutable version's state, and their files must exist.
    let head_state_lower: BTreeSet<String> = inventory
        .versions
        .get(&inventory.head)
        .map(|version| {
            version
                .state
                .keys()
                .map(|digest| digest.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default();
    for (digest, paths) in &inventory.manifest {
        if paths.iter().any(|path| is_mutable_head_path(path))
            && !head_state_lower.contains(&digest.to_ascii_lowercase())
        {
            report.error(format!(
                "mutable HEAD: manifest digest {digest:?} references extension content but is not in the mutable version's state"
            ));
        }
        for path in paths {
            if is_mutable_head_path(path) && !object_root.join(path).is_file() {
                report.error(format!("mutable HEAD: {path}: missing or unreadable"));
            }
        }
    }

    // Content: every file referenced, and under a revision directory.
    let content = head.join(inventory.content_dir());
    if content.is_dir() {
        let referenced: BTreeSet<String> = inventory.manifest.values().flatten().cloned().collect();
        let walk = walk_content(&content);
        for relative in &walk.files {
            let content_path = format!(
                "extensions/{MUTABLE_HEAD_EXTENSION}/{HEAD_DIR}/{}/{relative}",
                inventory.content_dir()
            );
            if !referenced.contains(&content_path) {
                report.error(format!(
                    "mutable HEAD: {content_path}: file is not referenced by the manifest"
                ));
            }
            match relative.split('/').next().and_then(revision_number) {
                Some(number) if numbers.contains(&number) => {}
                _ => report.error(format!(
                    "mutable HEAD: {relative:?} is not under a revision directory (rN)"
                )),
            }
        }
        for relative in &walk.empty_dirs {
            report.error(format!(
                "mutable HEAD: {relative:?}: empty directory in the content tree"
            ));
        }
        for relative in &walk.links {
            report.error(format!(
                "mutable HEAD: {relative:?}: links are not portable and MUST NOT be used in OCFL"
            ));
        }
    }
}
