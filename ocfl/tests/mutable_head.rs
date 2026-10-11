//! The `0005-mutable-head` extension end to end: stage, revise, access,
//! commit, purge — and the checks verification is supposed to make.

use std::path::Path;

use ocfl::fs::{MutableHead, Object, ObjectWriter};
use ocfl::{ContentAlgorithm, Inventory, LogicalState, sidecar_contents, sidecar_name};

fn state(files: &[(&str, &[u8])]) -> LogicalState {
    files
        .iter()
        .map(|(path, bytes)| ((*path).to_string(), bytes.to_vec()))
        .collect()
}

fn mint(dir: &Path) {
    let mut writer = ObjectWriter::create(dir, "urn:mugz:0001", ContentAlgorithm::Sha512).unwrap();
    writer
        .commit(
            &state(&[("0001.webp", b"still")]),
            "2026-10-11T00:00:00Z",
            Some("Minted".into()),
            None,
        )
        .unwrap();
}

fn create_head(dir: &Path) -> MutableHead {
    let mut writer = ObjectWriter::open(dir).unwrap();
    writer
        .create_mutable_head(
            &state(&[("0001.webp", b"still"), ("wrap.txt", b"wrap-v1")]),
            "2026-10-12T00:00:00Z",
            Some("Wrapping".into()),
            None,
        )
        .unwrap()
}

fn head_dir(root: &Path) -> std::path::PathBuf {
    root.join("extensions/0005-mutable-head")
}

fn rewrite_root_inventory(root: &Path, inventory: &Inventory) {
    let bytes = serde_json::to_vec_pretty(inventory).unwrap();
    let sidecar = sidecar_contents(&ContentAlgorithm::Sha512.digest(&bytes));
    for dir in [root.to_path_buf(), root.join(&inventory.head)] {
        std::fs::write(dir.join("inventory.json"), &bytes).unwrap();
        std::fs::write(dir.join(sidecar_name(ContentAlgorithm::Sha512)), &sidecar).unwrap();
    }
}

#[test]
fn a_mutable_head_layers_a_revision_over_a_committed_version() {
    let dir = tempfile::tempdir().unwrap();
    mint(dir.path());
    let head = create_head(dir.path());
    assert_eq!(head.version(), "v2");
    assert_eq!(head.latest_revision(), 1);
    assert!(!head.conflict().unwrap());
    assert!(head_dir(dir.path()).join("revisions/r1").is_file());
    assert!(
        head_dir(dir.path())
            .join("head/content/r1/wrap.txt")
            .is_file()
    );

    let object = Object::open(dir.path()).unwrap();
    let info = object.mutable_head().unwrap();
    assert_eq!(info.version, "v2");
    assert_eq!(info.latest_revision, 1);
    assert_eq!(object.inventory().head, "v2");
    assert_eq!(object.root_inventory().head, "v1");
    assert_eq!(object.read_file("wrap.txt").unwrap(), b"wrap-v1");
    assert_eq!(object.read_file("0001.webp").unwrap(), b"still");
    let report = object.verify();
    assert!(report.is_ok(), "{:#?}", report.issues);

    // New root versions are refused while the HEAD is active.
    let mut writer = ObjectWriter::open(dir.path()).unwrap();
    assert!(
        writer
            .commit(
                &state(&[("other.txt", b"x")]),
                "2026-10-12T01:00:00Z",
                None,
                None,
            )
            .is_err()
    );
}

#[test]
fn a_revision_replaces_state_and_deletes_unreferenced_content() {
    let dir = tempfile::tempdir().unwrap();
    mint(dir.path());
    let mut head = create_head(dir.path());
    let revision = head
        .revise(
            &state(&[("0001.webp", b"still"), ("wrap.txt", b"wrap-v2")]),
            "2026-10-13T00:00:00Z",
            Some("Rewrapped".into()),
            None,
        )
        .unwrap();
    assert_eq!(revision, 2);
    assert!(head_dir(dir.path()).join("revisions/r2").is_file());
    assert!(
        head_dir(dir.path())
            .join("head/content/r2/wrap.txt")
            .is_file()
    );
    // r1's copy of wrap.txt is no longer referenced: pruned, with its
    // now-empty revision directory.
    assert!(!head_dir(dir.path()).join("head/content/r1").exists());

    let object = Object::open(dir.path()).unwrap();
    assert_eq!(object.read_file("wrap.txt").unwrap(), b"wrap-v2");
    assert_eq!(object.mutable_head().unwrap().latest_revision, 2);
    let report = object.verify();
    assert!(report.is_ok(), "{:#?}", report.issues);
}

