//! End-to-end: emit an object version by version, verify it — and check the
//! failures verification is supposed to catch.

use std::collections::BTreeSet;
use std::path::Path;

use ocfl::fs::{Layout, Object, ObjectWriter, Severity, StorageRoot};
use ocfl::{ContentAlgorithm, LogicalState};

fn state(files: &[(&str, &[u8])]) -> LogicalState {
    files
        .iter()
        .map(|(path, bytes)| ((*path).to_string(), bytes.to_vec()))
        .collect()
}

fn mint(dir: &Path) -> ObjectWriter {
    let mut writer = ObjectWriter::create(dir, "urn:mugz:0001", ContentAlgorithm::Sha512).unwrap();
    writer
        .commit(
            &state(&[
                ("0001.webp", b"still-bytes"),
                ("pack.json", br#"{"schema":"mugz-customiser-pack/2"}"#),
                ("pieces/aaaa.png", b"piece-bytes"),
            ]),
            "2026-10-11T00:00:00Z",
            Some("Minted".into()),
            None,
        )
        .unwrap();
    writer
}

fn errors(report: &ocfl::fs::VerifyReport) -> Vec<String> {
    report
        .issues
        .iter()
        .filter(|issue| issue.severity == Severity::Error)
        .map(|issue| issue.message.clone())
        .collect()
}

#[test]
fn a_first_version_emits_a_valid_object() {
    let dir = tempfile::tempdir().unwrap();
    let writer = mint(dir.path());
    assert_eq!(writer.inventory().head, "v1");

    let object = Object::open(dir.path()).unwrap();
    let report = object.verify();
    assert!(report.is_ok(), "{:#?}", report.issues);
    assert_eq!(report.versions, 1);
    assert_eq!(report.files, 3);

    // The root inventory is byte-identical to the newest version's copy, and
    // the sidecar names it.
    let root_bytes = std::fs::read(dir.path().join("inventory.json")).unwrap();
    let version_bytes = std::fs::read(dir.path().join("v1/inventory.json")).unwrap();
    assert_eq!(root_bytes, version_bytes);
    let sidecar = std::fs::read_to_string(dir.path().join("inventory.json.sha512")).unwrap();
    assert!(sidecar.ends_with(" inventory.json\n"), "{sidecar:?}");

    // A logical file reads back digest-verified.
    assert_eq!(object.read_file("0001.webp").unwrap(), b"still-bytes");
    assert!(object.read_file("not-there").is_err());
}

#[test]
fn an_update_stores_only_the_delta() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer = mint(dir.path());

    let version = writer
        .commit(
            &state(&[
                ("0001.webp", b"still-bytes"), // unchanged — referenced, not copied
                ("pack.json", br#"{"schema":"mugz-customiser-pack/2","n":1}"#), // changed
                ("update.json", br#"{"changes":{}}"#), // added
            ]),
            "2026-10-11T01:00:00Z",
            Some("Owner update".into()),
            None,
        )
        .unwrap();
    assert_eq!(version, "v2");

    // v2 stores exactly the two files whose digests are new.
    let stored: BTreeSet<String> = std::fs::read_dir(dir.path().join("v2/content"))
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        stored,
        BTreeSet::from(["pack.json".to_string(), "update.json".to_string()])
    );

    // State resolves the unchanged file to its v1 content path; verification
    // passes and the root copy has moved on.
    let object = Object::open(dir.path()).unwrap();
    let report = object.verify();
    assert!(report.is_ok(), "{:#?}", report.issues);
    let resolved = object.state(None).unwrap();
    assert_eq!(resolved["0001.webp"], "v1/content/0001.webp");
    assert_eq!(resolved["pack.json"], "v2/content/pack.json");
    assert_eq!(
        std::fs::read(dir.path().join("inventory.json")).unwrap(),
        std::fs::read(dir.path().join("v2/inventory.json")).unwrap()
    );
}

#[test]
fn content_corruption_is_caught() {
    let dir = tempfile::tempdir().unwrap();
    mint(dir.path());
    let file = dir.path().join("v1/content/0001.webp");
    let mut bytes = std::fs::read(&file).unwrap();
    bytes[0] ^= 0xff;
    std::fs::write(&file, bytes).unwrap();

    let report = Object::open(dir.path()).unwrap().verify();
    assert!(!report.is_ok());
    assert!(
        errors(&report).iter().any(|e| e.contains("digest")),
        "{:#?}",
        report.issues
    );
}

#[test]
fn a_tampered_root_inventory_is_caught() {
    let dir = tempfile::tempdir().unwrap();
    mint(dir.path());
    let object = Object::open(dir.path()).unwrap();

    let file = dir.path().join("inventory.json");
    let mut bytes = std::fs::read(&file).unwrap();
    bytes.push(b' ');
    std::fs::write(&file, bytes).unwrap();

    let report = object.verify();
    assert!(!report.is_ok());
    assert!(
        errors(&report)
            .iter()
            .any(|e| e.contains("sidecar") || e.contains("differs")),
        "{:#?}",
        report.issues
    );
}

#[test]
fn a_missing_file_is_caught() {
    let dir = tempfile::tempdir().unwrap();
    mint(dir.path());
    std::fs::remove_file(dir.path().join("v1/content/pack.json")).unwrap();

    let report = Object::open(dir.path()).unwrap().verify();
    assert!(!report.is_ok());
    assert!(
        errors(&report)
            .iter()
            .any(|e| e.contains("missing or unreadable")),
        "{:#?}",
        report.issues
    );
}

#[test]
fn an_unreferenced_file_is_caught() {
    let dir = tempfile::tempdir().unwrap();
    mint(dir.path());
    std::fs::write(dir.path().join("v1/content/extra.bin"), b"stowaway").unwrap();

    let report = Object::open(dir.path()).unwrap().verify();
    assert!(!report.is_ok());
    assert!(
        errors(&report).iter().any(|e| e.contains("not referenced")),
        "{:#?}",
        report.issues
    );
}

#[test]
fn a_storage_root_maps_ids_and_hosts_objects() {
    let dir = tempfile::tempdir().unwrap();
    StorageRoot::create(
        dir.path(),
        Layout::OmitPrefix {
            delimiter: ":".into(),
        },
    )
    .unwrap();

    let root = StorageRoot::open(dir.path()).unwrap();
    assert_eq!(
        root.layout(),
        &Layout::OmitPrefix {
            delimiter: ":".into()
        }
    );
    let path = root.object_path("urn:mugz:0001").unwrap();
    assert_eq!(path.file_name().unwrap(), "0001");
    assert!(
        dir.path()
            .join("extensions/0006-flat-omit-prefix-storage-layout/config.json")
            .is_file()
    );

    let mut writer = root
        .writer("urn:mugz:0001", ContentAlgorithm::Sha512)
        .unwrap();
    writer
        .commit(
            &state(&[("0001.webp", b"still")]),
            "2026-10-11T00:00:00Z",
            None,
            None,
        )
        .unwrap();

    let object = root.object("urn:mugz:0001").unwrap();
    let report = object.verify();
    assert!(report.is_ok(), "{:#?}", report.issues);
}

#[test]
fn a_custom_content_directory_is_honoured_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer =
        ObjectWriter::create(dir.path(), "urn:mugz:0001", ContentAlgorithm::Sha512).unwrap();
    writer.set_content_directory("data").unwrap();
    writer
        .commit(
            &state(&[("0001.webp", b"still")]),
            "2026-10-11T00:00:00Z",
            None,
            None,
        )
        .unwrap();
    assert!(dir.path().join("v1/data/0001.webp").is_file());
    assert!(!dir.path().join("v1/content").exists());

    let object = Object::open(dir.path()).unwrap();
    assert_eq!(object.inventory().content_dir(), "data");
    assert_eq!(object.read_file("0001.webp").unwrap(), b"still");
    let report = object.verify();
    assert!(report.is_ok(), "{:#?}", report.issues);

    // The name is fixed for the object's life (§3.3.1).
    let mut writer = ObjectWriter::open(dir.path()).unwrap();
    assert!(writer.set_content_directory("elsewhere").is_err());
}
