//! Guard: text sizes come from the THEME, never from a literal.
//!
//! `theme.rs` exists because the suite once had ~294 inline `.size(11.0)`
//! literals and no way to change the type ramp, the density or the scale
//! without touching every one of them. That migration is essentially done.
//! This test stops it coming back.
//!
//! ## Why a test and not clippy
//!
//! `clippy::disallowed_methods` matches a method PATH, not its arguments — it
//! cannot tell `.size(11.0)` from `.size(ui.text_size(TextSize::Base))`, and
//! banning `RichText::size` outright would ban the blessed form too. A custom
//! lint would mean dylint, a second toolchain and a second thing to keep
//! working. A source scan is exact, runs in `cargo test`, and reads like the
//! rule it enforces — the same contract as `tests/catalog.rs` and
//! `tests/contrast.rs`.
//!
//! ## The rule
//!
//! In a widget module, `.size(…)` takes a resolved theme step:
//!
//! ```ignore
//! // NO  — a size no theme switch and no density setting can reach
//! RichText::new(label).size(11.0)
//! // YES — the ramp decides, the widget asks
//! RichText::new(label).size(ui.text_size(TextSize::Base))
//! ```
//!
//! A config struct holds `TextSize` steps rather than `f32`, resolved in
//! `show()` where a `Ui` is finally available. `route_quote` and
//! `pool_inspector` are worked examples.
//!
//! Genuinely non-textual `.size(…)` — a pixel dimension like
//! `ImageStack::size(96.0)` — opts out with a trailing `// theme-exempt:`
//! naming the reason.

use std::collections::BTreeSet;
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

/// Strip a line comment, so a `.size(11.0)` inside prose is not a violation.
/// Crude on purpose: a `//` inside a string literal is not something this
/// crate's sources do, and the cost of being wrong is a false positive with a
/// clear message rather than a silent miss.
fn code_of(line: &str) -> &str {
    match line.find("//") {
        Some(at) => &line[..at],
        None => line,
    }
}

/// Does this line pass a bare numeric literal to `.size(`?
fn literal_size_call(code: &str) -> bool {
    let Some(at) = code.find(".size(") else {
        return false;
    };
    let rest = &code[at + ".size(".len()..];
    let arg: String = rest
        .chars()
        .take_while(|c| *c != ')')
        .filter(|c| !c.is_whitespace())
        .collect();
    !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '_')
}

/// No widget may write a point size down.
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

        for (index, line) in source.lines().enumerate() {
            if line.contains("theme-exempt:") {
                continue;
            }
            if literal_size_call(code_of(line)) {
                offenders.push(format!("  {name}:{} — {}", index + 1, line.trim()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "a widget wrote a point size down instead of asking the theme:\n{}\n\n\
         Use `ui.text_size(TextSize::…)`, and hold `TextSize` in the widget's \
         config rather than `f32` — see `route_quote` / `pool_inspector`. A \
         genuinely non-textual size (a pixel dimension) opts out with a \
         trailing `// theme-exempt: <reason>`.",
        offenders.join("\n")
    );
}

/// Config structs hold theme STEPS, not point sizes.
///
/// A ratchet rather than a hard zero: this is the second half of the same
/// migration and several widgets predate it. The count may only ever go DOWN
/// — converting one is a handful of lines, and the failure message says so.
///
/// Scoped to fields that are unambiguously TEXT. A widget legitimately owns
/// pixel dimensions — `thumb_size: 72.0`, `icon_size: 20.0`,
/// `hero_size: 32.0` — and those have nothing to do with the type ramp;
/// flagging them would push someone to "fix" them into a `TextSize`, which
/// would be worse than leaving them alone.
#[test]
fn widget_configs_do_not_carry_point_sizes() {
    /// Field names that can only mean type.
    const TEXT_FIELDS: &[&str] = &[
        "font_size",
        "text_size",
        "title_size",
        "heading_size",
        "label_size",
        "value_size",
        "total_size",
        "caption_size",
        "legend_size",
        "name_size",
        "ticker_size",
        "qty_size",
    ];

    /// Every remaining text-size field default in a widget config.
    ///
    /// Lower this as widgets convert. It must never rise: a new widget has
    /// `TextSize` available from the first line, so there is no reason to add
    /// one.
    ///
    /// 33 at the time this guard was written — the tail of the `.size(11.0)`
    /// migration that `theme.rs` describes. `route_quote` and
    /// `pool_inspector` are the converted shape to copy.
    const BASELINE: usize = 33;

    let mut found: BTreeSet<String> = BTreeSet::new();

    for path in widget_sources() {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<unknown>")
            .to_string();

        for (index, line) in source.lines().enumerate() {
            let code = code_of(line).trim();
            // `font_size: 11.0,` and friends — a literal point size standing
            // in for a ramp step.
            let Some((field, value)) = code.split_once(':') else {
                continue;
            };
            let field = field.trim();
            if !TEXT_FIELDS.contains(&field) {
                continue;
            }
            let value: String = value
                .trim()
                .trim_end_matches(',')
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            if !value.is_empty()
                && value
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.' || c == '_')
                && value.contains('.')
            {
                found.insert(format!("  {name}:{} — {code}", index + 1));
            }
        }
    }

    assert!(
        found.len() <= BASELINE,
        "widget configs carrying point sizes rose to {} (baseline {BASELINE}):\n{}\n\n\
         A config field should be a `TextSize`, resolved in `show()` where a \
         `Ui` exists — see `route_quote::RouteQuoteConfig`. If you converted \
         one, LOWER the baseline.",
        found.len(),
        found.into_iter().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn the_detector_recognises_both_forms() {
    // The thing we are banning.
    assert!(literal_size_call(".size(11.0)"));
    assert!(literal_size_call("    .size( 9.5 )"));
    assert!(literal_size_call(".size(16)"));
    // The blessed form, and anything else derived.
    assert!(!literal_size_call(".size(ui.text_size(TextSize::Base))"));
    assert!(!literal_size_call(".size(sizes.body)"));
    assert!(!literal_size_call(".size(config.font_size - 1.0)"));
    assert!(!literal_size_call("let x = 11.0;"));
    // A literal inside prose is not code.
    assert_eq!(code_of("    // was .size(11.0) before").trim(), "");
}
