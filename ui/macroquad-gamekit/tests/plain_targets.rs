//! Fails if any source file in this crate calls macroquad's `render_target`
//! or `render_target_msaa` directly instead of `harness::plain_target`.
//!
//! Both make a multisampled target whose passes end in a `glReadBuffer` blit,
//! which WebGL 1 lacks: the game dies in the browser while every native run
//! and capture looks fine. Clippy cannot see it, so this scan does.

use std::path::PathBuf;

/// Calls of `name(` not preceded by an identifier character, so
/// `plain_target(` and `.render_target` field accesses do not count.
fn bare_calls<'a>(src: &'a str, name: &str) -> Vec<(usize, &'a str)> {
    let needle = format!("{name}(");
    src.lines()
        .enumerate()
        .filter(|(_, line)| {
            let code = line.split("//").next().unwrap_or("");
            code.match_indices(&needle).any(|(i, _)| {
                code[..i]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '.'))
            })
        })
        .map(|(n, line)| (n + 1, line))
        .collect()
}

#[test]
fn no_multisampled_render_targets() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    let mut scanned = 0;
    for entry in std::fs::read_dir(&src).expect("read src/") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        scanned += 1;
        let text = std::fs::read_to_string(&path).expect("read source");
        for name in ["render_target", "render_target_msaa"] {
            for (line, code) in bare_calls(&text, name) {
                found.push(format!("{}:{line}: {}", path.display(), code.trim()));
            }
        }
    }
    // A scan that read nothing would pass vacuously.
    assert!(
        scanned >= 10,
        "scanned only {scanned} files in {}",
        src.display()
    );
    assert!(
        found.is_empty(),
        "use harness::plain_target, not macroquad's render_target (WebGL 1 crash):\n{}",
        found.join("\n")
    );
}

#[test]
fn the_scan_catches_a_bare_call_and_ignores_the_helper() {
    let src = "let a = render_target(w, h);\nlet b = plain_target(w, h);\ncam.render_target = None;\n// render_target(w, h) in a comment\n";
    let hits = bare_calls(src, "render_target");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].0, 1);
}