#[test]
fn a_root_change_is_a_conflict_that_blocks_commit() {
    let dir = tempfile::tempdir().unwrap();
    mint(dir.path());
    let head = create_head(dir.path());

    // Simulate an external tool touching the root object.
    let mut root_inventory = Object::open(dir.path()).unwrap().root_inventory().clone();
    root_inventory.versions.get_mut("v1").unwrap().message = Some("touched elsewhere".into());
    rewrite_root_inventory(dir.path(), &root_inventory);

    assert!(head.conflict().unwrap());
    assert!(head.commit().is_err());

    let report = Object::open(dir.path()).unwrap().verify();
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.message.contains("version conflict")),
        "{:#?}",
        report.issues
    );

    let mut writer = ObjectWriter::open(dir.path()).unwrap();
    assert!(
        writer
            .commit(
                &state(&[("other.txt", b"x")]),
                "2026-10-12T01:00:00Z",
                None,
                None,
            )
            .is_err()
    );
}

#[test]
fn commit_installs_the_mutable_head_as_the_next_version() {
    let dir = tempfile::tempdir().unwrap();
    mint(dir.path());
    let head = create_head(dir.path());
    let version = head.commit().unwrap();
    assert_eq!(version, "v2");
    assert!(!head_dir(dir.path()).exists());
    assert!(dir.path().join("v2/content/r1/wrap.txt").is_file());

    let object = Object::open(dir.path()).unwrap();
    assert!(object.mutable_head().is_none());
    assert_eq!(object.inventory().head, "v2");
    assert_eq!(object.read_file("wrap.txt").unwrap(), b"wrap-v1");
    assert_eq!(object.read_file("0001.webp").unwrap(), b"still");
    let state_v2 = object.state(Some("v2")).unwrap();
    assert_eq!(state_v2["wrap.txt"], "v2/content/r1/wrap.txt");
    assert_eq!(state_v2["0001.webp"], "v1/content/0001.webp");
    let report = object.verify();
    assert!(report.is_ok(), "{:#?}", report.issues);
}

#[test]
fn purge_discards_the_mutable_head() {
    let dir = tempfile::tempdir().unwrap();
    mint(dir.path());
    let head = create_head(dir.path());
    head.purge().unwrap();
    assert!(!head_dir(dir.path()).exists());
    let object = Object::open(dir.path()).unwrap();
    assert!(object.mutable_head().is_none());
    assert_eq!(object.inventory().head, "v1");
    let report = object.verify();
    assert!(report.is_ok(), "{:#?}", report.issues);
}

#[test]
fn a_new_object_stages_its_first_content_in_a_mutable_head() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer =
        ObjectWriter::create(dir.path(), "urn:mugz:0002", ContentAlgorithm::Sha512).unwrap();
    writer
        .commit(
            &LogicalState::new(),
            "2026-10-11T00:00:00Z",
            Some("Empty v1".into()),
            None,
        )
        .unwrap();
    let head = writer
        .create_mutable_head(
            &state(&[("0001.webp", b"still")]),
            "2026-10-11T01:00:00Z",
            None,
            None,
        )
        .unwrap();
    assert_eq!(head.version(), "v2");
    assert_eq!(head.commit().unwrap(), "v2");

    let object = Object::open(dir.path()).unwrap();
    assert_eq!(object.read_file("0001.webp").unwrap(), b"still");
    let report = object.verify();
    assert!(report.is_ok(), "{:#?}", report.issues);
}

#[test]
fn verify_catches_an_unreferenced_file_in_the_mutable_head() {
    let dir = tempfile::tempdir().unwrap();
    mint(dir.path());
    create_head(dir.path());
    std::fs::write(
        head_dir(dir.path()).join("head/content/r1/stray.txt"),
        b"stray",
    )
    .unwrap();
    let report = Object::open(dir.path()).unwrap().verify();
    assert!(!report.is_ok(), "{:#?}", report.issues);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.message.contains("not referenced by the manifest")),
        "{:#?}",
        report.issues
    );
}
