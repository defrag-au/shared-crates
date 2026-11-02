//! The app's `AndroidManifest.xml`, checked against the package being built.
//!
//! The manifest is the app's own — its permissions, leanback features, activity
//! and theme are not the packager's business. One thing is: the `package`
//! attribute decides what `.MainActivity` resolves to, and the Java is compiled
//! into the package `--package` names. If the two disagree the APK builds and
//! then dies on launch with a `ClassNotFoundException`, so the packager renders
//! the manifest and refuses the mismatch here instead.
//!
//! A manifest may write `package="${package}"` (or miniquad's
//! `TARGET_PACKAGE_NAME`) when it is shared between apps; the attribute is
//! substituted. Nothing else in the file is rewritten.

/// Render `source` with `package` as the manifest's package.
pub fn render(source: &str, package: &str) -> Result<String, String> {
    let start = source
        .find("<manifest")
        .ok_or("no <manifest> element — is this an Android manifest?")?;
    let end = source[start..]
        .find('>')
        .map(|offset| start + offset)
        .ok_or("the <manifest> element is not closed")?;
    let tag = &source[start..end];
    let (value, declared) = attribute(tag, "package").ok_or(
        "the <manifest> element has no package attribute — aapt2 cannot name the APK without one",
    )?;

    if declared == package {
        return Ok(source.to_string());
    }
    if declared != "${package}" && declared != "TARGET_PACKAGE_NAME" {
        return Err(format!(
            "the manifest declares package `{declared}`, but --package is `{package}` — the Java would be compiled into one and launched as the other"
        ));
    }

    // Splice the value in place so the attribute keeps its quote style.
    let mut rewritten = String::with_capacity(tag.len() + package.len());
    rewritten.push_str(&tag[..value.start]);
    rewritten.push_str(package);
    rewritten.push_str(&tag[value.end..]);
    let mut out = String::with_capacity(source.len() + package.len());
    out.push_str(&source[..start]);
    out.push_str(&rewritten);
    out.push_str(&source[end..]);
    Ok(out)
}

/// The value of `name` in an XML start tag, single- or double-quoted, as a
/// byte range within `tag`.
fn attribute(tag: &str, name: &str) -> Option<(std::ops::Range<usize>, String)> {
    for quote in ['"', '\''] {
        let pattern = format!("{name}={quote}");
        if let Some(start) = tag.find(&pattern) {
            let value_start = start + pattern.len();
            let value_end = value_start + tag[value_start..].find(quote)?;
            return Some((
                value_start..value_end,
                tag[value_start..value_end].to_string(),
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!-- The package is the app's identity. -->
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
        package="au.defrag.example"
        android:versionCode="1">
    <application android:label="example" />
</manifest>
"#;

    #[test]
    fn a_manifest_that_already_names_the_package_is_unchanged() {
        assert_eq!(
            render(MANIFEST, "au.defrag.example").expect("matches"),
            MANIFEST
        );
    }

    #[test]
    fn a_placeholder_is_filled_in_and_nothing_else_is_touched() {
        let manifest = MANIFEST.replace("au.defrag.example", "${package}");
        let out = render(&manifest, "au.defrag.other").expect("renders");
        assert!(out.contains("package=\"au.defrag.other\""));
        assert!(out.contains("<!-- The package is the app's identity. -->"));
        assert!(!out.contains("${package}"));
    }

    #[test]
    fn a_different_package_is_named_rather_than_packaged() {
        let err = render(MANIFEST, "au.defrag.other").expect_err("mismatch");
        assert!(err.contains("au.defrag.example"), "{err}");
        assert!(err.contains("au.defrag.other"), "{err}");
    }

    #[test]
    fn a_manifest_without_a_package_is_an_error() {
        let err = render("<manifest></manifest>", "a.b").expect_err("no attribute");
        assert!(err.contains("package"), "{err}");
    }

    #[test]
    fn a_single_quoted_attribute_is_read() {
        let manifest = "<manifest package='a.b'></manifest>";
        assert_eq!(render(manifest, "a.b").expect("matches"), manifest);
        assert_eq!(
            render("<manifest package='${package}'></manifest>", "c.d").expect("renders"),
            "<manifest package='c.d'></manifest>"
        );
    }

    #[test]
    fn something_that_is_not_a_manifest_is_an_error() {
        assert!(render("<resources />", "a.b").is_err());
        assert!(render("<manifest", "a.b").is_err());
    }
}
