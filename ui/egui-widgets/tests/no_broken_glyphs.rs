//! Guardrail: no known-broken Unicode glyphs in rendered strings.
//!
//! egui's default font (Ubuntu) does NOT cover many symbol glyphs — arrows,
//! the warning sign, ballot/check marks, the math minus — and renders them as
//! a "tofu" box. We keep hitting this (the broken `✕`, a broken `←`, …). The
//! rule: in `egui-widgets`, never put those glyphs in rendered text — use a
//! [`PhosphorIcon`](egui_widgets::PhosphorIcon) (the installed icon font) for
//! iconography, or plain ASCII (e.g. `-` not the math minus).
//!
//! This test scans every `src/**/*.rs`, strips trailing `//` comments (which
//! don't render), and fails if any denylisted codepoint appears in the code —
//! i.e. inside a string/char literal. The denylist is spelled with `\u{}`
//! escapes so this file doesn't trip its own check. To add an icon, extend
//! `PhosphorIcon`; don't reach for a bare Unicode symbol.

use std::path::Path;

/// Codepoints egui's default font won't render — keep them out of rendered
/// strings. (Safe glyphs deliberately omitted: middle dot `·`, em dash `—`,
/// ellipsis `…`, `×` — all render fine.)
const DENY: &[(char, &str)] = &[
    ('\u{2190}', "left arrow"),
    ('\u{2191}', "up arrow"),
    ('\u{2192}', "right arrow"),
    ('\u{2193}', "down arrow"),
    ('\u{2194}', "left-right arrow"),
    ('\u{2195}', "up-down arrow"),
    ('\u{26A0}', "warning sign"),
    ('\u{2713}', "check mark"),
    ('\u{2714}', "heavy check mark"),
    ('\u{2715}', "multiplication x"),
    ('\u{2716}', "heavy multiplication x"),
    ('\u{2717}', "ballot x"),
    ('\u{2718}', "heavy ballot x"),
    ('\u{2212}', "minus sign (use ASCII '-')"),
];

/// Truncate a line at the first `//` that is NOT inside a string literal, so
/// trailing comments (which don't render) are ignored. Byte-indexed, but only
/// matches ASCII `"`/`/`, so multibyte glyphs are never split.
fn code_portion(line: &str) -> &str {
    let b = line.as_bytes();
    let mut in_str = false;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => {
                // Toggle unless this quote is escaped (odd run of backslashes).
                let mut bs = 0;
                let mut j = i;
                while j > 0 && b[j - 1] == b'\\' {
                    bs += 1;
                    j -= 1;
                }
                if bs % 2 == 0 {
                    in_str = !in_str;
                }
            }
            b'/' if !in_str && b.get(i + 1) == Some(&b'/') => return &line[..i],
            _ => {}
        }
        i += 1;
    }
    line
}

fn scan(dir: &Path, failures: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            scan(&path, failures);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("read file");
        for (n, line) in src.lines().enumerate() {
            let code = code_portion(line);
            for (glyph, name) in DENY {
                if code.contains(*glyph) {
                    failures.push(format!(
                        "{}:{} — broken glyph U+{:04X} ({name}); use PhosphorIcon or ASCII",
                        path.display(),
                        n + 1,
                        *glyph as u32,
                    ));
                }
            }
        }
    }
}

#[test]
fn no_broken_glyphs_in_rendered_strings() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut failures = Vec::new();
    scan(&src, &mut failures);
    assert!(
        failures.is_empty(),
        "found {} broken-glyph use(s) in rendered strings:\n{}",
        failures.len(),
        failures.join("\n"),
    );
}
