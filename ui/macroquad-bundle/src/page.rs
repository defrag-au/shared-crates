//! The bundle's `index.html`, rendered from a shell template.

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

/// Render `template`. `scripts` are the `src` values in load order, already
/// prefixed with any base path; `wasm` likewise, with its cache key.
pub fn render(
    template: &str,
    title: &str,
    scripts: &[String],
    wasm: &str,
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
    Ok(template
        .replace("__TITLE__", &escape(title))
        .replace("__SCRIPTS__", &tags)
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
        )
        .expect("render");
        for t in TOKENS {
            assert!(!html.contains(t), "{t} left in the page");
        }
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
        assert!(render("<html>__TITLE__ __WASM__</html>", "t", &[], "w").is_err());
    }
}
