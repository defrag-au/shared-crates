//! miniquad's Java activity, rendered for one app.
//!
//! miniquad ships `java/MainActivity.java` with `//%` markers: the
//! preprocessor cargo-quad-apk ran over it. With no plugins every marker
//! becomes the empty string except one — the line that keeps the screen on
//! while the activity is showing, because an endless renderer must not let a
//! TV's screensaver in.
//!
//! A marker is a line whose first non-whitespace characters are `//%`, and the
//! rest of the line names it. An unknown marker is an **error**, not a line to
//! delete: miniquad gaining a marker means this renderer has to learn what it
//! means, and dropping it would compile a Java file with a feature missing and
//! report success.

/// Every marker miniquad 0.4.11's `MainActivity.java` carries, and the Java
/// that replaces it. An empty replacement removes the marker line.
pub const MARKERS: &[(&str, &str)] = &[
    ("IMPORTS", ""),
    ("QUAD_SURFACE_ON_CREATE_INPUT_CONNECTION", ""),
    ("RESIZING_LAYOUT_BODY", ""),
    ("RESIZING_LAYOUT_CONSTRUCTOR", ""),
    ("RESIZING_LAYOUT_ON_APPLY_WINDOW_INSETS", ""),
    ("MAIN_ACTIVITY_BODY", ""),
    ("MAIN_ACTIVITY_ON_CREATE", KEEP_SCREEN_ON),
    ("MAIN_ACTIVITY_ON_RESUME", ""),
    ("MAIN_ACTIVITY_ON_PAUSE", ""),
    ("MAIN_ACTIVITY_ON_ACTIVITY_RESULT", ""),
];

/// What is injected into `onCreate`: `LayoutParams` is already imported by
/// miniquad's activity, so this needs no import of its own.
pub const KEEP_SCREEN_ON: &str = "getWindow().addFlags(LayoutParams.FLAG_KEEP_SCREEN_ON);";

/// Render miniquad's `MainActivity.java` for `package` and `library`.
///
/// `TARGET_PACKAGE_NAME` and `LIBRARY_NAME` are miniquad's own placeholders,
/// the two strings its preprocessor substituted before compiling. Both must be
/// present — a miniquad that stopped using one would otherwise produce a Java
/// file with a literal `TARGET_PACKAGE_NAME` as its package.
pub fn render(source: &str, package: &str, library: &str) -> Result<String, String> {
    for token in ["TARGET_PACKAGE_NAME", "LIBRARY_NAME"] {
        if !source.contains(token) {
            return Err(format!(
                "miniquad's MainActivity.java no longer contains `{token}` — the renderer needs updating"
            ));
        }
    }

    let mut out = String::with_capacity(source.len());
    for (index, line) in source.split_inclusive('\n').enumerate() {
        let indent = line.len() - line.trim_start().len();
        let Some(rest) = line[indent..].strip_prefix("//%") else {
            out.push_str(line);
            continue;
        };
        let marker = rest.trim();
        let Some(replacement) = replacement(marker) else {
            return Err(format!(
                "line {}: unrecognised //% marker `{marker}` — miniquad has gained a marker this renderer does not know",
                index + 1
            ));
        };
        if !replacement.is_empty() {
            out.push_str(&line[..indent]);
            out.push_str(replacement);
            // Keep whatever line ending the source uses.
            out.push_str(if line.ends_with("\r\n") { "\r\n" } else { "\n" });
        }
    }
    Ok(out
        .replace("TARGET_PACKAGE_NAME", package)
        .replace("LIBRARY_NAME", library))
}

fn replacement(marker: &str) -> Option<&'static str> {
    MARKERS
        .iter()
        .find(|(name, _)| *name == marker)
        .map(|(_, replacement)| *replacement)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "\
package TARGET_PACKAGE_NAME;

import quad_native.QuadNative;

//% IMPORTS

public class MainActivity extends Activity {
    //% MAIN_ACTIVITY_BODY

    static {
        System.loadLibrary(\"LIBRARY_NAME\");
    }

    public void onCreate(Bundle savedInstanceState) {
        //% MAIN_ACTIVITY_ON_CREATE
    }

    //% MAIN_ACTIVITY_ON_RESUME
}
";

    #[test]
    fn fills_the_names_in_and_injects_only_what_is_known() {
        let out = render(SOURCE, "au.defrag.example", "example_mq").expect("renders");
        assert!(out.contains("package au.defrag.example;"));
        assert!(out.contains("System.loadLibrary(\"example_mq\");"));
        assert!(out.contains("        getWindow().addFlags(LayoutParams.FLAG_KEEP_SCREEN_ON);"));
        assert!(!out.contains("//%"));
        assert!(!out.contains("MAIN_ACTIVITY_BODY"));
        assert!(!out.contains("MAIN_ACTIVITY_ON_RESUME"));
    }

    #[test]
    fn an_unknown_marker_is_an_error_naming_it_and_its_line() {
        let source = "package TARGET_PACKAGE_NAME;\nclass A {\n    //% A_MARKER_FROM_A_NEWER_MINiquad\n}\nstatic { System.loadLibrary(\"LIBRARY_NAME\"); }\n";
        let err = render(source, "a.b", "l").expect_err("unknown marker");
        assert!(err.contains("line 3"), "{err}");
        assert!(err.contains("A_MARKER_FROM_A_NEWER_MINiquad"), "{err}");
    }

    #[test]
    fn a_missing_placeholder_is_an_error() {
        let err = render("package com.example;\n", "a.b", "l").expect_err("no token");
        assert!(err.contains("TARGET_PACKAGE_NAME"), "{err}");
        let err = render("package TARGET_PACKAGE_NAME;\n", "a.b", "l").expect_err("no library");
        assert!(err.contains("LIBRARY_NAME"), "{err}");
    }

    #[test]
    fn every_marker_miniquad_ships_is_known() {
        // The list is the contract: a miniquad bump that renames one fails in
        // `render` with the name in the message rather than here.
        assert_eq!(MARKERS.len(), 10);
        assert!(MARKERS.iter().all(|(name, _)| !name.is_empty()));
    }
}
