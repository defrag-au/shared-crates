//! Filesystem-backed OCFL 1.1 — emit objects version by version, read their
//! state back, verify them against their own bytes, and lay out a collection
//! as an OCFL storage root.
//!
//! The write order is deliberate: a new version's content and inventory land
//! first, and the **root inventory — the pointer readers follow — is swapped
//! last**, so a crash at any point leaves the object pointing at a complete
//! version. Version directories, once written, are never touched again.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::digest::{ContentAlgorithm, DigestAlgorithm, digest_eq};
use crate::inventory::{
    INVENTORY_FILENAME, Inventory, OBJECT_DECL_CONTENTS, OBJECT_DECL_FILENAME, ROOT_DECL_CONTENTS,
    ROOT_DECL_FILENAME, User, parse_sidecar, sidecar_name, version_number,
};
use crate::plan::{LogicalState, PlanError, VersionPlan, plan_version};

mod mutable_head;
mod schema_registry;

pub use mutable_head::{MutableHead, MutableHeadInfo};
pub use schema_registry::{SCHEMA_REGISTRY_EXTENSION, SchemaEntry, SchemaRegistry};

#[derive(Debug, Error)]
pub enum FsError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("inventory invalid:\n  {0}")]
    Invalid(String),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error("{0}")]
    Other(String),
}

fn io(path: &Path, source: std::io::Error) -> FsError {
    FsError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// The current time as the RFC 3339 UTC string OCFL versions carry. A native
/// convenience — the pure path never reads a clock; hosts pass `created` in.
pub fn now_rfc3339() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    crate::rfc3339_utc(seconds as i64)
}

/// Read and sidecar-check an inventory at `dir` (root or a version directory).
fn read_inventory_at(dir: &Path) -> Result<(Inventory, Vec<u8>), FsError> {
    let path = dir.join(INVENTORY_FILENAME);
    let bytes = std::fs::read(&path).map_err(|error| io(&path, error))?;
    let inventory: Inventory = serde_json::from_slice(&bytes).map_err(|source| FsError::Parse {
        path: path.clone(),
        source,
    })?;
    let sidecar_path = dir.join(sidecar_name(inventory.digest_algorithm));
    let sidecar =
        std::fs::read_to_string(&sidecar_path).map_err(|error| io(&sidecar_path, error))?;
    let stated = parse_sidecar(&sidecar).ok_or_else(|| {
        FsError::Other(format!(
            "{}: malformed sidecar contents",
            sidecar_path.display()
        ))
    })?;
    let recomputed = inventory.digest_algorithm.digest(&bytes);
    if !digest_eq(&stated, &recomputed) {
        return Err(FsError::Other(format!(
            "{}: sidecar digest {stated} does not match the inventory's {recomputed}",
            sidecar_path.display()
        )));
    }
    Ok((inventory, bytes))
}

/// Write `inventory.json` and its digest sidecar into `dir`. The sidecar is
/// written second — it is the last step of any inventory occurrence (§3.6).
fn write_inventory_pair(
    dir: &Path,
    algorithm: ContentAlgorithm,
    bytes: &[u8],
) -> Result<(), FsError> {
    std::fs::create_dir_all(dir).map_err(|error| io(dir, error))?;
    let inventory_path = dir.join(INVENTORY_FILENAME);
    std::fs::write(&inventory_path, bytes).map_err(|error| io(&inventory_path, error))?;
    let sidecar_path = dir.join(sidecar_name(algorithm));
    let contents = crate::inventory::sidecar_contents(&algorithm.digest(bytes));
    std::fs::write(&sidecar_path, contents).map_err(|error| io(&sidecar_path, error))?;
    Ok(())
}

// ── writer ──────────────────────────────────────────────────────────────────

/// Emit an object version by version.
pub struct ObjectWriter {
    root: PathBuf,
    inventory: Inventory,
}

impl ObjectWriter {
    /// Create an object root with its conformance declaration. The object is
    /// not a valid OCFL object until the first version commits.
    pub fn create(
        root: impl Into<PathBuf>,
        id: impl Into<String>,
        algorithm: ContentAlgorithm,
    ) -> Result<Self, FsError> {
        let root = root.into();
        if root.join(INVENTORY_FILENAME).exists() {
            return Err(FsError::Other(format!(
                "{} already holds an object — use ObjectWriter::open to add versions",
                root.display()
            )));
        }
        std::fs::create_dir_all(&root).map_err(|error| io(&root, error))?;
        let declaration = root.join(OBJECT_DECL_FILENAME);
        std::fs::write(&declaration, OBJECT_DECL_CONTENTS)
            .map_err(|error| io(&declaration, error))?;
        Ok(Self {
            root,
            inventory: Inventory::new(id, algorithm),
        })
    }

    /// Open an existing object to add versions; validates it structurally.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, FsError> {
        let root = root.into();
        let (inventory, _) = read_inventory_at(&root)?;
        let problems = inventory.validate();
        if !problems.is_ok() {
            return Err(FsError::Invalid(problems.errors.join("\n  ")));
        }
        Ok(Self { root, inventory })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn inventory(&self) -> &Inventory {
        &self.inventory
    }

    /// Set the object's content directory name — `contentDirectory`, fixed for
    /// the object's life (§3.3.1), so only before the first commit. The
    /// default is `content`.
    pub fn set_content_directory(&mut self, name: impl Into<String>) -> Result<(), FsError> {
        let name = name.into();
        if !self.inventory.versions.is_empty() {
            return Err(FsError::Other(
                "contentDirectory must be set before the first version commits".into(),
            ));
        }
        if name.is_empty() || name.contains('/') || name == "." || name == ".." {
            return Err(FsError::Other(format!(
                "contentDirectory {name:?} is not a single, safe path element"
            )));
        }
        self.inventory.content_directory = Some(name);
        Ok(())
    }

    /// Commit `state` as the next version, returning its directory name.
    ///
    /// Files whose digest is already stored are referenced, never copied; new
    /// content is written under the new version's content directory. The root
    /// inventory is rewritten last. Refused while a mutable HEAD is active —
    /// commit or purge it first (§0005's version-conflict rule).
    pub fn commit(
        &mut self,
        state: &LogicalState,
        created: impl Into<String>,
        message: Option<String>,
        user: Option<User>,
    ) -> Result<String, FsError> {
        if MutableHead::exists(&self.root) {
            return Err(FsError::Other(
                "the object has an active mutable HEAD — commit or purge it before adding a root version"
                    .into(),
            ));
        }
        let plan = plan_version(&self.inventory, state, created, message, user)?;
        self.commit_plan(&plan)?;
        Ok(plan.version_dir)
    }

    fn commit_plan(&mut self, plan: &VersionPlan) -> Result<(), FsError> {
        let version_path = self.root.join(&plan.version_dir);
        if version_path.exists() {
            return Err(FsError::Other(format!(
                "{} already exists — a previous write was interrupted; resolve it before committing",
                version_path.display()
            )));
        }
        for (content_path, bytes) in &plan.writes {
            let path = self.root.join(content_path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| io(parent, error))?;
            }
            std::fs::write(&path, bytes).map_err(|error| io(&path, error))?;
        }
        let bytes =
            serde_json::to_vec_pretty(&plan.inventory).map_err(|source| FsError::Parse {
                path: version_path.join(INVENTORY_FILENAME),
                source,
            })?;
        write_inventory_pair(&version_path, plan.inventory.digest_algorithm, &bytes)?;
        // Everything is in place; the root inventory is the last write and the
        // pointer every reader follows.
        write_inventory_pair(&self.root, plan.inventory.digest_algorithm, &bytes)?;
        self.inventory = plan.inventory.clone();
        Ok(())
    }
}

// ── reader ──────────────────────────────────────────────────────────────────

/// Read an object's inventory, resolve its state, and verify it. When the
/// object has an active mutable HEAD (`0005-mutable-head`), [`Object::inventory`]
/// is that mutable inventory, as the extension's access rules require.
pub struct Object {
    root: PathBuf,
    inventory: Inventory,
    root_inventory: Inventory,
    mutable_head: Option<MutableHeadInfo>,
}

