//! `0008-schema-registry`: one reference copy of every schema referenced by an
//! OCFL storage root's objects, kept in the extension directory and indexed by
//! `schema_inventory.json`.
//!
//! The registry is a storage-root extension: `[_storage_root_]/extensions/0008-schema-registry/`
//! holds `config.json`, `schemata/` (the stored copies), `schema_inventory.json`
//! and its digest sidecar. A schema's stored filename is the digest of its
//! identifier under `identifierDigestAlgorithm` (default `md5`); the inventory
//! entry records the identifier and the file's own digest under the registry's
//! `digestAlgorithm`, so integrity is verifiable the same way an object's is.
//!
//! Registering an identifier that is already present with the same identifier
//! is a no-op; the same filename under a *different* identifier is a hash
//! collision and raises.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::digest::DigestAlgorithm;
use crate::inventory::{Problems, parse_sidecar, sidecar_contents};

use super::{FsError, io};

/// The registered name of the extension.
pub const SCHEMA_REGISTRY_EXTENSION: &str = "0008-schema-registry";
/// The registry index file.
const INVENTORY_FILENAME: &str = "schema_inventory.json";

fn default_md5() -> DigestAlgorithm {
    DigestAlgorithm::Md5
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchemaRegistryConfig {
    extension_name: String,
    #[serde(default = "default_md5")]
    identifier_digest_algorithm: DigestAlgorithm,
    /// Absent means "the algorithm the objects use"; the registry cannot see
    /// those, so [`SchemaRegistry::open`] resolves it to `sha512` (the spec's
    /// `SHOULD`) and says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    digest_algorithm: Option<DigestAlgorithm>,
}

/// One registered schema: the stored file's digest and the identifier it was
/// stored under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaEntry {
    pub digest: String,
    pub identifier: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SchemaInventory {
    manifest: BTreeMap<String, SchemaEntry>,
}

/// A storage root's schema registry.
pub struct SchemaRegistry {
    directory: PathBuf,
    identifier_algorithm: DigestAlgorithm,
    digest_algorithm: DigestAlgorithm,
    inventory: SchemaInventory,
}

fn extension_dir(storage_root: &Path) -> PathBuf {
    storage_root
        .join("extensions")
        .join(SCHEMA_REGISTRY_EXTENSION)
}

fn inventory_sidecar_name(algorithm: DigestAlgorithm) -> String {
    format!("{INVENTORY_FILENAME}.{}", algorithm.as_str())
}

/// The algorithms must be computable hashes — `size` is a byte count, not a
/// digest, and unknown names cannot be computed at all.
fn require_hash(algorithm: &DigestAlgorithm, role: &str) -> Result<(), FsError> {
    if algorithm.hex_len().is_some() {
        Ok(())
    } else {
        Err(FsError::Other(format!(
            "{SCHEMA_REGISTRY_EXTENSION}: {role} {algorithm} is not a fixed-length hash"
        )))
    }
}

impl SchemaRegistry {
    /// Create a registry under a storage root, using `digest_algorithm` for
    /// the stored schemata's fixity (the algorithm the objects use).
    pub fn create(storage_root: &Path, digest_algorithm: DigestAlgorithm) -> Result<Self, FsError> {
        require_hash(&digest_algorithm, "digestAlgorithm")?;
        let directory = extension_dir(storage_root);
        let schemata = directory.join("schemata");
        std::fs::create_dir_all(&schemata).map_err(|error| io(&schemata, error))?;
        let config = SchemaRegistryConfig {
            extension_name: SCHEMA_REGISTRY_EXTENSION.to_string(),
            identifier_digest_algorithm: DigestAlgorithm::Md5,
            digest_algorithm: Some(digest_algorithm.clone()),
        };
        write_config(&directory, &config)?;
        let registry = Self {
            directory,
            identifier_algorithm: config.identifier_digest_algorithm,
            digest_algorithm,
            inventory: SchemaInventory {
                manifest: BTreeMap::new(),
            },
        };
        registry.write_inventory()?;
        Ok(registry)
    }

    /// Open a storage root's schema registry.
    pub fn open(storage_root: &Path) -> Result<Self, FsError> {
        let directory = extension_dir(storage_root);
        let config_path = directory.join("config.json");
        let bytes = std::fs::read(&config_path).map_err(|error| io(&config_path, error))?;
        let config: SchemaRegistryConfig =
            serde_json::from_slice(&bytes).map_err(|source| FsError::Parse {
                path: config_path,
                source,
            })?;
        if config.extension_name != SCHEMA_REGISTRY_EXTENSION {
            return Err(FsError::Other(format!(
                "extension config names {:?}, expected {SCHEMA_REGISTRY_EXTENSION}",
                config.extension_name
            )));
        }
        require_hash(
            &config.identifier_digest_algorithm,
            "identifierDigestAlgorithm",
        )?;
        // The spec's default is "the same value used elsewhere in the OCFL";
        // unreachable from the root alone, so resolve it to the spec's SHOULD.
        let digest_algorithm = config
            .digest_algorithm
            .clone()
            .unwrap_or(DigestAlgorithm::Sha512);
        require_hash(&digest_algorithm, "digestAlgorithm")?;

        let path = directory.join(INVENTORY_FILENAME);
        let bytes = std::fs::read(&path).map_err(|error| io(&path, error))?;
        let inventory: SchemaInventory =
            serde_json::from_slice(&bytes).map_err(|source| FsError::Parse {
                path: path.clone(),
                source,
            })?;
        let sidecar_path = directory.join(inventory_sidecar_name(digest_algorithm.clone()));
        let sidecar =
            std::fs::read_to_string(&sidecar_path).map_err(|error| io(&sidecar_path, error))?;
        let stated = parse_sidecar(&sidecar).ok_or_else(|| {
            FsError::Other(format!(
                "{}: malformed sidecar contents",
                sidecar_path.display()
            ))
        })?;
        let recomputed = digest_algorithm
            .digest(&bytes)
            .ok_or_else(|| FsError::Other("schema digest unavailable".into()))?;
        if stated != recomputed {
            return Err(FsError::Other(format!(
                "{}: sidecar digest {stated} does not match the inventory's {recomputed}",
                sidecar_path.display()
            )));
        }
        Ok(Self {
            directory,
            identifier_algorithm: config.identifier_digest_algorithm,
            digest_algorithm,
            inventory,
        })
    }

    /// The stored filename a schema identifier hashes to.
    pub fn filename_for(&self, identifier: &str) -> Result<String, FsError> {
        self.identifier_algorithm
            .digest(identifier.as_bytes())
            .ok_or_else(|| FsError::Other("identifier digest unavailable".into()))
    }

    /// The registered schemata, by stored filename.
    pub fn manifest(&self) -> &BTreeMap<String, SchemaEntry> {
        &self.inventory.manifest
    }

    /// The entry registered for `identifier`, if any.
    pub fn lookup(&self, identifier: &str) -> Result<Option<&SchemaEntry>, FsError> {
        Ok(self.inventory.manifest.get(&self.filename_for(identifier)?))
    }

    /// Register `bytes` under `identifier`, storing them if new. Returns
    /// `true` when the schema was stored, `false` when it was already present.
    /// The same filename under a different identifier is a hash collision and
    /// raises.
    pub fn register(&mut self, identifier: &str, bytes: &[u8]) -> Result<bool, FsError> {
        let filename = self.filename_for(identifier)?;
        if let Some(entry) = self.inventory.manifest.get(&filename) {
            if entry.identifier == identifier {
                return Ok(false);
            }
            return Err(FsError::Other(format!(
                "schema registry collision: {filename} is registered for {:?} but {identifier:?} hashes to the same name",
                entry.identifier
            )));
        }
        let digest = self
            .digest_algorithm
            .digest(bytes)
            .ok_or_else(|| FsError::Other("schema digest unavailable".into()))?;
        let path = self.directory.join("schemata").join(&filename);
        std::fs::write(&path, bytes).map_err(|error| io(&path, error))?;
        self.inventory.manifest.insert(
            filename,
            SchemaEntry {
                digest,
                identifier: identifier.to_string(),
            },
        );
        self.write_inventory()?;
        Ok(true)
    }

    /// Read a registered schema's stored bytes.
    pub fn read(&self, identifier: &str) -> Result<Vec<u8>, FsError> {
        let filename = self.filename_for(identifier)?;
        if !self.inventory.manifest.contains_key(&filename) {
            return Err(FsError::Other(format!(
                "schema {identifier:?} is not registered"
            )));
        }
        let path = self.directory.join("schemata").join(&filename);
        std::fs::read(&path).map_err(|error| io(&path, error))
    }

    /// Check every registered schema: the filename is the identifier's digest,
    /// the stored file exists and hashes to its recorded digest, and nothing
    /// in `schemata/` is unregistered.
    pub fn verify(&self) -> Problems {
        let mut problems = Problems::default();
        for (filename, entry) in &self.inventory.manifest {
            match self
                .identifier_algorithm
                .digest(entry.identifier.as_bytes())
            {
                Some(expected) if expected == *filename => {}
                Some(expected) => problems.error(format!(
                    "{filename}: the entry's identifier {:?} hashes to {expected}",
                    entry.identifier
                )),
                None => problems.error(format!(
                    "{filename}: the identifier digest algorithm is unavailable"
                )),
            }
            let path = self.directory.join("schemata").join(filename);
            match std::fs::read(&path) {
                Ok(bytes) => match self.digest_algorithm.digest(&bytes) {
                    Some(got) if got == entry.digest => {}
                    Some(got) => problems.error(format!(
                        "{filename}: digest {got} does not match the registry's {}",
                        entry.digest
                    )),
                    None => {
                        problems.error(format!("{filename}: the digest algorithm is unavailable"))
                    }
                },
                Err(error) => problems.error(format!("{filename}: unreadable ({error})")),
            }
        }
        let schemata = self.directory.join("schemata");
        if let Ok(entries) = std::fs::read_dir(&schemata) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !self.inventory.manifest.contains_key(&name) {
                    problems.error(format!(
                        "schemata/{name}: file is not registered in {INVENTORY_FILENAME}"
                    ));
                }
            }
        }
        problems
    }

    fn write_inventory(&self) -> Result<(), FsError> {
        let path = self.directory.join(INVENTORY_FILENAME);
        let bytes =
            serde_json::to_vec_pretty(&self.inventory).map_err(|source| FsError::Parse {
                path: path.clone(),
                source,
            })?;
        std::fs::write(&path, &bytes).map_err(|error| io(&path, error))?;
        let digest = self
            .digest_algorithm
            .digest(&bytes)
            .ok_or_else(|| FsError::Other("schema digest unavailable".into()))?;
        let sidecar_path = self
            .directory
            .join(inventory_sidecar_name(self.digest_algorithm.clone()));
        std::fs::write(&sidecar_path, sidecar_contents(&digest))
            .map_err(|error| io(&sidecar_path, error))
    }
}

