//! The case-file schema, as DDL both storage backends execute verbatim.
//!
//! A local file (rusqlite) and a per-case Durable Object (`storage().sql()`)
//! cannot share a driver, but a schema written twice drifts, and a case exported
//! from one that will not open in the other is worthless. So the statements live
//! here and each edge runs them.

/// Bumped whenever [`DDL`] changes in a way an older reader would misread.
///
/// Written into every export. A reader that finds a *higher* version must
/// refuse the file rather than parse what it recognises — partially reading a
/// newer case silently drops whatever the new version added, and in a case file
/// the thing dropped is evidence.
pub const SCHEMA_VERSION: u32 = 1;

/// Statements to run in order on an empty case.
///
/// Every table is `IF NOT EXISTS` so opening an existing case is the same call
/// as creating one.
pub const DDL: &[&str] = &[
    // ── parties ─────────────────────────────────────────────────────────
    // `basis` and `source` are NOT NULL together by convention rather than
    // constraint: an asserted party with no source is a state the tool must be
    // able to hold and display as suspect, not one it refuses to store. Refusing
    // it would just push the assertion somewhere unrecorded.
    "CREATE TABLE IF NOT EXISTS party (
        key            TEXT PRIMARY KEY,
        chain          TEXT NOT NULL,
        has_stake      INTEGER NOT NULL,
        label          TEXT,
        basis          TEXT NOT NULL DEFAULT 'observed',
        source         TEXT,
        note           TEXT,
        updated_at     INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS cluster (
        name           TEXT PRIMARY KEY,
        color          TEXT,
        note           TEXT
    )",
    "CREATE TABLE IF NOT EXISTS cluster_member (
        cluster        TEXT NOT NULL,
        party          TEXT NOT NULL,
        PRIMARY KEY (cluster, party)
    )",
    // ── chain cache ─────────────────────────────────────────────────────
    // Per-tx NET only. There is no gross column because there is no gross
    // accessor in the model, and a cache that stored one would reintroduce the
    // error the model exists to foreclose.
    "CREATE TABLE IF NOT EXISTS tx_delta (
        tx_id          TEXT NOT NULL,
        party          TEXT NOT NULL,
        delta          TEXT NOT NULL,
        timestamp      INTEGER NOT NULL,
        PRIMARY KEY (tx_id, party)
    )",
    "CREATE INDEX IF NOT EXISTS tx_delta_party_time ON tx_delta (party, timestamp)",
    // The UTxO edge a provenance walk follows. Absent on account chains, which
    // is why it is a separate table rather than a nullable column on tx_delta.
    "CREATE TABLE IF NOT EXISTS provenance (
        tx_id          TEXT NOT NULL,
        input_index    INTEGER NOT NULL,
        party          TEXT NOT NULL,
        value          TEXT NOT NULL,
        source_tx      TEXT NOT NULL,
        PRIMARY KEY (tx_id, input_index)
    )",
    // ── findings ────────────────────────────────────────────────────────
    // `falsifier` is nullable: capture is free, and the gate is on promotion to
    // `status = 'survived'`, not on writing the claim down.
    "CREATE TABLE IF NOT EXISTS claim (
        id             TEXT PRIMARY KEY,
        statement      TEXT NOT NULL,
        falsifier      TEXT,
        status         TEXT NOT NULL DEFAULT 'untested',
        outcome        TEXT,
        created_at     INTEGER NOT NULL,
        updated_at     INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS claim_support (
        claim          TEXT NOT NULL,
        ordinal        INTEGER NOT NULL,
        summary        TEXT NOT NULL,
        basis          TEXT NOT NULL,
        source         TEXT,
        reference      TEXT,
        PRIMARY KEY (claim, ordinal)
    )",
    "CREATE TABLE IF NOT EXISTS note (
        id             TEXT PRIMARY KEY,
        subject_kind   TEXT NOT NULL,
        subject_id     TEXT NOT NULL,
        body           TEXT NOT NULL,
        created_at     INTEGER NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS note_subject ON note (subject_kind, subject_id)",
    // ── file metadata ───────────────────────────────────────────────────
    "CREATE TABLE IF NOT EXISTS case_meta (
        k              TEXT PRIMARY KEY,
        v              TEXT NOT NULL
    )",
];

/// Key under which [`SCHEMA_VERSION`] is recorded in `case_meta`.
pub const META_SCHEMA_VERSION: &str = "schema_version";
/// Human name of the case.
pub const META_CASE_NAME: &str = "case_name";

/// Whether a reader at [`SCHEMA_VERSION`] may open a case written at `found`.
///
/// Older is fine — migrations run forward. Newer is refused: a reader that
/// parses the fields it recognises out of a future file silently discards the
/// rest, and in a case file the discarded part is evidence.
pub fn can_open(found: u32) -> bool {
    found <= SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ddl_is_idempotent_by_construction() {
        // Opening an existing case runs the same statements as creating one, so
        // every statement has to tolerate already existing.
        for stmt in DDL {
            assert!(stmt.contains("IF NOT EXISTS"), "not idempotent: {stmt}");
        }
    }

    #[test]
    fn a_newer_case_file_is_refused_not_partially_read() {
        assert!(can_open(SCHEMA_VERSION));
        assert!(can_open(SCHEMA_VERSION.saturating_sub(1)));
        assert!(!can_open(SCHEMA_VERSION + 1));
    }

    /// The cache stores NET only. A gross column here would reintroduce the
    /// exact error the model forecloses.
    #[test]
    fn tx_delta_has_no_gross_column() {
        let tx = DDL.iter().find(|d| d.contains("tx_delta (")).unwrap();
        assert!(tx.contains("delta"));
        assert!(!tx.to_lowercase().contains("gross"));
    }

    /// Capture must stay free — the schema cannot require a falsifier.
    #[test]
    fn falsifier_is_nullable() {
        let claim = DDL.iter().find(|d| d.contains("claim (")).unwrap();
        assert!(
            claim.contains("falsifier      TEXT,"),
            "falsifier must be nullable"
        );
        assert!(claim.contains("statement      TEXT NOT NULL"));
    }
}