impl Object {
    /// Open an object: parses the root inventory and checks its sidecar — the
    /// mutable HEAD inventory instead where one is active. A structurally
    /// non-conforming object still opens — [`Object::verify`] reports what is
    /// wrong; a corrupt mutable HEAD does not, because it is the effective
    /// state and silently falling back would serve stale content.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, FsError> {
        let root = root.into();
        let (root_inventory, _) = read_inventory_at(&root)?;
        let (inventory, mutable_head) = if MutableHead::exists(&root) {
            let head = MutableHead::open(&root)?;
            let info = head.info();
            (head.inventory().clone(), Some(info))
        } else {
            (root_inventory.clone(), None)
        };
        Ok(Self {
            root,
            inventory,
            root_inventory,
            mutable_head,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The effective inventory: the mutable HEAD's while one is active, the
    /// root inventory otherwise.
    pub fn inventory(&self) -> &Inventory {
        &self.inventory
    }

    /// The root inventory: the last committed immutable state.
    pub fn root_inventory(&self) -> &Inventory {
        &self.root_inventory
    }

    /// The active mutable HEAD, when the object has one.
    pub fn mutable_head(&self) -> Option<&MutableHeadInfo> {
        self.mutable_head.as_ref()
    }

    /// The logical state at `version` (default: the head) as logical path →
    /// content path.
    pub fn state(&self, version: Option<&str>) -> Result<BTreeMap<String, String>, FsError> {
        let version = version.unwrap_or(&self.inventory.head);
        self.inventory.resolve_state(version).ok_or_else(|| {
            FsError::Other(format!(
                "cannot resolve the state of version {version:?} (unknown version, or a state digest missing from the manifest)"
            ))
        })
    }

    /// Read a logical file at the head version, verifying its digest against
    /// the manifest.
    pub fn read_file(&self, logical: &str) -> Result<Vec<u8>, FsError> {
        let state = self.state(None)?;
        let content = state.get(logical).ok_or_else(|| {
            FsError::Other(format!("no logical file {logical:?} at the head version"))
        })?;
        let path = self.root.join(content);
        let bytes = std::fs::read(&path).map_err(|error| io(&path, error))?;
        if let Some(digest) = self
            .inventory
            .manifest
            .iter()
            .find(|(_, paths)| paths.iter().any(|path| path == content))
            .map(|(digest, _)| digest.clone())
        {
            let got = self.inventory.digest_algorithm.digest(&bytes);
            if !digest_eq(&digest, &got) {
                return Err(FsError::Other(format!(
                    "{content}: digest {got} does not match the manifest's {digest}"
                )));
            }
        }
        Ok(bytes)
    }

    /// Check the whole object against OCFL 1.1's structural and fixity rules:
    /// the committed (root) state, plus any active mutable HEAD against
    /// `0005-mutable-head`.
    pub fn verify(&self) -> VerifyReport {
        let mut report = verify_object(&self.root, &self.root_inventory);
        if self.mutable_head.is_some() {
            mutable_head::verify(&self.root, &self.root_inventory, &mut report);
        }
        report
    }
}

// ── verification ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Violates an OCFL `MUST` — the object is not conforming.
    Error,
    /// A `SHOULD`-level deviation worth telling the operator about.
    Warning,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub severity: Severity,
    pub message: String,
}

/// What verification found, plus a small summary of what was checked.
#[derive(Debug, Clone)]
pub struct VerifyReport {
    pub root: PathBuf,
    pub id: String,
    pub versions: usize,
    /// Files verified through the manifest (existence + digest).
    pub files: usize,
    pub bytes: u64,
    pub issues: Vec<Issue>,
}

impl VerifyReport {
    /// True when no error-severity issue was found.
    pub fn is_ok(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| issue.severity == Severity::Error)
    }

    fn error(&mut self, message: impl Into<String>) {
        self.issues.push(Issue {
            severity: Severity::Error,
            message: message.into(),
        });
    }

    fn warning(&mut self, message: impl Into<String>) {
        self.issues.push(Issue {
            severity: Severity::Warning,
            message: message.into(),
        });
    }
}

#[derive(Default)]
struct ContentWalk {
    files: Vec<String>,
    empty_dirs: Vec<String>,
    links: Vec<String>,
}

fn walk_content(dir: &Path) -> ContentWalk {
    let mut walk = ContentWalk::default();
    walk_into(dir, "", &mut walk);
    walk
}

fn walk_into(dir: &Path, prefix: &str, walk: &mut ContentWalk) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            walk.links.push(relative);
        } else if file_type.is_dir() {
            let before = walk.files.len() + walk.links.len();
            walk_into(&entry.path(), &relative, walk);
            if walk.files.len() + walk.links.len() == before {
                walk.empty_dirs.push(relative);
            }
        } else {
            walk.files.push(relative);
        }
    }
}

fn verify_object(root: &Path, inventory: &Inventory) -> VerifyReport {
    let mut report = VerifyReport {
        root: root.to_path_buf(),
        id: inventory.id.clone(),
        versions: inventory.versions.len(),
        files: 0,
        bytes: 0,
        issues: Vec::new(),
    };

    // The inventory's own structural rules (§3.5).
    let problems = inventory.validate();
    for message in problems.errors {
        report.error(format!("inventory: {message}"));
    }
    for message in problems.warnings {
        report.warning(format!("inventory: {message}"));
    }

    let algorithm = inventory.digest_algorithm;
    let content_dir = inventory.content_dir().to_string();

    // Object root: exactly one conformance declaration, nothing unexpected.
    // The root inventory and its sidecar are re-checked here so verify stands
    // alone, not just on top of a successful open.
    let root_inventory_bytes = match read_inventory_at(root) {
        Ok((_, bytes)) => Some(bytes),
        Err(error) => {
            report.error(format!("root inventory: {error}"));
            None
        }
    };
    match std::fs::read_dir(root) {
        Ok(entries) => {
            let mut declarations = Vec::new();
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with("0=") {
                    declarations.push(name);
                    continue;
                }
                let allowed = name == INVENTORY_FILENAME
                    || name == sidecar_name(algorithm)
                    || name == "logs"
                    || name == "extensions"
                    || version_number(&name).is_some();
                if !allowed {
                    report.error(format!("unexpected entry at the object root: {name:?}"));
                }
            }
            match declarations.as_slice() {
                [only] if only.as_str() == OBJECT_DECL_FILENAME => {
                    match std::fs::read_to_string(root.join(only)) {
                        Ok(contents) if contents == OBJECT_DECL_CONTENTS => {}
                        Ok(contents) => report.error(format!(
                            "object declaration contents {contents:?} ≠ {OBJECT_DECL_CONTENTS:?}"
                        )),
                        Err(error) => {
                            report.error(format!("object declaration unreadable: {error}"))
                        }
                    }
                }
                [only] => report.error(format!(
                    "unsupported conformance declaration {only:?} — this crate speaks OCFL 1.1"
                )),
                [] => report
                    .error("no object conformance declaration (0=ocfl_object_1.1) at the root"),
                many => report.error(format!(
                    "{} conformance declarations at the root; exactly one is required",
                    many.len()
                )),
            }
        }
        Err(error) => report.error(format!("object root unreadable: {error}")),
    }

    // Version directories: allowed children, their inventories, and agreement
    // with the root inventory.
    let dirs = inventory.version_dirs();
    for dir in &dirs {
        let path = root.join(dir);
        if !path.is_dir() {
            report.error(format!("{dir}: version directory missing"));
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name == INVENTORY_FILENAME
                    || name == sidecar_name(algorithm)
                    || name == content_dir
                {
                    continue;
                }
                let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
                if is_dir {
                    report.warning(format!(
                        "{dir}: unexpected directory {name:?} (a version directory SHOULD contain only the content directory)"
                    ));
                } else {
                    report.error(format!(
                        "{dir}: unexpected file {name:?} in a version directory"
                    ));
                }
            }
        }
        if path.join(INVENTORY_FILENAME).is_file() {
            match read_inventory_at(&path) {
                Ok((version_inventory, bytes)) => {
                    if version_inventory.head != *dir {
                        report.error(format!(
                            "{dir}: its inventory has head {:?} (expected {dir:?})",
                            version_inventory.head
                        ));
                    }
                    for (other, block) in &version_inventory.versions {
                        match inventory.versions.get(other) {
                            Some(root_block) if root_block.state == block.state => {
                                if root_block.created != block.created
                                    || root_block.message != block.message
                                    || root_block.user != block.user
                                {
                                    report.warning(format!(
                                        "{dir}: version {other:?}'s provenance differs from the root inventory"
                                    ));
                                }
                            }
                            Some(_) => report.error(format!(
                                "{dir}: version {other:?}'s state differs from the root inventory"
                            )),
                            None => report.error(format!(
                                "{dir}: describes version {other:?}, absent from the root inventory"
                            )),
                        }
                    }
                    if inventory.head == *dir
                        && root_inventory_bytes
                            .as_ref()
                            .is_some_and(|root_bytes| root_bytes != &bytes)
                    {
                        report.error(format!(
                            "{dir}: the newest version's inventory differs from the root inventory (they must be identical)"
                        ));
                    }
                }
                Err(error) => report.error(format!("{dir}: {error}")),
            }
        } else {
            report.warning(format!(
                "{dir}: no version inventory (SHOULD, for redundancy) — the object validates only against the root copy"
            ));
        }
    }

    // Manifest ↔ filesystem: every listed path exists and hashes to its
    // digest; every file under a content directory is listed.
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    for (digest, paths) in &inventory.manifest {
        for content_path in paths {
            let file = root.join(content_path);
            match std::fs::read(&file) {
                Ok(bytes) => {
                    let got = algorithm.digest(&bytes);
                    if !digest_eq(digest, &got) {
                        report.error(format!("{content_path}: digest {got} ≠ manifest {digest}"));
                    }
                    report.files += 1;
                    report.bytes += bytes.len() as u64;
                    referenced.insert(content_path.clone());
                }
                Err(error) => {
                    report.error(format!("{content_path}: missing or unreadable ({error})"))
                }
            }
        }
    }
    // Fixity (§3.5.4): every listed value must match the file at its content
    // path. Unknown algorithms were ignored by validation and are ignored here
    // for the same reason (§3.4).
    for (algorithm, digests) in &inventory.fixity {
        if !algorithm.is_known() {
            continue;
        }
        for (digest, paths) in digests {
            for content_path in paths {
                let file = root.join(content_path);
                match std::fs::read(&file) {
                    Ok(bytes) => {
                        if let Some(got) = algorithm.digest(&bytes)
                            && !digest_eq(digest, &got)
                        {
                            report.error(format!(
                                "{content_path}: fixity {algorithm} digest {got} ≠ {digest}"
                            ));
                        }
                    }
                    Err(error) => report.error(format!(
                        "{content_path}: fixity {algorithm} check: missing or unreadable ({error})"
                    )),
                }
            }
        }
    }

    for dir in &dirs {
        let content = root.join(dir).join(&content_dir);
        if !content.is_dir() {
            continue;
        }
        let walk = walk_content(&content);
        for relative in &walk.files {
            let content_path = format!("{dir}/{content_dir}/{relative}");
            if !referenced.contains(&content_path) {
                report.error(format!(
                    "{content_path}: file is not referenced by the manifest"
                ));
            }
        }
        for relative in &walk.empty_dirs {
            report.error(format!(
                "{dir}/{content_dir}/{relative}: empty directory in the content tree"
            ));
        }
        for relative in &walk.links {
            report.error(format!(
                "{dir}/{content_dir}/{relative}: links are not portable and MUST NOT be used in OCFL"
            ));
        }
    }

    report
}

