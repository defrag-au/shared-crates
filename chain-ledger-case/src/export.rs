//! Checksummed export — how a case leaves the machine it was built on.
//!
//! A finding that cannot be handed to someone else and re-checked is not
//! evidence, it is an assertion with citations. So an export is
//! **content-addressed**: the payload serialises canonically, the digest is
//! taken over those exact bytes, and [`verify`] recomputes it. Two exports of
//! the same case produce the same digest; one byte of tampering produces a
//! different one.
//!
//! This mirrors the discipline already applied by hand to archived evidence
//! (`PROVENANCE.md` + `SHA256SUMS.txt` beside the data) — the difference is that
//! here the tool does it, so it cannot be skipped on the day it matters.
//!
//! # Canonical form
//!
//! Digests are only useful if serialisation is deterministic. Records are sorted
//! by primary key and maps are `BTreeMap`, so the bytes depend on the case's
//! *contents* and not on the order rows came back from a query. Without that a
//! re-export of an unchanged case yields a fresh digest and verification means
//! nothing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::records::{ClaimRecord, ClusterRecord, NoteRecord, PartyRecord};
use crate::schema::SCHEMA_VERSION;

/// Everything a case contains, in canonical order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CasePayload {
    pub parties: Vec<PartyRecord>,
    pub clusters: Vec<ClusterRecord>,
    pub claims: Vec<ClaimRecord>,
    pub notes: Vec<NoteRecord>,
    /// Anything the exporter wants to carry that is not modelled — kept sorted
    /// so it cannot perturb the digest.
    pub meta: BTreeMap<String, String>,
}

impl CasePayload {
    /// Sort every collection by its primary key.
    ///
    /// Called before serialising. Query results have no guaranteed order, and an
    /// unsorted payload gives a different digest each export, which quietly
    /// turns verification into a coin flip.
    pub fn canonicalise(&mut self) {
        self.parties.sort_by(|a, b| a.key.cmp(&b.key));
        self.clusters.sort_by(|a, b| a.name.cmp(&b.name));
        self.claims.sort_by(|a, b| a.id.cmp(&b.id));
        self.notes.sort_by(|a, b| a.id.cmp(&b.id));
        for c in &mut self.clusters {
            c.members.sort();
        }
    }

    /// Parties whose identity was asserted with nothing to attribute it to.
    ///
    /// Surfaced on the export rather than left in the data: an export is the
    /// moment a case becomes someone else's input, and an unattributed assertion
    /// travelling as though it were chain data is precisely how the `$187`
    /// figure became load-bearing.
    pub fn unsourced_assertions(&self) -> Vec<&PartyRecord> {
        self.parties
            .iter()
            .filter(|p| p.basis == "asserted" && p.source.as_deref().unwrap_or("").is_empty())
            .collect()
    }
}

/// A case, plus what is needed to check it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseExport {
    pub schema_version: u32,
    pub case_name: String,
    /// Caller-supplied; this crate does no I/O and reads no clock.
    pub exported_at: i64,
    pub payload: CasePayload,
    /// SHA-256 over the canonical serialisation of `payload`, hex-encoded.
    pub digest: String,
    /// How many parties in this export rest on an unattributed assertion.
    /// Recorded so a recipient sees it without having to go looking.
    pub unsourced_assertion_count: usize,
}

/// Serialise a payload the one way that produces a stable digest.
fn canonical_bytes(payload: &CasePayload) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(payload)
}

fn digest_of(payload: &CasePayload) -> Result<String, serde_json::Error> {
    let bytes = canonical_bytes(payload)?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok(format!("{:x}", h.finalize()))
}

/// Package a case for handing over.
///
/// `exported_at` is passed in rather than read from a clock: this crate has no
/// I/O, and a caller-supplied timestamp is also what makes an export
/// reproducible in a test.
pub fn export(
    case_name: impl Into<String>,
    mut payload: CasePayload,
    exported_at: i64,
) -> Result<CaseExport, serde_json::Error> {
    payload.canonicalise();
    let digest = digest_of(&payload)?;
    let unsourced_assertion_count = payload.unsourced_assertions().len();
    Ok(CaseExport {
        schema_version: SCHEMA_VERSION,
        case_name: case_name.into(),
        exported_at,
        payload,
        digest,
        unsourced_assertion_count,
    })
}

/// Why an export could not be accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// Written by a newer version of the tool. Refused rather than partly read.
    SchemaTooNew { found: u32, supported: u32 },
    /// The payload does not hash to the recorded digest.
    DigestMismatch { recorded: String, computed: String },
    /// The payload could not be serialised for hashing.
    Malformed(String),
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::SchemaTooNew { found, supported } => write!(
                f,
                "case was written at schema v{found}; this build supports v{supported}. \
                 Refusing to open it — reading only the recognised fields would silently \
                 drop whatever v{found} added."
            ),
            VerifyError::DigestMismatch { recorded, computed } => write!(
                f,
                "digest mismatch: recorded {recorded}, computed {computed}. The payload \
                 has changed since it was exported."
            ),
            VerifyError::Malformed(e) => write!(f, "case payload could not be serialised: {e}"),
        }
    }
}

