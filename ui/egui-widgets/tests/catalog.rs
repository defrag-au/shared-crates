//! Generates `CATALOG.md` — the one-page index of every widget in this crate —
//! and fails if the committed copy is stale.
//!
//! ## Why this exists
//!
//! This crate has ninety-odd modules. Finding out whether something already
//! exists meant grepping for a name you had already guessed, which only ever
//! finds what you already suspect. The predictable result: widgets rebuilt from
//! scratch because nobody knew the original was there. `IdPill` — middle-elided
//! identifier with a copy button — was reimplemented, worse, inline, in a
//! project that already depended on this crate.
//!
//! ## Why GENERATED, and why a test
//!
//! A hand-written catalogue drifts the first time someone adds a widget, and a
//! stale index is worse than none because it teaches you not to trust it.
//!
//! Every module here already opens with `//! \`Name\` — one-line purpose.`, and
//! that line is maintained for free because it sits against the code. So the
//! catalogue is derived from it, and this test asserts the committed file
//! matches — the same contract as `cargo fmt --check`. Add a widget without
//! regenerating and CI says so.
//!
//! Regenerate with:
//!
//! ```sh
//! UPDATE_CATALOG=1 cargo test -p egui-widgets --test catalog
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Modules that are infrastructure rather than widgets. Listing them would
/// bury the things a reader is actually shopping for.
const NOT_WIDGETS: &[&str] = &[
    "fonts",
    "icons",
    "image_loader",
    "motion",
    "screenshot",
    "selection",
    "theme",
    "utils",
];

/// Words whose trailing dot is not a sentence end.
const ABBREVIATIONS: &[&str] = &["e.g", "i.e", "cf", "vs", "etc", "approx", "no"];

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

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

/// First sentence of a module's `//!` header, flattened to one line.
///
/// Deliberately just the FIRST line: it is the part authors write as a
/// summary, and a catalogue entry that runs to a paragraph stops being
/// skimmable — which is the only thing this file is for.
fn summary(src: &str) -> Option<String> {
    let mut out = String::new();
    for line in src.lines() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix("//!") else {
            // Stop at the first non-doc line: attributes and `use` mean the
            // header is over.
            if out.is_empty() && (line.is_empty() || line.starts_with("//")) {
                continue;
            }
            break;
        };
        let rest = rest.trim();
        if rest.is_empty() {
            if !out.is_empty() {
                break; // blank doc line ends the summary paragraph
            }
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(rest);
        // A full stop ends it, unless it is an abbreviation-looking dot.
        if out.ends_with('.') {
            break;
        }
    }
    let out = first_sentence(out.trim()).trim_end_matches('.').to_string();
    (!out.is_empty()).then_some(out)
}

fn render(entries: &BTreeMap<String, String>) -> String {
    let mut s = String::new();
    s.push_str("# egui-widgets catalogue\n\n");
    s.push_str(
        "**Read this before building a widget.** One line per module, alphabetical, generated\n\
         from each module's own `//!` header by `tests/catalog.rs` — so it cannot drift.\n\n\
         Regenerate: `UPDATE_CATALOG=1 cargo test -p egui-widgets --test catalog`\n\n",
    );
    s.push_str(&format!("{} widgets.\n\n", entries.len()));
    s.push_str("| module | what it is |\n|---|---|\n");
    for (name, desc) in entries {
        // Pipes would break the table; nothing else needs escaping.
        s.push_str(&format!("| `{name}` | {} |\n", desc.replace('|', "\\|")));
    }
    s
}

fn collect(src_dir: &Path) -> BTreeMap<String, String> {
    let mut entries = BTreeMap::new();
    let mut undocumented = Vec::new();
    for entry in std::fs::read_dir(src_dir).expect("read src/") {
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
    entries
}

#[test]
fn catalogue_is_current() {
    let dir = crate_dir();
    let entries = collect(&dir.join("src"));
    let rendered = render(&entries);
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
         `UPDATE_CATALOG=1 cargo test -p egui-widgets --test catalog`"
    );
}

/// The summary parser is the whole contract; if it silently produced empty or
/// runaway entries the catalogue would be useless without failing.
#[test]
fn summary_takes_the_first_sentence_only() {
    let src =
        "//! `Thing` — does the thing.\n//!\n//! A much longer explanation follows.\n\nuse egui;";
    assert_eq!(summary(src).as_deref(), Some("`Thing` — does the thing"));

    // Wrapped first sentence is joined.
    let src = "//! `Wide` — a summary that happens\n//! to wrap across two lines.\n//!\n//! More.";
    assert_eq!(
        summary(src).as_deref(),
        Some("`Wide` — a summary that happens to wrap across two lines")
    );

    // A header with no full stop still yields its paragraph.
    let src = "//! `NoStop` — no full stop here\n//!\n//! Body.";
    assert_eq!(
        summary(src).as_deref(),
        Some("`NoStop` — no full stop here")
    );

    // A second sentence on the SAME line is cut — headers wrap, so a line end
    // is not a reliable sentence end.
    let src = "//! `Thing` — does the thing. Then a second sentence that\n//! wraps and must not appear.\n\nuse egui;";
    assert_eq!(summary(src).as_deref(), Some("`Thing` — does the thing"));

    // An abbreviation's dot is not a boundary.
    let src = "//! `Thing` — takes a list, e.g. of assets, and shows it.\n\nuse egui;";
    assert_eq!(
        summary(src).as_deref(),
        Some("`Thing` — takes a list, e.g. of assets, and shows it")
    );

    // No doc header at all.
    assert_eq!(summary("use egui;\n"), None);
    assert_eq!(summary(""), None);
}
