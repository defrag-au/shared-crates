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
const NOT_WIDGETS: &[&str] = &[
    "painter",
    "theme",
    "gesture",
    "format",
    "market_ink",
    "marks",
];

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

/// The painter's text-placement methods and the first sentence of each one's
/// doc comment: what a screen built from scratch reaches for to put a label
/// in a box. The painter is infrastructure, not a widget, so it has no row in
/// the module table, and these were not being found: labels were placed with
/// `text_top` and an offset tuned by eye, and sat a point or two off centre.
fn placement(src: &str) -> Vec<(String, String)> {
    let places = |name: &str| {
        name.starts_with("text")
            || name.starts_with("mono")
            || name.starts_with("stack")
            || name.ends_with("baseline")
            || name.starts_with("cap_height")
    };
    let mut out = Vec::new();
    let mut doc: Vec<String> = Vec::new();
    // Only `impl Painter`'s own methods: a helper type's `mono` is not one.
    let mut inside = false;
    for line in src.lines() {
        if line.starts_with("impl") {
            inside = line.contains("Painter");
        } else if line == "}" {
            inside = false;
        }
        if !inside {
            doc.clear();
            continue;
        }
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("///") {
            doc.push(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = t.strip_prefix("pub fn ") {
            let name: String = rest
                .chars()
                .take_while(|c| *c == '_' || c.is_alphanumeric())
                .collect();
            if places(&name) && !doc.is_empty() {
                let mut para = String::new();
                for d in &doc {
                    if d.is_empty() {
                        break;
                    }
                    if !para.is_empty() {
                        para.push(' ');
                    }
                    para.push_str(d);
                }
                let first = first_sentence(&para).trim_end_matches('.').to_string();
                out.push((name, first));
            }
        }
        if !t.starts_with("#[") {
            doc.clear();
        }
    }
    out
}

#[test]
fn placement_reads_each_methods_first_sentence() {
    let src = "impl<'a> Painter<'a> {\n    /// Text in a box. More detail.\n    pub fn text_in(&self) {}\n    /// Not text.\n    pub fn tapped(&self) {}\n}\nimpl Line {\n    /// Not the painter's.\n    pub fn mono(self) {}\n}";
    assert_eq!(
        placement(src),
        vec![("text_in".to_string(), "Text in a box".to_string())]
    );
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

    let painter = std::fs::read_to_string(dir.join("src/painter.rs")).expect("read painter");
    rendered.push_str(
        "\n## Text placement\n\n\
         **A label in a box is placed by its capitals, through these `Painter` methods**, never\n\
         with `text_top` and an offset tuned by eye: that is how labels end up a point or two\n\
         off centre, differently per face and size. `text_in` for one line (start, centre or\n\
         end), `stack_in` for a block (a title over its subtitle). Generated from `painter.rs`.\n\n\
         | method | what it does |\n|---|---|\n",
    );
    for (name, desc) in placement(&painter) {
        rendered.push_str(&format!(
            "| `Painter::{name}` | {} |\n",
            desc.replace('|', "\\|")
        ));
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
