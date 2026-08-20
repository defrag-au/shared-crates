//! Generates `CATALOG.md` for this crate and fails if the committed copy is
//! stale. Same contract as `egui-widgets/tests/catalog.rs` — see that file for
//! why the index is generated rather than written by hand.
//!
//! This crate is smaller today, which is exactly why the index goes in now:
//! the egui one was added at ninety modules, after widgets had already been
//! rebuilt from scratch because nobody could find the originals.
//!
//! Regenerate with:
//!
//! ```sh
//! UPDATE_CATALOG=1 cargo test -p macroquad-widgets --test catalog
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Infrastructure rather than widgets.
const NOT_WIDGETS: &[&str] = &["painter", "theme", "gesture"];

/// Words whose trailing dot is not a sentence end.
const ABBREVIATIONS: &[&str] = &["e.g", "i.e", "cf", "vs", "etc", "approx", "no"];

/// Truncate at the first real sentence boundary — a full stop followed by a
/// space. Headers wrap, so relying on a line ending to close the sentence lets
/// whole paragraphs through, which is exactly what a skimmable index must not
/// contain.
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
fn summary_stops_at_the_first_sentence_even_mid_line() {
    let src = "//! `Thing` — does the thing. Then a second sentence that\n//! wraps and must not appear.\n\nuse macroquad::prelude::*;";
    assert_eq!(summary(src).as_deref(), Some("`Thing` — does the thing"));

    // An abbreviation's dot is not a boundary.
    let src = "//! `Thing` — takes a roster, e.g. a squad, and picks from it.\n\nuse x;";
    assert_eq!(
        summary(src).as_deref(),
        Some("`Thing` — takes a roster, e.g. a squad, and picks from it")
    );

    // Wrapped single sentence still joins.
    let src = "//! `Wide` — a summary that happens\n//! to wrap across two lines.\n//!\n//! More.";
    assert_eq!(
        summary(src).as_deref(),
        Some("`Wide` — a summary that happens to wrap across two lines")
    );

    assert_eq!(summary("use macroquad;\n"), None);
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
        if stem == "lib" || NOT_WIDGETS.contains(&stem.as_str()) {
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
    rendered.push_str("# macroquad-widgets catalogue\n\n");
    rendered.push_str(
        "**Read this before building a widget.** One line per module, alphabetical, generated\n\
         from each module's own `//!` header by `tests/catalog.rs` — so it cannot drift.\n\n\
         Regenerate: `UPDATE_CATALOG=1 cargo test -p macroquad-widgets --test catalog`\n\n\
         Note: these are MACROQUAD widgets. They cannot use wasm-bindgen, so they do not\n\
         interchange with `egui-widgets` — see the shared-crates CLAUDE.md on runtime pairs.\n\n",
    );
    rendered.push_str(&format!("{} widgets.\n\n", entries.len()));
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
         `UPDATE_CATALOG=1 cargo test -p macroquad-widgets --test catalog`"
    );
}