// ── storage root ────────────────────────────────────────────────────────────

/// Registered storage layouts this crate can map object ids through — one
/// variant per community extension this crate implements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layout {
    /// Extension `0002-flat-direct-storage-layout`: objects are direct children
    /// and the object id is the directory name.
    FlatDirect,
    /// Extension `0003-hash-and-id-n-tuple-storage-layout`: n-tuples of the
    /// id's digest, ending in a percent-encoded encapsulation directory named
    /// from the id.
    HashIdNTuple {
        algorithm: DigestAlgorithm,
        tuple_size: u32,
        number_of_tuples: u32,
    },
    /// Extension `0004-hashed-n-tuple-storage-layout`: n-tuples of the id's
    /// digest, ending in a directory named from the digest itself.
    HashedNTuple {
        algorithm: DigestAlgorithm,
        tuple_size: u32,
        number_of_tuples: u32,
        short_object_root: bool,
    },
    /// Extension `0006-flat-omit-prefix-storage-layout`: everything up to and
    /// including the *last* `delimiter` is omitted from the directory name —
    /// `urn:mugz:0001` with `:` maps to `0001`.
    OmitPrefix { delimiter: String },
    /// Extension `0007-n-tuple-omit-prefix-storage-layout`: n-tuples of the
    /// prefix-omitted id — zero-padded to fill the tuple space, optionally
    /// reversed — ending in the prefix-omitted id.
    NTupleOmitPrefix {
        delimiter: String,
        tuple_size: u32,
        number_of_tuples: u32,
        zero_padding: ZeroPadding,
        reverse_object_root: bool,
    },
    /// Extension `0010-differential-n-tuple-omit-prefix-storage-layout`:
    /// n-tuples of unequal, configured sizes, optionally ending in the
    /// prefix-omitted id.
    DifferentialNTupleOmitPrefix {
        delimiter: String,
        tuple_segment_sizes: Vec<u32>,
        full_identifier_as_object_root: bool,
    },
    /// Extension `0012-hash-and-no-prefix-id-n-tuple-storage-layout`: `0003`
    /// with a prefix stripped (at any of `delimiters`) before hashing.
    HashNoPrefixIdNTuple {
        algorithm: DigestAlgorithm,
        tuple_size: u32,
        number_of_tuples: u32,
        delimiters: Vec<String>,
    },
}

/// Which side of the prefix-omitted id `0007-n-tuple-omit-prefix-storage-layout`
/// zero-pads when the id is shorter than the tuple space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ZeroPadding {
    Left,
    Right,
}

