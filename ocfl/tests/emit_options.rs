//! Writer options end to end: fixity emission (§3.5.4) and the
//! `0011-direct-clean-path-layout` path map.

use ocfl::fs::{Object, ObjectWriter};
use ocfl::{ContentAlgorithm, DigestAlgorithm, DirectClean, DirectCleanConfig, LogicalState};

fn state(files: &[(&str, &[u8])]) -> LogicalState {
    files
        .iter()
        .map(|(path, bytes)| ((*path).to_string(), bytes.to_vec()))
        .collect()
}

fn writer(dir: &std::path::Path) -> ObjectWriter {
    ObjectWriter::create(dir, "urn:mugz:0001", ContentAlgorithm::Sha512).unwrap()
}

#[test]
fn fixity_emission_round_trips_and_is_verified() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer = writer(dir.path());
    writer
        .set_fixity_algorithms([DigestAlgorithm::Md5, DigestAlgorithm::Sha1])
        .unwrap();
    writer
        .commit(
            &state(&[("0001.webp", b"still")]),
            "2026-10-11T00:00:00Z",
            None,
            None,
        )
        .unwrap();
    // A second commit adds values for its new file; v1 values stay.
    writer
        .commit(
            &state(&[("0001.webp", b"still"), ("wrap.txt", b"wrap")]),
            "2026-10-12T00:00:00Z",
            None,
            None,
        )
        .unwrap();

    let object = Object::open(dir.path()).unwrap();
    let fixity = &object.inventory().fixity;
    let still = DigestAlgorithm::Md5.digest(b"still").unwrap();
    assert_eq!(
        fixity[&DigestAlgorithm::Md5][&still],
        ["v1/content/0001.webp"]
    );
    let wrap = DigestAlgorithm::Md5.digest(b"wrap").unwrap();
    assert_eq!(
        fixity[&DigestAlgorithm::Md5][&wrap],
        ["v2/content/wrap.txt"]
    );
    assert!(fixity.contains_key(&DigestAlgorithm::Sha1));
    let report = object.verify();
    assert!(report.is_ok(), "{:#?}", report.issues);
}

#[test]
fn an_algorithm_we_cannot_compute_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer = writer(dir.path());
    let error = writer
        .set_fixity_algorithms([DigestAlgorithm::Other("xxh3-128".into())])
        .unwrap_err()
        .to_string();
    assert!(error.contains("xxh3-128"), "{error}");
}

#[test]
fn the_0011_map_places_content_under_cleaned_paths() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer = writer(dir.path());
    writer.set_content_path_map(DirectClean::new(DirectCleanConfig::default()).unwrap());
    writer
        .commit(
            &state(&[("a:b.txt", b"one"), ("plain.txt", b"two")]),
            "2026-10-11T00:00:00Z",
            None,
            None,
        )
        .unwrap();

    assert!(dir.path().join("v1/content/a_b.txt").is_file());
    assert!(!dir.path().join("v1/content/a:b.txt").exists());
    let object = Object::open(dir.path()).unwrap();
    assert_eq!(object.state(None).unwrap()["a:b.txt"], "v1/content/a_b.txt");
    assert_eq!(object.read_file("a:b.txt").unwrap(), b"one");
    let report = object.verify();
    assert!(report.is_ok(), "{:#?}", report.issues);
}

#[test]
fn a_colliding_map_is_rejected_at_commit() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer = writer(dir.path());
    writer.set_content_path_map(DirectClean::new(DirectCleanConfig::default()).unwrap());
    let error = writer
        .commit(
            &state(&[("a:b", b"one"), ("a_b", b"two")]),
            "2026-10-11T00:00:00Z",
            None,
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("would hold both"), "{error}");
}
