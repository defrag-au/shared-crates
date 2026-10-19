//! The bundle's `index.html`, rendered from a shell template, and its web
//! app manifest.

use serde::Serialize;

/// The shared shell: WebGL pre-flight, boot panel, panic mirroring.
pub const SHELL: &str = include_str!("../shell.html");

const TOKENS: [&str; 3] = ["__TITLE__", "__SCRIPTS__", "__WASM__"];

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Drop an authoring comment that sits before `<html`: it names the tokens,
/// and would otherwise have them substituted into their own documentation.
fn strip_authoring_comment(template: &str) -> String {
    let (Some(open), Some(html)) = (template.find("<!--"), template.find("<html")) else {
        return template.to_owned();
    };
    if open > html {
        return template.to_owned();
    }
    match template[open..].find("-->") {
        Some(len) => {
            let after = open + len + 3;
            let rest = template[after..].trim_start();
            format!("{}{rest}", &template[..open])
        }
        None => template.to_owned(),
    }
}

/// The bundle's web app manifest, beside `index.html`.
pub const MANIFEST: &str = "manifest.webmanifest";

/// A web app manifest: what a phone needs to launch the bundle from its home
/// screen fullscreen, without the browser's bars.
#[derive(Serialize)]
pub struct WebManifest<'a> {
    pub name: &'a str,
    pub short_name: &'a str,
    pub display: &'static str,
    pub orientation: &'static str,
    pub background_color: &'static str,
    pub theme_color: &'static str,
    pub start_url: &'static str,
    pub scope: &'static str,
}

/// The manifest for a bundle titled `title`: fullscreen, any orientation, the
/// shell's background. URLs are relative to the manifest, so it works at any
/// base path.
pub fn manifest(title: &str) -> String {
    let m = WebManifest {
        name: title,
        short_name: title,
        display: "fullscreen",
        orientation: "any",
        background_color: "#050510",
        theme_color: "#050510",
        start_url: "./",
        scope: "./",
    };
    serde_json::to_string_pretty(&m).expect("a manifest of strings serialises")
}

/// Render `template`. `scripts` are the `src` values in load order, already
/// prefixed with any base path; `wasm` likewise, with its cache key; `base`
/// the base path itself, for the manifest link (`__MANIFEST__`, optional).
pub fn render(
    template: &str,
    title: &str,
    scripts: &[String],
    wasm: &str,
    base: &str,
) -> Result<String, String> {
    let template = strip_authoring_comment(template);
    if let Some(missing) = TOKENS.iter().find(|t| !template.contains(**t)) {
        return Err(format!("shell template has no {missing} token"));
    }
    let tags = scripts
        .iter()
        .map(|src| format!("  <script src=\"{}\"></script>", escape(src)))
        .collect::<Vec<_>>()
        .join("\n");
    let manifest = format!(
        "  <link rel=\"manifest\" href=\"{}\">",
        escape(&format!("{base}{MANIFEST}"))
    );
    Ok(template
        .replace("__TITLE__", &escape(title))
        .replace("__SCRIPTS__", &tags)
        .replace("__MANIFEST__", &manifest)
        .replace("__WASM__", &escape(wasm)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_shell_renders_every_token() {
        let html = render(
            SHELL,
            "A & B",
            &["gl.js".into(), "/pump/x.js".into()],
            "g.wasm?v=1",
            "/pump/",
        )
        .expect("render");
        for t in TOKENS.iter().chain(&["__MANIFEST__"]) {
            assert!(!html.contains(t), "{t} left in the page");
        }
        assert!(html.contains("<link rel=\"manifest\" href=\"/pump/manifest.webmanifest\">"));
        assert!(html.contains("apple-mobile-web-app-capable"));
        assert!(html.contains("<title>A &amp; B</title>"));
        assert!(
            html.contains(
                "<script src=\"gl.js\"></script>\n  <script src=\"/pump/x.js\"></script>"
            )
        );
        assert!(html.contains("load(\"g.wasm?v=1\")"));
        assert!(
            html.starts_with("<!DOCTYPE html>\n<html"),
            "authoring comment kept"
        );
    }

    #[test]
    fn a_template_without_a_token_is_refused() {
        assert!(render("<html>__TITLE__ __WASM__</html>", "t", &[], "w", "").is_err());
    }

    #[test]
    fn a_custom_shell_need_not_link_a_manifest() {
        let html = render(
            "<html>__TITLE__ __SCRIPTS__ __WASM__</html>",
            "t",
            &[],
            "w",
            "",
        )
        .expect("renders without __MANIFEST__");
        assert!(!html.contains("manifest"));
    }

    #[test]
    fn the_manifest_launches_fullscreen_in_any_orientation() {
        let m: serde_json::Value =
            serde_json::from_str(&manifest("Island \"Funk\"")).expect("json");
        assert_eq!(m["name"], "Island \"Funk\"");
        assert_eq!(m["display"], "fullscreen");
        assert_eq!(m["orientation"], "any");
        assert_eq!(m["start_url"], "./");
    }
}