impl Layout {
    pub const fn extension_name(&self) -> &'static str {
        match self {
            Self::FlatDirect => "0002-flat-direct-storage-layout",
            Self::HashIdNTuple { .. } => "0003-hash-and-id-n-tuple-storage-layout",
            Self::HashedNTuple { .. } => "0004-hashed-n-tuple-storage-layout",
            Self::OmitPrefix { .. } => "0006-flat-omit-prefix-storage-layout",
            Self::NTupleOmitPrefix { .. } => "0007-n-tuple-omit-prefix-storage-layout",
            Self::DifferentialNTupleOmitPrefix { .. } => {
                "0010-differential-n-tuple-omit-prefix-storage-layout"
            }
            Self::HashNoPrefixIdNTuple { .. } => {
                "0012-hash-and-no-prefix-id-n-tuple-storage-layout"
            }
        }
    }

    /// A human-readable `description` for `ocfl_layout.json` (§4.1).
    pub fn description(&self) -> String {
        match self {
            Self::FlatDirect => "Objects are direct children named after the object id".into(),
            Self::HashIdNTuple {
                algorithm,
                tuple_size,
                number_of_tuples,
            } => format!(
                "{number_of_tuples} directory level(s) of {tuple_size} characters of the {algorithm} digest of the object id, ending in a percent-encoded directory named after the id"
            ),
            Self::HashedNTuple {
                algorithm,
                tuple_size,
                number_of_tuples,
                short_object_root,
            } => {
                let root = if *short_object_root {
                    "the digest's unused remainder"
                } else {
                    "the full digest"
                };
                format!(
                    "{number_of_tuples} directory level(s) of {tuple_size} characters of the {algorithm} digest of the object id, ending in {root}"
                )
            }
            Self::OmitPrefix { delimiter } => format!(
                "Objects are direct children named after the part of the object id following its last {delimiter:?}"
            ),
            Self::NTupleOmitPrefix {
                delimiter,
                tuple_size,
                number_of_tuples,
                ..
            } => format!(
                "{number_of_tuples} directory level(s) of {tuple_size} characters of the prefix-omitted object id, ending in the prefix-omitted id (prefix ends at the last {delimiter:?})"
            ),
            Self::DifferentialNTupleOmitPrefix {
                delimiter,
                tuple_segment_sizes,
                full_identifier_as_object_root,
            } => {
                let leaf = if *full_identifier_as_object_root {
                    ", ending in the prefix-omitted id"
                } else {
                    ""
                };
                format!(
                    "Directory levels of sizes {tuple_segment_sizes:?} of the prefix-omitted object id (prefix ends at the last {delimiter:?}){leaf}"
                )
            }
            Self::HashNoPrefixIdNTuple {
                algorithm,
                tuple_size,
                number_of_tuples,
                ..
            } => format!(
                "{number_of_tuples} directory level(s) of {tuple_size} characters of the {algorithm} digest of the prefix-stripped object id, ending in a percent-encoded directory named after the prefix-stripped id"
            ),
        }
    }

    /// The relative object root path for `id` under the storage root.
    pub fn object_dirname(&self, id: &str) -> Result<String, FsError> {
        match self {
            Self::FlatDirect => {
                safe_element(id, id, self.extension_name())?;
                Ok(id.to_string())
            }
            Self::HashIdNTuple {
                algorithm,
                tuple_size,
                number_of_tuples,
            } => {
                let digest = layout_digest(algorithm, id)?;
                let (tuples, _) = tuple_prefix(
                    &digest,
                    *tuple_size as usize,
                    *number_of_tuples as usize,
                    self.extension_name(),
                )?;
                let path = format!("{tuples}{}", encapsulated_directory(id, &digest));
                check_relative(&path, self.extension_name())?;
                Ok(path)
            }
            Self::HashedNTuple {
                algorithm,
                tuple_size,
                number_of_tuples,
                short_object_root,
            } => {
                let digest = layout_digest(algorithm, id)?;
                let (tuples, used) = tuple_prefix(
                    &digest,
                    *tuple_size as usize,
                    *number_of_tuples as usize,
                    self.extension_name(),
                )?;
                let root = if *short_object_root {
                    &digest[used..]
                } else {
                    digest.as_str()
                };
                if root.is_empty() {
                    return Err(FsError::Other(format!(
                        "{}: shortObjectRoot is true but the n-tuples consume the whole digest",
                        self.extension_name()
                    )));
                }
                let path = format!("{tuples}{root}");
                check_relative(&path, self.extension_name())?;
                Ok(path)
            }
            Self::OmitPrefix { delimiter } => {
                let name = omit_prefix_last(id, delimiter)?;
                safe_element(&name, id, self.extension_name())?;
                Ok(name)
            }
            Self::NTupleOmitPrefix {
                delimiter,
                tuple_size,
                number_of_tuples,
                zero_padding,
                reverse_object_root,
            } => {
                check_ascii_range(id, self.extension_name())?;
                let name = omit_prefix_case_insensitive(id, delimiter, self.extension_name())?;
                let space = *tuple_size as usize * *number_of_tuples as usize;
                let mut padded = name.clone();
                if padded.len() < space {
                    let zeros = "0".repeat(space - padded.len());
                    match zero_padding {
                        ZeroPadding::Left => padded = format!("{zeros}{padded}"),
                        ZeroPadding::Right => padded.push_str(&zeros),
                    }
                }
                if *reverse_object_root {
                    padded = padded.chars().rev().collect();
                }
                let (tuples, _) = tuple_prefix(
                    &padded,
                    *tuple_size as usize,
                    *number_of_tuples as usize,
                    self.extension_name(),
                )?;
                let path = format!("{tuples}{name}");
                check_relative(&path, self.extension_name())?;
                Ok(path)
            }
            Self::DifferentialNTupleOmitPrefix {
                delimiter,
                tuple_segment_sizes,
                full_identifier_as_object_root,
            } => {
                check_ascii_range(id, self.extension_name())?;
                let name = omit_prefix_case_insensitive(id, delimiter, self.extension_name())?;
                let total: usize = tuple_segment_sizes.iter().map(|&size| size as usize).sum();
                if name.len() != total {
                    return Err(FsError::Other(format!(
                        "{}: the prefix-omitted id {name:?} is {} characters but tupleSegmentSizes sums to {total}",
                        self.extension_name(),
                        name.len()
                    )));
                }
                let mut path = String::new();
                let mut at = 0usize;
                for &size in tuple_segment_sizes {
                    let end = at + size as usize;
                    path.push_str(&name[at..end]);
                    path.push('/');
                    at = end;
                }
                if *full_identifier_as_object_root {
                    path.push_str(&name);
                } else {
                    // The final tuple directory is the object root.
                    path.pop();
                }
                check_relative(&path, self.extension_name())?;
                Ok(path)
            }
            Self::HashNoPrefixIdNTuple {
                algorithm,
                tuple_size,
                number_of_tuples,
                delimiters,
            } => {
                let name = strip_prefix_delimiters(id, delimiters);
                let digest = layout_digest(algorithm, &name)?;
                let (tuples, _) = tuple_prefix(
                    &digest,
                    *tuple_size as usize,
                    *number_of_tuples as usize,
                    self.extension_name(),
                )?;
                let path = format!("{tuples}{}", encapsulated_directory(&name, &digest));
                check_relative(&path, self.extension_name())?;
                Ok(path)
            }
        }
    }
}

/// The digest of `id` under a layout's algorithm — the id UTF-8 encoded and
/// hashed, lowercase hex. `size` is a byte count, not a hash, and cannot map
/// identifiers.
fn layout_digest(algorithm: &DigestAlgorithm, id: &str) -> Result<String, FsError> {
    match algorithm {
        DigestAlgorithm::Size => Err(FsError::Other(
            "the `size` algorithm is not a hash and cannot map object identifiers".into(),
        )),
        _ => algorithm.digest(id.as_bytes()).ok_or_else(|| {
            FsError::Other(format!(
                "the storage layout digest algorithm {algorithm} is not one this client implements"
            ))
        }),
    }
}

/// The leading n-tuple directories (`"a/b/c/"`, or `""` for zero tuples) and
/// how many characters they consumed from `text`.
fn tuple_prefix(
    text: &str,
    tuple_size: usize,
    number_of_tuples: usize,
    extension: &str,
) -> Result<(String, usize), FsError> {
    let used = tuple_size * number_of_tuples;
    if used > text.len() {
        return Err(FsError::Other(format!(
            "{extension}: tupleSize × numberOfTuples ({tuple_size} × {number_of_tuples}) exceeds the {} characters available",
            text.len()
        )));
    }
    let mut path = String::new();
    for index in 0..number_of_tuples {
        let start = index * tuple_size;
        path.push_str(&text[start..start + tuple_size]);
        path.push('/');
    }
    Ok((path, used))
}

/// `0003`/`0012` encapsulation directory: safe characters kept, everything
/// else percent-encoded from its UTF-8 bytes with lowercase hex. Encoded names
/// longer than 100 characters are cut to 100 and disambiguated with
/// `-<digest>`. The encoded form is ASCII, so the cut is a byte cut.
fn encapsulated_directory(id: &str, digest: &str) -> String {
    let mut encoded = String::new();
    for character in id.chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            encoded.push(character);
        } else {
            let mut bytes = [0u8; 4];
            for byte in character.encode_utf8(&mut bytes).bytes() {
                encoded.push_str(&format!("%{byte:02x}"));
            }
        }
    }
    if encoded.len() > 100 {
        format!("{}-{digest}", &encoded[..100])
    } else {
        encoded
    }
}

/// `0006`: everything after the right-most case-insensitive `delimiter`; an
/// error when it does not occur.
fn omit_prefix_last(id: &str, delimiter: &str) -> Result<String, FsError> {
    let length = delimiter.len();
    if length == 0 {
        return Err(FsError::Other("storage layout delimiter is empty".into()));
    }
    let mut found = None;
    for at in 0..=id.len().saturating_sub(length) {
        if id.is_char_boundary(at)
            && id.is_char_boundary(at + length)
            && id[at..at + length].eq_ignore_ascii_case(delimiter)
        {
            found = Some(at);
        }
    }
    let Some(at) = found else {
        return Err(FsError::Other(format!(
            "id {id:?} does not contain the layout delimiter {delimiter:?}"
        )));
    };
    Ok(id[at + length..].to_string())
}

/// `0007`/`0010`: everything after the right-most case-insensitive occurrence
/// of `delimiter`; the whole id when it does not occur; an error when the
/// occurrence ends the id. The id is ASCII by the time this runs.
fn omit_prefix_case_insensitive(
    id: &str,
    delimiter: &str,
    extension: &str,
) -> Result<String, FsError> {
    if delimiter.is_empty() {
        return Err(FsError::Other(format!("{extension}: delimiter is empty")));
    }
    let haystack = id.to_ascii_uppercase();
    let needle = delimiter.to_ascii_uppercase();
    let mut found = None;
    let mut from = 0;
    while let Some(offset) = haystack[from..].find(&needle) {
        found = Some(from + offset);
        from += offset + 1;
    }
    match found {
        None => Ok(id.to_string()),
        Some(start) if start + delimiter.len() == id.len() => Err(FsError::Other(format!(
            "{extension}: the delimiter {delimiter:?} ends the id {id:?}"
        ))),
        Some(start) => Ok(id[start + delimiter.len()..].to_string()),
    }
}

