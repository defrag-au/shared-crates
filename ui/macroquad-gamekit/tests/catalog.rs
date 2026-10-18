//! Generates `CATALOG.md` for this crate and fails if the committed copy is
//! stale. Same contract as `macroquad-widgets/tests/catalog.rs`: a module
//! without a `//!` header cannot be found, so it fails the build instead.
//!
//! Regenerate with:
//!
//! ```sh
//! UPDATE_CATALOG=1 cargo test -p macroquad-gamekit --test catalog
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Words whose trailing dot is not a sentence end.
const ABBREVIATIONS: &[&str] = &["e.g", "i.e", "cf", "vs", "etc", "approx", "no"];

/// Truncate at the first real sentence boundary: a full stop then a space.
fn first_sentence(s: &str) -> &str {
    let bytes = s.as_bytes();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] != b'.' || bytes[i + 1] != b' ' {
            continue;
        }
        let head = &s[..i];
        let last = head.rsplit(' ').next().unwrap_or("").to_ascii_lowercase();
        if !ABBREVIATIONS.contains(&last.as_str()) {
            return head;
        }
    }
    s
}

fn summary(src: &str) -> Option<String> {
    let mut out = String::new();
    for line in src.lines() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix("//!") else {
            if out.is_empty() && (line.is_empty() || line.starts_with("//")) {
                continue;
            }
            break;
        };
        let rest = rest.trim();
        if rest.is_empty() {
            if !out.is_empty() {
                break;
            }
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(rest);
        if out.ends_with('.') {
            break;
        }
    }
    let out = first_sentence(out.trim()).trim_end_matches('.').to_string();
    (!out.is_empty()).then_some(out)
}

#[test]
fn catalogue_is_current() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut entries: BTreeMap<String, String> = BTreeMap::new();
    let mut undocumented = Vec::new();
    for entry in std::fs::read_dir(dir.join("src")).expect("read src/") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        if stem == "lib" {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("read module");
        match summary(&src) {
            Some(s) => {
                entries.insert(stem, s);
            }
            None => undocumented.push(stem),
        }
    }
    assert!(
        undocumented.is_empty(),
        "these modules have no `//!` header, so they cannot appear in the catalogue \
         and nobody will find them: {undocumented:?}"
    );

    let mut rendered = String::new();
    rendered.push_str("# macroquad-gamekit catalogue\n\n");
    rendered.push_str(
        "**Read this before building game-feel code.** One line per module, alphabetical,\n\
         generated from each module's own `//!` header by `tests/catalog.rs` — so it cannot drift.\n\n\
         Regenerate: `UPDATE_CATALOG=1 cargo test -p macroquad-gamekit --test catalog`\n\n\
         The companion to `macroquad-widgets` (stateless widgets): everything here holds state\n\
         across frames and takes time as an argument. Stories are in `macroquad-storybook`\n\
         under \"gamekit\". MACROQUAD only: no wasm-bindgen, see the shared-crates CLAUDE.md on\n\
         runtime pairs.\n\n",
    );
    rendered.push_str(&format!("{} modules.\n\n", entries.len()));
    rendered.push_str("| module | what it is |\n|---|---|\n");
    for (name, desc) in &entries {
        rendered.push_str(&format!("| `{name}` | {} |\n", desc.replace('|', "\\|")));
    }

    let path = dir.join("CATALOG.md");
    if std::env::var("UPDATE_CATALOG").is_ok() {
        std::fs::write(&path, &rendered).expect("write CATALOG.md");
        eprintln!("wrote {} entries to {}", entries.len(), path.display());
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        committed, rendered,
        "CATALOG.md is out of date — regenerate with \
         `UPDATE_CATALOG=1 cargo test -p macroquad-gamekit --test catalog`"
    );
}
