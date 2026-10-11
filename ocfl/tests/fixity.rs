//! The fixity block (§3.5.4) end to end: inject md5 values into a minted
//! object the way an external tool adding legacy fixity would, verify they
//! hold, then watch verification catch a value that does not match its file.

use std::collections::BTreeMap;
use std::path::Path;

use ocfl::fs::{Object, ObjectWriter};
use ocfl::{
    ContentAlgorithm, DigestAlgorithm, Inventory, LogicalState, sidecar_contents, sidecar_name,
};

fn mint(dir: &Path) {
    let mut writer = ObjectWriter::create(dir, "urn:mugz:0001", ContentAlgorithm::Sha512).unwrap();
    let mut state = LogicalState::new();
    state.insert("0001.webp".into(), b"still-bytes".to_vec());
    state.insert(
        "pack.json".into(),
        br#"{"schema":"mugz-customiser-pack/2"}"#.to_vec(),
    );
    writer
        .commit(&state, "2026-10-11T00:00:00Z", Some("Minted".into()), None)
        .unwrap();
}

/// Rewrite the inventory — the root and the head version copies must agree
/// byte for byte — and its sidecar, as an external tool would.
fn rewrite_inventory(root: &Path, inventory: &Inventory) {
    let bytes = serde_json::to_vec_pretty(inventory).unwrap();
    let sidecar = sidecar_contents(&ContentAlgorithm::Sha512.digest(&bytes));
    for dir in [root.to_path_buf(), root.join(&inventory.head)] {
        std::fs::write(dir.join("inventory.json"), &bytes).unwrap();
        std::fs::write(dir.join(sidecar_name(ContentAlgorithm::Sha512)), &sidecar).unwrap();
    }
}

fn md5_of(root: &Path, content: &str) -> String {
    let bytes = std::fs::read(root.join(content)).unwrap();
    DigestAlgorithm::Md5.digest(&bytes).unwrap()
}

#[test]
fn fixity_values_are_verified_against_the_files() {
    let dir = tempfile::tempdir().unwrap();
    mint(dir.path());

    let mut inventory = Object::open(dir.path()).unwrap().inventory().clone();
    inventory.fixity.insert(
        DigestAlgorithm::Md5,
        BTreeMap::from([
            (
                md5_of(dir.path(), "v1/content/0001.webp"),
                vec!["v1/content/0001.webp".to_string()],
            ),
            (
                md5_of(dir.path(), "v1/content/pack.json"),
                vec!["v1/content/pack.json".to_string()],
            ),
        ]),
    );
    rewrite_inventory(dir.path(), &inventory);
    let report = Object::open(dir.path()).unwrap().verify();
    assert!(report.is_ok(), "{:#?}", report.issues);

    // A fixity value that does not match its file is an error, independent of
    // the manifest's own digest.
    let mut wrong = inventory.clone();
    wrong.fixity.insert(
        DigestAlgorithm::Md5,
        BTreeMap::from([("0".repeat(32), vec!["v1/content/0001.webp".to_string()])]),
    );
    rewrite_inventory(dir.path(), &wrong);
    let report = Object::open(dir.path()).unwrap().verify();
    assert!(!report.is_ok(), "{:#?}", report.issues);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.message.contains("fixity md5")),
        "{:#?}",
        report.issues
    );
}