/// `0012`: the id from just after the right-most occurrence of any delimiter
/// that does not itself end the id; the whole id when none applies.
/// Case-sensitive, unlike `0007`'s delimiter.
fn strip_prefix_delimiters(id: &str, delimiters: &[String]) -> String {
    // The search window excludes the id's final character: an occurrence
    // ending the id is ignored, and an earlier one — if any — wins.
    let searchable = match id.char_indices().next_back() {
        Some((last, _)) => &id[..last],
        None => "",
    };
    let mut cut = 0;
    for delimiter in delimiters {
        if !delimiter.is_empty()
            && let Some(position) = searchable.rfind(delimiter.as_str())
        {
            cut = cut.max(position + delimiter.len());
        }
    }
    id[cut..].to_string()
}

/// `0007`/`0010` are defined over the ASCII subset 0x20..=0x7F; anything else
/// in the id is an error.
fn check_ascii_range(id: &str, extension: &str) -> Result<(), FsError> {
    if id.bytes().all(|byte| (0x20..=0x7f).contains(&byte)) {
        Ok(())
    } else {
        Err(FsError::Other(format!(
            "{extension}: the id {id:?} leaves the ASCII 0x20–0x7F range this layout is defined over"
        )))
    }
}

/// A single safe path element.
fn safe_element(name: &str, id: &str, extension: &str) -> Result<(), FsError> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(FsError::Other(format!(
            "{extension}: object id {id:?} maps to a directory name that is not a safe path element: {name:?}"
        )));
    }
    Ok(())
}

