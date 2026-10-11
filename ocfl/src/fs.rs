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

use crate::digest::{DigestAlgorithm, digest_eq};
use crate::inventory::{
    INVENTORY_FILENAME, Inventory, OBJECT_DECL_CONTENTS, OBJECT_DECL_FILENAME, ROOT_DECL_CONTENTS,
    ROOT_DECL_FILENAME, User, parse_sidecar, sidecar_name, version_number,
};
use crate::plan::{LogicalState, PlanError, VersionPlan, plan_version};

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
    algorithm: DigestAlgorithm,
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
        algorithm: DigestAlgorithm,
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

    /// Commit `state` as the next version, returning its directory name.
    ///
    /// Files whose digest is already stored are referenced, never copied; new
    /// content is written under the new version's content directory. The root
    /// inventory is rewritten last.
    pub fn commit(
        &mut self,
        state: &LogicalState,
        created: impl Into<String>,
        message: Option<String>,
        user: Option<User>,
    ) -> Result<String, FsError> {
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

/// Read an object's inventory, resolve its state, and verify it.
pub struct Object {
    root: PathBuf,
    inventory: Inventory,
}

impl Object {
    /// Open an object: parses the root inventory and checks its sidecar. A
    /// structurally non-conforming object still opens — [`Object::verify`]
    /// reports what is wrong.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, FsError> {
        let root = root.into();
        let (inventory, _) = read_inventory_at(&root)?;
        Ok(Self { root, inventory })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn inventory(&self) -> &Inventory {
        &self.inventory
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

    /// Check the whole object against OCFL 1.1's structural and fixity rules.
    pub fn verify(&self) -> VerifyReport {
        verify_object(&self.root, &self.inventory)
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

/// Registered storage layouts this crate can map object ids through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layout {
    /// Extension `0002-flat-direct-storage-layout`: objects are direct children
    /// and the object id is the directory name.
    FlatDirect,
    /// Extension `0006-flat-omit-prefix-storage-layout`: everything up to and
    /// including the *last* `delimiter` is omitted from the directory name —
    /// `urn:mugz:0001` with `:` maps to `0001`.
    OmitPrefix { delimiter: String },
}

impl Layout {
    pub const fn extension_name(&self) -> &'static str {
        match self {
            Self::FlatDirect => "0002-flat-direct-storage-layout",
            Self::OmitPrefix { .. } => "0006-flat-omit-prefix-storage-layout",
        }
    }

    /// The direct-child directory name for `id` under the storage root.
    pub fn object_dirname(&self, id: &str) -> Result<String, FsError> {
        let name = match self {
            Self::FlatDirect => id.to_string(),
            Self::OmitPrefix { delimiter } => {
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
                id[at + length..].to_string()
            }
        };
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.contains('/')
            || name.contains('\\')
        {
            return Err(FsError::Other(format!(
                "id {id:?} maps to a directory name that is not a safe path element: {name:?}"
            )));
        }
        Ok(name)
    }
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
        if let Layout::OmitPrefix { delimiter } = &layout {
            if delimiter.is_empty() {
                return Err(FsError::Other("storage layout delimiter is empty".into()));
            }
            let config = OmitPrefixConfig {
                extension_name: layout.extension_name().to_string(),
                delimiter: delimiter.clone(),
            };
            let config_path = root
                .join("extensions")
                .join(layout.extension_name())
                .join("config.json");
            let parent = config_path.parent().expect("config path has a parent");
            std::fs::create_dir_all(parent).map_err(|error| io(parent, error))?;
            let bytes = serde_json::to_vec_pretty(&config).map_err(|source| FsError::Parse {
                path: config_path.clone(),
                source,
            })?;
            std::fs::write(&config_path, bytes).map_err(|error| io(&config_path, error))?;
        }
        let layout_file = LayoutFile {
            extension: layout.extension_name().to_string(),
            description: format!(
                "{} — objects are direct children of this root",
                layout.extension_name()
            ),
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
        let layout = match layout_file.extension.as_str() {
            "0002-flat-direct-storage-layout" => Layout::FlatDirect,
            "0006-flat-omit-prefix-storage-layout" => {
                let config_path = root
                    .join("extensions")
                    .join("0006-flat-omit-prefix-storage-layout")
                    .join("config.json");
                let bytes = std::fs::read(&config_path).map_err(|error| io(&config_path, error))?;
                let config: OmitPrefixConfig =
                    serde_json::from_slice(&bytes).map_err(|source| FsError::Parse {
                        path: config_path,
                        source,
                    })?;
                if config.extension_name != "0006-flat-omit-prefix-storage-layout" {
                    return Err(FsError::Other(format!(
                        "extension config names {:?}, expected 0006-flat-omit-prefix-storage-layout",
                        config.extension_name
                    )));
                }
                Layout::OmitPrefix {
                    delimiter: config.delimiter,
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
    pub fn writer(&self, id: &str, algorithm: DigestAlgorithm) -> Result<ObjectWriter, FsError> {
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
}
