//! Guard: text sizes come from the THEME, never from a literal.
//!
//! The macroquad sibling of `egui-widgets/tests/theme_tokens.rs`, and it exists
//! because that crate had the guard and this one did not. The asymmetry was not
//! theoretical: every point size on this side was a bare literal, so no theme
//! switch, no density setting and no accessibility scale could reach any of
//! them. `Theme` now carries a `ui_theme::TypeScale`; this test is what stops
//! the literals coming back.
//!
//! ## Why a test and not the type system
//!
//! Making `Painter::text` take a `TextSize` directly would be stronger — a
//! literal simply would not compile. It is not what this does, because a
//! handful of sizes here are *genuinely* derived from geometry
//! (`quantity_stepper` sizes its `−`/`+` glyphs from the row height, so the
//! control scales as one piece). Those are real and they must stay
//! expressible. A scan with a named opt-out keeps them sayable while still
//! making the default path the themed one — the same trade `theme_tokens.rs`
//! makes on the egui side, enforced the same way, which is the point.
//!
//! ## The rule
//!
//! ```ignore
//! // NO  — a size no theme switch and no ramp can reach
//! p.text(label, x, y, 14.0, col);
//! // YES — the ramp decides, the widget asks
//! p.text(label, x, y, p.size(TextSize::Sm), col);
//! // YES — knows what its text IS, so it takes the role
//! p.text(label, x, y, p.role(TextRole::Body), col);
//! ```
//!
//! A size that is genuinely a geometric proportion opts out with a trailing
//! `// theme-exempt: <reason>` on any line of the call.

use std::path::{Path, PathBuf};

/// Widget sources, excluding the theme itself — it DEFINES the ramp, so it is
/// the one place a point size is allowed to be written down.
fn widget_sources() -> Vec<PathBuf> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect(&src, &mut files);
    files.retain(|p| p.file_name().is_some_and(|n| n != "theme.rs"));
    files.sort();
    files
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every call that takes a point size, and which argument it is.
///
/// Positional rather than named, so the index is part of the contract. If one
/// of these signatures gains a parameter ahead of the size, this table is what
/// has to move with it — and a wrong index here shows up as a false positive
/// with the offending line printed, not as a silent hole.
const SIZE_ARGS: &[(&str, usize)] = &[
    (".text(", 3),
    (".mono(", 3),
    (".text_top(", 3),
    (".measure(", 1),
    (".measure_mono(", 1),
    (".centre_baseline(", 2),
    (".top_baseline(", 1),
    (".font_size(", 0),
];

/// Split a call's argument list on TOP-LEVEL commas.
///
/// Nested calls are the common case here — `p.text(&format!("{a}, {b}"), …)` —
/// so a naive `split(',')` would mis-index and flag the wrong argument.
fn top_level_args(inside: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    let mut in_str = false;
    let mut escaped = false;

    for ch in inside.chars() {
        if in_str {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_str = true;
                current.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }
    args
}

/// The body of a call starting at `open` (the index of its `(`), and the byte
/// index just past its closing paren.
fn call_body(source: &str, open: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;

    for (offset, &b) in bytes.iter().enumerate().skip(open) {
        let ch = b as char;
        if in_str {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((source[open + 1..offset].to_string(), offset + 1));
                }
            }
            _ => {}
        }
    }
    None
}

/// A bare number — `14.0`, `16`, `1_0.0`. An expression that merely *contains*
/// one (`h * 0.55`, `base + 2.0`) is not: those are geometry, and the opt-out
/// comment is how a reader tells them apart from a forgotten migration.
fn is_bare_number(arg: &str) -> bool {
    let arg = arg.trim();
    !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '_')
}

fn line_of(source: &str, byte: usize) -> usize {
    source[..byte].matches('\n').count() + 1
}

/// Does the call spanning `[start, end)` carry the opt-out marker?
fn exempt(source: &str, start: usize, end: usize) -> bool {
    let first = source[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let last = source[end..]
        .find('\n')
        .map(|i| end + i)
        .unwrap_or(source.len());
    source[first..last].contains("theme-exempt:")
}

#[test]
fn text_sizes_come_from_the_theme() {
    let mut offenders: Vec<String> = Vec::new();

    for path in widget_sources() {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<unknown>")
            .to_string();

        for (call, index) in SIZE_ARGS {
            let mut from = 0usize;
            while let Some(found) = source[from..].find(call) {
                let at = from + found;
                let open = at + call.len() - 1;
                let Some((inside, end)) = call_body(&source, open) else {
                    from = at + call.len();
                    continue;
                };
                from = end;

                let args = top_level_args(&inside);
                let Some(arg) = args.get(*index) else {
                    continue;
                };
                if is_bare_number(arg) && !exempt(&source, at, end) {
                    let line = line_of(&source, at);
                    offenders.push(format!(
                        "  {name}:{line} — {}{} takes a literal {arg}",
                        call.trim_start_matches('.').trim_end_matches('('),
                        format_args!("()")
                    ));
                }
            }
        }
    }

    offenders.sort();
    assert!(
        offenders.is_empty(),
        "a widget wrote a point size down instead of asking the theme:\n{}\n\n\
         Use `p.size(TextSize::…)`, or `p.role(TextRole::…)` when the call site \
         knows what its text is. A size that is genuinely a geometric \
         proportion (a glyph scaled to its row height) opts out with a \
         trailing `// theme-exempt: <reason>`.",
        offenders.join("\n")
    );
}