/// A safe `/`-joined relative path — every segment non-empty, `.`, `..` and
/// backslashes rejected.
fn check_relative(path: &str, extension: &str) -> Result<(), FsError> {
    let bad = path.is_empty()
        || path.split('/').any(|segment| {
            segment.is_empty() || segment == "." || segment == ".." || segment.contains('\\')
        });
    if bad {
        return Err(FsError::Other(format!(
            "{extension}: the object id maps to a path that is not safe: {path:?}"
        )));
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct LayoutFile {
    extension: String,
    description: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct OmitPrefixConfig {
    #[serde(rename = "extensionName")]
    extension_name: String,
    delimiter: String,
}

fn default_sha256() -> DigestAlgorithm {
    DigestAlgorithm::Sha256
}

fn default_three() -> u32 {
    3
}

fn default_colon() -> String {
    ":".to_string()
}

fn default_segment_sizes() -> Vec<u32> {
    vec![2, 3, 2, 4]
}

fn default_zero_padding() -> ZeroPadding {
    ZeroPadding::Left
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HashIdNTupleConfig {
    extension_name: String,
    #[serde(default = "default_sha256")]
    digest_algorithm: DigestAlgorithm,
    #[serde(default = "default_three")]
    tuple_size: u32,
    #[serde(default = "default_three")]
    number_of_tuples: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HashedNTupleConfig {
    extension_name: String,
    #[serde(default = "default_sha256")]
    digest_algorithm: DigestAlgorithm,
    #[serde(default = "default_three")]
    tuple_size: u32,
    #[serde(default = "default_three")]
    number_of_tuples: u32,
    #[serde(default)]
    short_object_root: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NTupleOmitPrefixConfig {
    extension_name: String,
    #[serde(default = "default_colon")]
    delimiter: String,
    #[serde(default = "default_three")]
    tuple_size: u32,
    #[serde(default = "default_three")]
    number_of_tuples: u32,
    #[serde(default = "default_zero_padding")]
    zero_padding: ZeroPadding,
    #[serde(default)]
    reverse_object_root: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DifferentialNTupleOmitPrefixConfig {
    extension_name: String,
    #[serde(default = "default_colon")]
    delimiter: String,
    #[serde(default = "default_segment_sizes")]
    tuple_segment_sizes: Vec<u32>,
    #[serde(default)]
    full_identifier_as_object_root: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HashNoPrefixIdNTupleConfig {
    extension_name: String,
    #[serde(default = "default_sha256")]
    digest_algorithm: DigestAlgorithm,
    #[serde(default = "default_three")]
    tuple_size: u32,
    #[serde(default = "default_three")]
    number_of_tuples: u32,
    #[serde(default)]
    delimiters: Vec<String>,
}

fn check_extension_name(extension: &str, config_extension: &str) -> Result<(), FsError> {
    if config_extension != extension {
        return Err(FsError::Other(format!(
            "extension config names {config_extension:?}, expected {extension:?}"
        )));
    }
    Ok(())
}

/// The shared parameter rules of the digest-tuple layouts (0003/0004/0012).
fn validate_tuple_bounds(
    extension: &str,
    algorithm: &DigestAlgorithm,
    tuple_size: u32,
    number_of_tuples: u32,
) -> Result<(), FsError> {
    if tuple_size > 32 || number_of_tuples > 32 {
        return Err(FsError::Other(format!(
            "{extension}: tupleSize and numberOfTuples must each be between 0 and 32"
        )));
    }
    if (tuple_size == 0) != (number_of_tuples == 0) {
        return Err(FsError::Other(format!(
            "{extension}: tupleSize and numberOfTuples must both be 0 or both be greater than 0"
        )));
    }
    let Some(hex_len) = algorithm.hex_len() else {
        return Err(FsError::Other(format!(
            "{extension}: digestAlgorithm {algorithm} is not a fixed-length hash and cannot map object identifiers"
        )));
    };
    if (tuple_size as usize) * (number_of_tuples as usize) > hex_len {
        return Err(FsError::Other(format!(
            "{extension}: tupleSize × numberOfTuples ({tuple_size} × {number_of_tuples}) exceeds the {hex_len}-character {algorithm} digest"
        )));
    }
    Ok(())
}

/// `0004`: `shortObjectRoot` must be false when the tuples use the whole digest.
fn validate_short_object_root(
    extension: &str,
    algorithm: &DigestAlgorithm,
    tuple_size: u32,
    number_of_tuples: u32,
    short_object_root: bool,
) -> Result<(), FsError> {
    if short_object_root && algorithm.hex_len() == Some((tuple_size * number_of_tuples) as usize) {
        return Err(FsError::Other(format!(
            "{extension}: shortObjectRoot must be false when tupleSize × numberOfTuples uses the whole digest"
        )));
    }
    Ok(())
}

/// `0007`'s parameter rules: positive tuple sizes, a non-empty delimiter.
fn validate_n_tuple_omit_prefix(
    extension: &str,
    delimiter: &str,
    tuple_size: u32,
    number_of_tuples: u32,
) -> Result<(), FsError> {
    if delimiter.is_empty() {
        return Err(FsError::Other(format!("{extension}: delimiter is empty")));
    }
    if tuple_size == 0 || tuple_size > 32 || number_of_tuples == 0 || number_of_tuples > 32 {
        return Err(FsError::Other(format!(
            "{extension}: tupleSize and numberOfTuples must each be between 1 and 32"
        )));
    }
    Ok(())
}

/// `0012`: delimiters must be non-empty strings.
fn validate_delimiters(extension: &str, delimiters: &[String]) -> Result<(), FsError> {
    if delimiters.iter().any(String::is_empty) {
        return Err(FsError::Other(format!(
            "{extension}: delimiters must be non-empty strings"
        )));
    }
    Ok(())
}

/// `0010`'s parameter rules: a non-empty delimiter. The segment sizes are
/// checked against each identifier at mapping time, as the spec prescribes.
fn validate_differential_omit_prefix(extension: &str, delimiter: &str) -> Result<(), FsError> {
    if delimiter.is_empty() {
        return Err(FsError::Other(format!("{extension}: delimiter is empty")));
    }
    Ok(())
}

fn write_extension_config<T: Serialize>(
    root: &Path,
    extension: &str,
    config: &T,
) -> Result<(), FsError> {
    let path = root.join("extensions").join(extension).join("config.json");
    let parent = path.parent().expect("config path has a parent");
    std::fs::create_dir_all(parent).map_err(|error| io(parent, error))?;
    let bytes = serde_json::to_vec_pretty(config).map_err(|source| FsError::Parse {
        path: path.clone(),
        source,
    })?;
    std::fs::write(&path, bytes).map_err(|error| io(&path, error))
}

fn read_extension_config<T: serde::de::DeserializeOwned>(
    root: &Path,
    extension: &str,
) -> Result<T, FsError> {
    let path = root.join("extensions").join(extension).join("config.json");
    let bytes = std::fs::read(&path).map_err(|error| io(&path, error))?;
    serde_json::from_slice(&bytes).map_err(|source| FsError::Parse { path, source })
}

/// A collection laid out as an OCFL storage root (§4): a conformance
/// declaration, `ocfl_layout.json`, and object directories per a registered
/// layout extension.
pub struct StorageRoot {
    root: PathBuf,
    layout: Layout,
}

impl StorageRoot {
    /// Create a storage root. For parameterised layouts the extension's
    /// `config.json` is written alongside (§4.4, extensions repo README).
    pub fn create(root: impl Into<PathBuf>, layout: Layout) -> Result<Self, FsError> {
        let root = root.into();
        std::fs::create_dir_all(&root).map_err(|error| io(&root, error))?;
        let declaration = root.join(ROOT_DECL_FILENAME);
        std::fs::write(&declaration, ROOT_DECL_CONTENTS)
            .map_err(|error| io(&declaration, error))?;
        let extension = layout.extension_name();
        match &layout {
            Layout::FlatDirect => {}
            Layout::OmitPrefix { delimiter } => {
                if delimiter.is_empty() {
                    return Err(FsError::Other("storage layout delimiter is empty".into()));
                }
                write_extension_config(
                    &root,
                    extension,
                    &OmitPrefixConfig {
                        extension_name: extension.to_string(),
                        delimiter: delimiter.clone(),
                    },
                )?;
            }
            Layout::HashIdNTuple {
                algorithm,
                tuple_size,
                number_of_tuples,
            } => {
                validate_tuple_bounds(extension, algorithm, *tuple_size, *number_of_tuples)?;
                write_extension_config(
                    &root,
                    extension,
                    &HashIdNTupleConfig {
                        extension_name: extension.to_string(),
                        digest_algorithm: algorithm.clone(),
                        tuple_size: *tuple_size,
                        number_of_tuples: *number_of_tuples,
                    },
                )?;
            }
            Layout::HashedNTuple {
                algorithm,
                tuple_size,
                number_of_tuples,
                short_object_root,
            } => {
                validate_tuple_bounds(extension, algorithm, *tuple_size, *number_of_tuples)?;
                validate_short_object_root(
                    extension,
                    algorithm,
                    *tuple_size,
                    *number_of_tuples,
                    *short_object_root,
                )?;
                write_extension_config(
                    &root,
                    extension,
                    &HashedNTupleConfig {
                        extension_name: extension.to_string(),
                        digest_algorithm: algorithm.clone(),
                        tuple_size: *tuple_size,
                        number_of_tuples: *number_of_tuples,
                        short_object_root: *short_object_root,
                    },
                )?;
            }
            Layout::NTupleOmitPrefix {
                delimiter,
                tuple_size,
                number_of_tuples,
                zero_padding,
                reverse_object_root,
            } => {
                validate_n_tuple_omit_prefix(extension, delimiter, *tuple_size, *number_of_tuples)?;
                write_extension_config(
                    &root,
                    extension,
                    &NTupleOmitPrefixConfig {
                        extension_name: extension.to_string(),
                        delimiter: delimiter.clone(),
                        tuple_size: *tuple_size,
                        number_of_tuples: *number_of_tuples,
                        zero_padding: *zero_padding,
                        reverse_object_root: *reverse_object_root,
                    },
                )?;
            }
            Layout::DifferentialNTupleOmitPrefix {
                delimiter,
                tuple_segment_sizes,
                full_identifier_as_object_root,
            } => {
                validate_differential_omit_prefix(extension, delimiter)?;
                write_extension_config(
                    &root,
                    extension,
                    &DifferentialNTupleOmitPrefixConfig {
                        extension_name: extension.to_string(),
                        delimiter: delimiter.clone(),
                        tuple_segment_sizes: tuple_segment_sizes.clone(),
                        full_identifier_as_object_root: *full_identifier_as_object_root,
                    },
                )?;
            }
            Layout::HashNoPrefixIdNTuple {
                algorithm,
                tuple_size,
                number_of_tuples,
                delimiters,
            } => {
                validate_tuple_bounds(extension, algorithm, *tuple_size, *number_of_tuples)?;
                validate_delimiters(extension, delimiters)?;
                write_extension_config(
                    &root,
                    extension,
                    &HashNoPrefixIdNTupleConfig {
                        extension_name: extension.to_string(),
                        digest_algorithm: algorithm.clone(),
                        tuple_size: *tuple_size,
                        number_of_tuples: *number_of_tuples,
                        delimiters: delimiters.clone(),
                    },
                )?;
            }
        }
        let layout_file = LayoutFile {
            extension: extension.to_string(),
            description: layout.description(),
        };
        let layout_path = root.join("ocfl_layout.json");
        let bytes = serde_json::to_vec_pretty(&layout_file).map_err(|source| FsError::Parse {
            path: layout_path.clone(),
            source,
        })?;
        std::fs::write(&layout_path, bytes).map_err(|error| io(&layout_path, error))?;
        Ok(Self { root, layout })
    }

    /// Open a storage root and read its layout from `ocfl_layout.json` (+ the
    /// extension config where the layout is parameterised).
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, FsError> {
        let root = root.into();
        let layout_path = root.join("ocfl_layout.json");
        let bytes = std::fs::read(&layout_path).map_err(|error| io(&layout_path, error))?;
        let layout_file: LayoutFile =
            serde_json::from_slice(&bytes).map_err(|source| FsError::Parse {
                path: layout_path,
                source,
            })?;
        let extension = layout_file.extension.as_str();
        let layout = match extension {
            "0002-flat-direct-storage-layout" => Layout::FlatDirect,
            "0003-hash-and-id-n-tuple-storage-layout" => {
                let config: HashIdNTupleConfig = read_extension_config(&root, extension)?;
                check_extension_name(extension, &config.extension_name)?;
                validate_tuple_bounds(
                    extension,
                    &config.digest_algorithm,
                    config.tuple_size,
                    config.number_of_tuples,
                )?;
                Layout::HashIdNTuple {
                    algorithm: config.digest_algorithm,
                    tuple_size: config.tuple_size,
                    number_of_tuples: config.number_of_tuples,
                }
            }
            "0004-hashed-n-tuple-storage-layout" => {
                let config: HashedNTupleConfig = read_extension_config(&root, extension)?;
                check_extension_name(extension, &config.extension_name)?;
                validate_tuple_bounds(
                    extension,
                    &config.digest_algorithm,
                    config.tuple_size,
                    config.number_of_tuples,
                )?;
                validate_short_object_root(
                    extension,
                    &config.digest_algorithm,
                    config.tuple_size,
                    config.number_of_tuples,
                    config.short_object_root,
                )?;
                Layout::HashedNTuple {
                    algorithm: config.digest_algorithm,
                    tuple_size: config.tuple_size,
                    number_of_tuples: config.number_of_tuples,
                    short_object_root: config.short_object_root,
                }
            }
            "0006-flat-omit-prefix-storage-layout" => {
                let config: OmitPrefixConfig = read_extension_config(&root, extension)?;
                check_extension_name(extension, &config.extension_name)?;
                if config.delimiter.is_empty() {
                    return Err(FsError::Other(format!("{extension}: delimiter is empty")));
                }
                Layout::OmitPrefix {
                    delimiter: config.delimiter,
                }
            }
            "0007-n-tuple-omit-prefix-storage-layout" => {
                let config: NTupleOmitPrefixConfig = read_extension_config(&root, extension)?;
                check_extension_name(extension, &config.extension_name)?;
                validate_n_tuple_omit_prefix(
                    extension,
                    &config.delimiter,
                    config.tuple_size,
                    config.number_of_tuples,
                )?;
                Layout::NTupleOmitPrefix {
                    delimiter: config.delimiter,
                    tuple_size: config.tuple_size,
                    number_of_tuples: config.number_of_tuples,
                    zero_padding: config.zero_padding,
                    reverse_object_root: config.reverse_object_root,
                }
            }
            "0010-differential-n-tuple-omit-prefix-storage-layout" => {
                let config: DifferentialNTupleOmitPrefixConfig =
                    read_extension_config(&root, extension)?;
                check_extension_name(extension, &config.extension_name)?;
                validate_differential_omit_prefix(extension, &config.delimiter)?;
                Layout::DifferentialNTupleOmitPrefix {
                    delimiter: config.delimiter,
                    tuple_segment_sizes: config.tuple_segment_sizes,
                    full_identifier_as_object_root: config.full_identifier_as_object_root,
                }
            }
            "0012-hash-and-no-prefix-id-n-tuple-storage-layout" => {
                let config: HashNoPrefixIdNTupleConfig = read_extension_config(&root, extension)?;
                check_extension_name(extension, &config.extension_name)?;
                validate_tuple_bounds(
                    extension,
                    &config.digest_algorithm,
                    config.tuple_size,
                    config.number_of_tuples,
                )?;
                validate_delimiters(extension, &config.delimiters)?;
                Layout::HashNoPrefixIdNTuple {
                    algorithm: config.digest_algorithm,
                    tuple_size: config.tuple_size,
                    number_of_tuples: config.number_of_tuples,
                    delimiters: config.delimiters,
                }
            }
            other => {
                return Err(FsError::Other(format!(
                    "unsupported storage layout extension {other:?}"
                )));
            }
        };
        Ok(Self { root, layout })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// The object directory for `id` per the layout.
    pub fn object_path(&self, id: &str) -> Result<PathBuf, FsError> {
        Ok(self.root.join(self.layout.object_dirname(id)?))
    }

    /// Create the object for `id` (the directory is written by the writer's
    /// first commit).
    pub fn writer(&self, id: &str, algorithm: ContentAlgorithm) -> Result<ObjectWriter, FsError> {
        ObjectWriter::create(self.object_path(id)?, id, algorithm)
    }

    /// Open the object for `id`.
    pub fn object(&self, id: &str) -> Result<Object, FsError> {
        Object::open(self.object_path(id)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omit_prefix_takes_everything_after_the_last_delimiter() {
        let layout = Layout::OmitPrefix {
            delimiter: ":".into(),
        };
        assert_eq!(layout.object_dirname("urn:mugz:0001").unwrap(), "0001");
        assert_eq!(
            layout
                .object_dirname("urn:uuid:6e8bc430-9c3a-11d9-9669-0800200c9a66")
                .unwrap(),
            "6e8bc430-9c3a-11d9-9669-0800200c9a66"
        );
    }

    #[test]
    fn omit_prefix_rejects_missing_delimiters_and_unsafe_names() {
        let layout = Layout::OmitPrefix {
            delimiter: ":".into(),
        };
        assert!(layout.object_dirname("no-delimiter-here").is_err());
        assert!(layout.object_dirname("urn:test:..").is_err());
        assert!(layout.object_dirname("urn:test:a/b").is_err());
        assert!(layout.object_dirname("urn:test:").is_err());
    }

    #[test]
    fn flat_direct_is_passthrough() {
        assert_eq!(Layout::FlatDirect.object_dirname("0001").unwrap(), "0001");
    }

    fn hash_id(algorithm: DigestAlgorithm, tuple_size: u32, number_of_tuples: u32) -> Layout {
        Layout::HashIdNTuple {
            algorithm,
            tuple_size,
            number_of_tuples,
        }
    }

    #[test]
    fn hash_id_n_tuple_matches_the_extension_vectors() {
        let layout = hash_id(DigestAlgorithm::Sha256, 3, 3);
        assert_eq!(
            layout.object_dirname("object-01").unwrap(),
            "3c0/ff4/240/object-01"
        );
        assert_eq!(
            layout.object_dirname("..hor/rib:le-$id").unwrap(),
            "487/326/d8c/%2e%2ehor%2frib%3ale-%24id"
        );
        assert_eq!(
            layout.object_dirname("..Hor/rib:lè-$id").unwrap(),
            "373/529/21a/%2e%2eHor%2frib%3al%c3%a8-%24id"
        );
        let md5 = hash_id(DigestAlgorithm::Md5, 2, 15);
        assert_eq!(
            md5.object_dirname("object-01").unwrap(),
            "ff/75/53/44/92/48/5e/ab/b3/9f/86/35/67/28/88/object-01"
        );
        assert_eq!(
            md5.object_dirname("..hor/rib:le-$id").unwrap(),
            "08/31/97/66/fb/6c/29/35/dd/17/5b/94/26/77/17/%2e%2ehor%2frib%3ale-%24id"
        );
        assert_eq!(
            hash_id(DigestAlgorithm::Md5, 5, 2)
                .object_dirname("object-01")
                .unwrap(),
            "ff755/34492/object-01"
        );
        assert_eq!(
            hash_id(DigestAlgorithm::Md5, 0, 0)
                .object_dirname("object-01")
                .unwrap(),
            "object-01"
        );
        // Encapsulation names over 100 characters are cut to 100 and
        // disambiguated with `-<digest>` of the whole id.
        let long = "abcdefghij".repeat(26);
        assert_eq!(
            layout.object_dirname(&long).unwrap(),
            format!(
                "55b/432/806/{}-55b432806f4e270da0cf23815ed338742179002153cd8d896f23b3e2d8a14359",
                "abcdefghij".repeat(10)
            )
        );
        let long_101 = format!("{}a", "abcdefghij".repeat(10));
        assert_eq!(
            layout.object_dirname(&long_101).unwrap(),
            format!(
                "5cc/73e/648/{}-5cc73e648fbcff136510e330871180922ddacf193b68fdeff855683a01464220",
                "abcdefghij".repeat(10)
            )
        );
    }

    #[test]
    fn hashed_n_tuple_matches_the_extension_vectors() {
        let layout = Layout::HashedNTuple {
            algorithm: DigestAlgorithm::Sha256,
            tuple_size: 3,
            number_of_tuples: 3,
            short_object_root: false,
        };
        assert_eq!(
            layout.object_dirname("object-01").unwrap(),
            "3c0/ff4/240/3c0ff4240c1e116dba14c7627f2319b58aa3d77606d0d90dfc6161608ac987d4"
        );
        let short = Layout::HashedNTuple {
            algorithm: DigestAlgorithm::Md5,
            tuple_size: 2,
            number_of_tuples: 15,
            short_object_root: true,
        };
        assert_eq!(
            short.object_dirname("object-01").unwrap(),
            "ff/75/53/44/92/48/5e/ab/b3/9f/86/35/67/28/88/4e"
        );
        assert_eq!(
            short.object_dirname("..hor/rib:le-$id").unwrap(),
            "08/31/97/66/fb/6c/29/35/dd/17/5b/94/26/77/17/e0"
        );
        let no_tuples = Layout::HashedNTuple {
            algorithm: DigestAlgorithm::Sha256,
            tuple_size: 0,
            number_of_tuples: 0,
            short_object_root: false,
        };
        assert_eq!(
            no_tuples.object_dirname("object-01").unwrap(),
            "3c0ff4240c1e116dba14c7627f2319b58aa3d77606d0d90dfc6161608ac987d4"
        );
        // shortObjectRoot with the whole digest in tuples has nothing left to
        // name the root with: an error, not an empty segment.
        let contradictory = Layout::HashedNTuple {
            algorithm: DigestAlgorithm::Sha512,
            tuple_size: 32,
            number_of_tuples: 4,
            short_object_root: true,
        };
        assert!(contradictory.object_dirname("object-01").is_err());
    }

    #[test]
    fn n_tuple_omit_prefix_matches_the_extension_vectors() {
        let layout = Layout::NTupleOmitPrefix {
            delimiter: ":".into(),
            tuple_size: 4,
            number_of_tuples: 2,
            zero_padding: ZeroPadding::Left,
            reverse_object_root: true,
        };
        assert_eq!(
            layout.object_dirname("namespace:12887296").unwrap(),
            "6927/8821/12887296"
        );
        assert_eq!(
            layout
                .object_dirname("urn:uuid:6e8bc430-9c3a-11d9-9669-0800200c9a66")
                .unwrap(),
            "66a9/c002/6e8bc430-9c3a-11d9-9669-0800200c9a66"
        );
        assert_eq!(layout.object_dirname("abc123").unwrap(), "321c/ba00/abc123");

        let right = Layout::NTupleOmitPrefix {
            delimiter: "edu/".into(),
            tuple_size: 3,
            number_of_tuples: 3,
            zero_padding: ZeroPadding::Right,
            reverse_object_root: false,
        };
        assert_eq!(
            right
                .object_dirname("https://institution.edu/3448793")
                .unwrap(),
            "344/879/300/3448793"
        );
        assert_eq!(
            right
                .object_dirname("https://institution.edu/abc/edu/f8.05v")
                .unwrap(),
            "f8./05v/000/f8.05v"
        );

        // The delimiter match is case-insensitive; an id it ends is an error.
        assert_eq!(
            right.object_dirname("urn:abc/EDU/x").unwrap(),
            "x00/000/000/x"
        );
        assert!(right.object_dirname("urn:abc/edu/").is_err());
        assert!(right.object_dirname("urn:éabc").is_err());
    }

    #[test]
    fn differential_n_tuple_omit_prefix_matches_the_extension_vectors() {
        let layout = Layout::DifferentialNTupleOmitPrefix {
            delimiter: ":".into(),
            tuple_segment_sizes: vec![2, 3, 2, 4],
            full_identifier_as_object_root: false,
        };
        assert_eq!(
            layout.object_dirname("druid:gh875jh5489").unwrap(),
            "gh/875/jh/5489"
        );
        assert_eq!(
            layout.object_dirname("namespace:11887296672").unwrap(),
            "11/887/29/6672"
        );
        assert_eq!(
            layout.object_dirname("urn:nbn:fi:111-0023815").unwrap(),
            "11/1-0/02/3815"
        );
        assert_eq!(
            layout.object_dirname("abc123xyz89").unwrap(),
            "ab/c12/3x/yz89"
        );

        let full = Layout::DifferentialNTupleOmitPrefix {
            delimiter: "edu/".into(),
            tuple_segment_sizes: vec![3, 4],
            full_identifier_as_object_root: true,
        };
        assert_eq!(
            full.object_dirname("https://institution.edu/3448793")
                .unwrap(),
            "344/8793/3448793"
        );
        assert_eq!(
            full.object_dirname("https://institution.edu/abc/edu/f8a905v")
                .unwrap(),
            "f8a/905v/f8a905v"
        );

        // The prefix-omitted id must exactly fill the segments.
        assert!(layout.object_dirname("druid:gh875jh548").is_err());
    }

    #[test]
    fn hash_no_prefix_id_n_tuple_matches_the_extension_vectors() {
        let layout = Layout::HashNoPrefixIdNTuple {
            algorithm: DigestAlgorithm::Sha256,
            tuple_size: 3,
            number_of_tuples: 3,
            delimiters: vec![],
        };
        assert_eq!(
            layout.object_dirname("object-01").unwrap(),
            "3c0/ff4/240/object-01"
        );
        assert_eq!(
            layout.object_dirname("..hor/rib:le-$id").unwrap(),
            "487/326/d8c/%2e%2ehor%2frib%3ale-%24id"
        );

        let dash = Layout::HashNoPrefixIdNTuple {
            algorithm: DigestAlgorithm::Sha256,
            tuple_size: 3,
            number_of_tuples: 3,
            delimiters: vec!["-".into()],
        };
        assert_eq!(dash.object_dirname("object-01").unwrap(), "938/db8/c9f/01");

        let md5 = Layout::HashNoPrefixIdNTuple {
            algorithm: DigestAlgorithm::Md5,
            tuple_size: 2,
            number_of_tuples: 15,
            delimiters: vec!["/".into()],
        };
        assert_eq!(
            md5.object_dirname("object-01").unwrap(),
            "ff/75/53/44/92/48/5e/ab/b3/9f/86/35/67/28/88/object-01"
        );
        assert_eq!(
            md5.object_dirname("..hor/rib:le-$id").unwrap(),
            "5d/6e/4e/8c/b5/cd/0c/7a/8f/bf/65/c1/29/51/27/rib%3ale-%24id"
        );

        // A delimiter ending the id is ignored; the previous occurrence — or
        // none — decides the boundary.
        let at_end = Layout::HashNoPrefixIdNTuple {
            algorithm: DigestAlgorithm::Sha256,
            tuple_size: 3,
            number_of_tuples: 3,
            delimiters: vec!["d".into()],
        };
        assert!(at_end.object_dirname("abcd").unwrap().ends_with("/abcd"));
        assert!(at_end.object_dirname("abcdd").unwrap().ends_with("/d"));
        let two = Layout::HashNoPrefixIdNTuple {
            algorithm: DigestAlgorithm::Sha256,
            tuple_size: 3,
            number_of_tuples: 3,
            delimiters: vec!["c".into(), "d".into()],
        };
        assert!(two.object_dirname("abcd").unwrap().ends_with("/d"));
    }

    #[test]
    fn every_layout_round_trips_through_a_storage_root() {
        let layouts = [
            Layout::FlatDirect,
            Layout::HashIdNTuple {
                algorithm: DigestAlgorithm::Sha256,
                tuple_size: 3,
                number_of_tuples: 3,
            },
            Layout::HashedNTuple {
                algorithm: DigestAlgorithm::Md5,
                tuple_size: 2,
                number_of_tuples: 15,
                short_object_root: true,
            },
            Layout::OmitPrefix {
                delimiter: ":".into(),
            },
            Layout::NTupleOmitPrefix {
                delimiter: ":".into(),
                tuple_size: 4,
                number_of_tuples: 2,
                zero_padding: ZeroPadding::Left,
                reverse_object_root: true,
            },
            Layout::DifferentialNTupleOmitPrefix {
                delimiter: ":".into(),
                tuple_segment_sizes: vec![2, 3, 2, 4],
                full_identifier_as_object_root: false,
            },
            Layout::HashNoPrefixIdNTuple {
                algorithm: DigestAlgorithm::Sha256,
                tuple_size: 3,
                number_of_tuples: 3,
                delimiters: vec!["/".into()],
            },
        ];
        for layout in layouts {
            let dir = tempfile::tempdir().unwrap();
            StorageRoot::create(dir.path(), layout.clone()).unwrap();
            let reopened = StorageRoot::open(dir.path()).unwrap();
            assert_eq!(reopened.layout(), &layout);
        }
    }

    #[test]
    fn object_path_joins_the_tuple_directories() {
        let dir = tempfile::tempdir().unwrap();
        let root = StorageRoot::create(dir.path(), hash_id(DigestAlgorithm::Sha256, 3, 3)).unwrap();
        assert_eq!(
            root.object_path("object-01").unwrap(),
            dir.path().join("3c0/ff4/240/object-01")
        );
    }

    #[test]
    fn invalid_layout_parameters_are_rejected() {
        fn create_error(layout: Layout) -> String {
            let dir = tempfile::tempdir().unwrap();
            match StorageRoot::create(dir.path(), layout) {
                Ok(_) => panic!("the layout should not have been accepted"),
                Err(error) => error.to_string(),
            }
        }

        // tupleSize × numberOfTuples beyond the digest.
        let error = create_error(hash_id(DigestAlgorithm::Sha256, 40, 3));
        assert!(error.contains("tupleSize"), "{error}");

        // `size` is not a hash.
        let error = create_error(hash_id(DigestAlgorithm::Size, 3, 3));
        assert!(error.contains("size"), "{error}");

        // A valid root whose config then disagrees with its layout file.
        let dir = tempfile::tempdir().unwrap();
        StorageRoot::create(dir.path(), hash_id(DigestAlgorithm::Sha256, 3, 3)).unwrap();
        let config_path = dir
            .path()
            .join("extensions/0003-hash-and-id-n-tuple-storage-layout/config.json");
        std::fs::write(
            &config_path,
            r#"{ "extensionName": "0003-hash-and-id-n-tuple-storage-layout", "tupleSize": 40, "numberOfTuples": 3 }"#,
        )
        .unwrap();
        assert!(StorageRoot::open(dir.path()).is_err());
        std::fs::write(
            &config_path,
            r#"{ "extensionName": "0004-hashed-n-tuple-storage-layout" }"#,
        )
        .unwrap();
        assert!(StorageRoot::open(dir.path()).is_err());
    }
}