fn write_config(directory: &Path, config: &SchemaRegistryConfig) -> Result<(), FsError> {
    let path = directory.join("config.json");
    let bytes = serde_json::to_vec_pretty(config).map_err(|source| FsError::Parse {
        path: path.clone(),
        source,
    })?;
    std::fs::write(&path, bytes).map_err(|error| io(&path, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spec_registry_example_filenames_are_reproduced() {
        let dir = tempfile::tempdir().unwrap();
        let registry = SchemaRegistry::create(dir.path(), DigestAlgorithm::Sha512).unwrap();
        assert_eq!(
            registry
                .filename_for(
                    "http://dublincore.org/specifications/dublin-core/dcmes-xml/2001-04-11/dcmes-xml-dtd.dtd"
                )
                .unwrap(),
            "40cdd53d9a263e5466b8954d82d23daa"
        );
        assert_eq!(
            registry
                .filename_for(
                    "http://schemata.hasdai.org/historic-persons/historic-person-entry-v1.0.0.json"
                )
                .unwrap(),
            "95d751340dcdc784fd759dbc7ddb9633"
        );
    }

    #[test]
    fn schemas_register_read_and_verify() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = SchemaRegistry::create(dir.path(), DigestAlgorithm::Sha512).unwrap();
        let dtd = b"<!ELEMENT rdf:RDF ANY>";
        assert!(registry.register("http://example.org/dc.dtd", dtd).unwrap());
        assert!(!registry.register("http://example.org/dc.dtd", dtd).unwrap());
        assert_eq!(registry.read("http://example.org/dc.dtd").unwrap(), dtd);
        let problems = registry.verify();
        assert!(problems.is_ok(), "{:#?}", problems.errors);

        // Reopening reads the same state back from disk.
        let reopened = SchemaRegistry::open(dir.path()).unwrap();
        assert_eq!(reopened.manifest().len(), 1);
        assert_eq!(reopened.read("http://example.org/dc.dtd").unwrap(), dtd);
        assert!(reopened.verify().is_ok());

        // A tampered stored file is caught.
        let filename = reopened.filename_for("http://example.org/dc.dtd").unwrap();
        std::fs::write(
            dir.path()
                .join("extensions/0008-schema-registry/schemata")
                .join(&filename),
            b"tampered",
        )
        .unwrap();
        let problems = reopened.verify();
        assert!(!problems.is_ok(), "{:#?}", problems.errors);
    }

    #[test]
    fn a_hash_collision_raises() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = SchemaRegistry::create(dir.path(), DigestAlgorithm::Sha512).unwrap();
        registry
            .register("http://example.org/a.json", b"a")
            .unwrap();
        // Same filename, different identifier: rewrite the entry.
        let filename = registry.filename_for("http://example.org/a.json").unwrap();
        let mut manifest = registry.manifest().clone();
        manifest.get_mut(&filename).unwrap().identifier = "http://elsewhere.example/b.json".into();
        registry.inventory.manifest = manifest;
        registry.write_inventory().unwrap();
        let mut reopened = SchemaRegistry::open(dir.path()).unwrap();
        let error = reopened
            .register("http://example.org/a.json", b"a")
            .unwrap_err()
            .to_string();
        assert!(error.contains("collision"), "{error}");
    }
}
