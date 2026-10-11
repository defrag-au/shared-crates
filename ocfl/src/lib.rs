//! `ocfl` — OCFL 1.1 objects: a typed inventory model, structural validation,
//! forward-delta versioning, and filesystem emit/verify.
//!
//! OCFL ("Oxford Common File Layout") is a preservation-oriented container
//! format: a sequence of immutable, content-addressed version directories under
//! an object root, plus **one mutable inventory** whose `head` names the current
//! version. It is the storage substrate of the living-token standard (see the
//! compositor's `docs/living-token-bundle.md`): a token is one OCFL object,
//! `v1` is the minted state, every later version is an owner update, and the
//! sha512 of `v1/inventory.json` is what a chain record anchors.
//!
//! # What is implemented
//!
//! - Content addressing with `sha512` / `sha256` — the two algorithms OCFL 1.1
//!   permits for an object's `digestAlgorithm` (§3.4).
//! - Version directories, per-version inventories and their digest sidecars
//!   (§3.3, §3.6, §3.7), with forward-delta writes: a new version stores only
//!   content whose digest is not already in the manifest; everything unchanged
//!   is referenced, never copied.
//! - Storage roots with the registered layout extensions
//!   `0002-flat-direct-storage-layout` and `0006-flat-omit-prefix-storage-layout`
//!   (§4, extension params in `extensions/<name>/config.json`).
//! - Full-object verification against the spec's structural rules: declaration
//!   files, sidecar integrity, `head` / numbering, path rules, manifest ↔
//!   filesystem fixity, version-inventory agreement.
//!
//! # What is not (yet)
//!
//! - The `fixity` block (§3.5.4) — we emit exactly one algorithm.
//! - Digest algorithms beyond the core two (extensions 0001 / 0009).
//! - The `0005-mutable-head` extension — reserved for draft/staged states.
//!
//! # Features
//!
//! `fs` (default) adds [`fs`] — the object writer/reader, verification and
//! storage-root helpers. Without it the crate is the pure model + planner
//! ([`Inventory`], [`DigestAlgorithm`], [`plan_version`]), so a Worker or a
//! wasm host can speak the same bytes through its own storage backend.
//!
//! ```no_run
//! use ocfl::fs::ObjectWriter;
//! use ocfl::{DigestAlgorithm, LogicalState};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut writer = ObjectWriter::create("/tmp/token-0001", "urn:mugz:0001", DigestAlgorithm::Sha512)?;
//! let mut state = LogicalState::new();
//! state.insert("0001.webp".into(), std::fs::read("0001.webp")?);
//! writer.commit(&state, "2026-10-11T00:00:00Z", Some("Minted".into()), None)?;
//!
//! let object = ocfl::fs::Object::open("/tmp/token-0001")?;
//! let report = object.verify();
//! assert!(report.is_ok(), "{:#?}", report.issues);
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]

mod digest;
mod inventory;
mod plan;
mod time;

#[cfg(feature = "fs")]
pub mod fs;

pub use digest::{DigestAlgorithm, digest_eq};
pub use inventory::{
    DEFAULT_CONTENT_DIRECTORY, INVENTORY_FILENAME, INVENTORY_TYPE_1_1, Inventory,
    OBJECT_DECL_CONTENTS, OBJECT_DECL_FILENAME, Problems, ROOT_DECL_CONTENTS, ROOT_DECL_FILENAME,
    User, Version, parse_sidecar, sidecar_contents, sidecar_name, version_dir, version_number,
};
pub use plan::{LogicalState, PlanError, VersionPlan, plan_version};
pub use time::{has_rfc3339_shape, rfc3339_utc};
