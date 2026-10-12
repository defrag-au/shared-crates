//! Verify an OCFL object against the spec: conformance, fixity and history.
//!
//! ```sh
//! cargo run -p ocfl --example verify -- <object-root>
//! ```
//!
//! Exits non-zero when an error-severity issue is found, so CI can run it.

use ocfl::fs::{Object, Severity};

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: verify <object-root>");
        std::process::exit(2);
    };
    let object = match Object::open(&path) {
        Ok(object) => object,
        Err(error) => {
            eprintln!("open {path}: {error}");
            std::process::exit(1);
        }
    };
    let report = object.verify();
    println!(
        "{}: {} version(s), {} file(s), {} bytes",
        report.id, report.versions, report.files, report.bytes
    );
    for issue in &report.issues {
        let tag = match issue.severity {
            Severity::Error => "error",
            Severity::Warning => "warn ",
        };
        println!("  {tag}  {}", issue.message);
    }
    if report.is_ok() {
        println!("verify: OK");
    } else {
        println!("verify: FAILED");
        std::process::exit(1);
    }
}
