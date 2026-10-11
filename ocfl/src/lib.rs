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
//! - The `fixity` block (§3.5.4) under the full fixity vocabulary — the spec's
//!   core table plus community extension `0009-digest-algorithms`: `md5`,
//!   `sha1`, `sha256`, `sha512`, `blake2b-160/256/384/512`, `sha512/256` and
//!   `size`. Values under an algorithm this client does not know are ignored,
//!   as the spec requires (§3.4).
//! - Version directories, per-version inventories and their digest sidecars
//!   (§3.3, §3.6, §3.7), with forward-delta writes: a new version stores only
//!   content whose digest is not already in the manifest; everything unchanged
//!   is referenced, never copied.
//! - Storage roots with the registered layout extensions `0002-flat-direct`,
//!   `0003-hash-and-id-n-tuple`, `0004-hashed-n-tuple`,
//!   `0006-flat-omit-prefix`, `0007-n-tuple-omit-prefix`,
//!   `0010-differential-n-tuple-omit-prefix` and
//!   `0012-hash-and-no-prefix-id-n-tuple` (§4, extension params in
//!   `extensions/<name>/config.json`). Each extension's own test vectors are
//!   ported as unit tests.
//! - The `0005-mutable-head` extension: stage version `N+1` under the
//!   extension directory, revise it in place under revision markers, then
//!   commit it into the object root or purge it. Access loads the mutable
//!   inventory while one is active, and new root versions are refused.
//! - The `0008-schema-registry` storage-root extension: one reference copy of
//!   every schema referenced across the root, indexed by
//!   `schema_inventory.json` and verified like object content.
//! - The `0011-direct-clean-path-layout` path-mapping extension
//!   ([`DirectClean`]): logical paths projected to safe content paths, with the
//!   UTF-encoded injective mode, the plain replacement mode, and digest-derived
//!   fallback names for over-long paths.
//! - Full-object verification against the spec's structural rules and the
//!   community validation codes: declaration files, sidecar integrity,
//!   `head` / numbering, path rules, `created` as real RFC 3339, manifest and
//!   fixity values against the files (root and per-version inventories),
//!   state digests exactly matching manifest keys, each version inventory
//!   resolving to the root's content even across a digest-algorithm change,
//!   id and spec-version consistency, and the mutable HEAD against §0005.
//!   The `OCFL/ocfl-fixtures` suite runs when `OCFL_FIXTURES` points at a
//!   checkout (`tests/fixtures.rs`) — currently 12 good / 13 warn / 55 bad
//!   objects with no mismatches.
//!
//! # What is not (yet)
//!
//! - Fixity *emission*: the model, validation and verification cover the
//!   block, but [`fs::ObjectWriter`] does not populate one — the manifest is
//!   the complete set, and additional fixity values are optional (§3.5.4).
//!
//! # Features
//!
//! `fs` (default) adds [`fs`] — the object writer/reader, verification and
//! storage-root helpers. Without it the crate is the pure model + planner
//! ([`Inventory`], [`ContentAlgorithm`], [`DigestAlgorithm`], [`plan_version`]),
//! so a Worker or a wasm host can speak the same bytes through its own storage
//! backend.
//!
//! ```no_run
//! use ocfl::fs::ObjectWriter;
//! use ocfl::{ContentAlgorithm, LogicalState};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut writer = ObjectWriter::create("/tmp/token-0001", "urn:mugz:0001", ContentAlgorithm::Sha512)?;
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
mod pathmap;
mod plan;
mod time;

#[cfg(feature = "fs")]
pub mod fs;

pub use digest::{ContentAlgorithm, DigestAlgorithm, digest_eq};
pub use inventory::{
    DEFAULT_CONTENT_DIRECTORY, INVENTORY_FILENAME, INVENTORY_TYPE_1_1, Inventory, InventoryContext,
    MUTABLE_HEAD_EXTENSION, OBJECT_DECL_CONTENTS, OBJECT_DECL_FILENAME, Problems,
    ROOT_DECL_CONTENTS, ROOT_DECL_FILENAME, User, Version, parse_sidecar, sidecar_contents,
    sidecar_name, version_dir, version_number,
};
pub use pathmap::{DirectClean, DirectCleanConfig, PathMapError};
pub use plan::{LogicalState, PlanError, VersionPlan, plan_version};
pub use time::{is_rfc3339_datetime, rfc3339_utc};