impl std::error::Error for VerifyError {}

/// Check an export end to end: schema first, then contents.
///
/// Schema is checked before the digest deliberately — a v2 file may well hash
/// correctly, and "the checksum passed" would then read as "safe to open" when
/// the reader still cannot understand it.
pub fn verify(export: &CaseExport) -> Result<(), VerifyError> {
    if !crate::schema::can_open(export.schema_version) {
        return Err(VerifyError::SchemaTooNew {
            found: export.schema_version,
            supported: SCHEMA_VERSION,
        });
    }
    let mut payload = export.payload.clone();
    payload.canonicalise();
    let computed = digest_of(&payload).map_err(|e| VerifyError::Malformed(e.to_string()))?;
    if computed != export.digest {
        return Err(VerifyError::DigestMismatch {
            recorded: export.digest.clone(),
            computed,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn party(key: &str, basis: &str, source: Option<&str>) -> PartyRecord {
        PartyRecord {
            key: key.into(),
            chain: "cardano".into(),
            has_stake: true,
            label: Some("a wallet".into()),
            basis: basis.into(),
            source: source.map(Into::into),
            note: None,
            updated_at: 0,
        }
    }

    fn payload_of(keys: &[&str]) -> CasePayload {
        CasePayload {
            parties: keys.iter().map(|k| party(k, "observed", None)).collect(),
            ..Default::default()
        }
    }

    /// A round trip must verify, or the export is decorative.
    #[test]
    fn an_untouched_export_verifies() {
        let e = export("mekka", payload_of(&["a", "b"]), 1_000).unwrap();
        assert!(verify(&e).is_ok());
    }

    /// The digest must depend on contents, not on the order a query happened to
    /// return rows. Otherwise re-exporting an unchanged case invalidates it.
    #[test]
    fn digest_is_independent_of_row_order() {
        let a = export("mekka", payload_of(&["a", "b", "c"]), 1_000).unwrap();
        let b = export("mekka", payload_of(&["c", "a", "b"]), 1_000).unwrap();
        assert_eq!(a.digest, b.digest, "canonical order must be enforced");
        assert_eq!(a.payload, b.payload);
    }

    /// One changed field has to be caught.
    #[test]
    fn tampering_is_detected() {
        let mut e = export("mekka", payload_of(&["a"]), 1_000).unwrap();
        e.payload.parties[0].label = Some("something else entirely".into());
        match verify(&e) {
            Err(VerifyError::DigestMismatch { .. }) => {}
            other => panic!("tampering not detected: {other:?}"),
        }
    }

    /// Adding a party is a change, not a free extension.
    #[test]
    fn appending_a_record_is_detected() {
        let mut e = export("mekka", payload_of(&["a"]), 1_000).unwrap();
        e.payload.parties.push(party("z", "observed", None));
        assert!(matches!(
            verify(&e),
            Err(VerifyError::DigestMismatch { .. })
        ));
    }

    /// Schema is checked before the digest: a correctly-hashed future file must
    /// still be refused, and must not report as merely a checksum problem.
    #[test]
    fn a_future_schema_is_refused_even_when_the_digest_is_valid() {
        let mut e = export("mekka", payload_of(&["a"]), 1_000).unwrap();
        e.schema_version = SCHEMA_VERSION + 1;
        match verify(&e) {
            Err(VerifyError::SchemaTooNew { found, supported }) => {
                assert_eq!(found, SCHEMA_VERSION + 1);
                assert_eq!(supported, SCHEMA_VERSION);
            }
            other => panic!("expected a schema refusal, got {other:?}"),
        }
    }

    /// The export states what it rests on, so a recipient does not have to
    /// notice for themselves.
    #[test]
    fn unsourced_assertions_are_counted_on_the_export() {
        let payload = CasePayload {
            parties: vec![
                party("observed", "observed", None),
                party("attributed", "asserted", Some("operator, 2026-08-12")),
                party("bare", "asserted", None),
                party("empty-source", "asserted", Some("")),
            ],
            ..Default::default()
        };
        let e = export("mekka", payload, 1_000).unwrap();
        assert_eq!(
            e.unsourced_assertion_count, 2,
            "bare and empty-source both count; an empty string is not a source"
        );
    }

    /// The whole point: an export survives a trip through JSON.
    #[test]
    fn survives_a_json_round_trip() {
        let e = export("mekka", payload_of(&["a", "b"]), 1_000).unwrap();
        let text = serde_json::to_string(&e).unwrap();
        let back: CaseExport = serde_json::from_str(&text).unwrap();
        assert_eq!(back, e);
        assert!(verify(&back).is_ok());
    }
}
