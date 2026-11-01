//! Where a token's picture is asked for — once, for every surface.
//!
//! A policy's picture is not one URL. Two services hold one, they disagree
//! visibly when both answer, and which of them has an answer depends on the
//! token: a registered fungible has the project's own brand mark in the token
//! registry, while a launchpad token has neither and keeps its initial.
//!
//! So the policy is an ORDERED list, asked one at a time, and the next tried
//! only when the one before failed for good:
//!
//! 1. the token registry's logo, through the worker
//!    (`/api/token-logo/{policy}/{unit}`): the project's own brand mark, and
//!    authoritative where it exists. Same-origin, so the browser only — a
//!    native capture has no origin to ask.
//! 2. the IIIF asset image for the same policy and unit, from the CIP-25 or
//!    CIP-68 metadata. MEASURED: `$PERP` has one ("PERP COIN"); `$BUNNY`, a
//!    lumpfun launch, has neither, and keeps its initial.
//!
//! **Failure means permanent failure.** A 404, a 403 or a body that will not
//! decode advances to the next URL; a network error does not, because retrying
//! the same URL is cheaper and likelier than falling back to a different
//! picture. `image-miniquad`'s `FetchError::is_permanent` is that question.
//!
//! `unit` is the asset name in HEX (`5045525020434f494e`), not bytes and not
//! the ticker. A reader that has only a policy cannot ask: `summary.unit` is
//! where the holder map gets it, and the tape's `unit` is where the arena does.

use crate::url::{Format, IiifUrl, ImageSize};

/// Where to ask for the picture of `policy`'s token `unit` (its asset name,
/// hex), best first. Empty when `unit` is — there is nothing to ask for.
pub fn policy_picture_urls(policy: &str, unit: &str) -> Vec<String> {
    if unit.is_empty() {
        return Vec::new();
    }
    let mut urls = Vec::new();
    if cfg!(target_arch = "wasm32") {
        urls.push(format!("/api/token-logo/{policy}/{unit}"));
    }
    // The browser decodes whatever the service picks; natively macroquad
    // decodes PNG and nothing else.
    let format = if cfg!(target_arch = "wasm32") {
        Format::Auto
    } else {
        Format::Png
    };
    urls.push(
        IiifUrl::new(policy, unit)
            .size(ImageSize::Thumb)
            .format(format)
            .build(),
    );
    urls
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: &str = "d9d59e71eb99e653a493956a6826a406060706bd8c9a2427b3564f64";

    #[test]
    fn a_token_without_its_asset_name_asks_for_nothing() {
        assert!(policy_picture_urls(POLICY, "").is_empty());
    }

    /// The asset image is always asked for, by the policy AND the name — it is
    /// the fallback a native capture can reach, where there is no origin for
    /// the registry route.
    #[test]
    fn the_asset_image_is_asked_for_by_policy_and_name() {
        let urls = policy_picture_urls(POLICY, "5045525020434f494e");
        let last = urls.last().expect("an image url");
        assert!(
            last.contains(&format!("{POLICY}:5045525020434f494e")),
            "{last}"
        );
    }

    #[cfg(target_arch = "wasm32")]
    #[test]
    fn the_browser_asks_the_worker_first() {
        let urls = policy_picture_urls(POLICY, "41");
        assert_eq!(
            urls.first().map(String::as_str),
            Some("/api/token-logo/d9d59e71eb99e653a493956a6826a406060706bd8c9a2427b3564f64/41")
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn natively_the_registry_route_is_not_asked_for() {
        let urls = policy_picture_urls(POLICY, "41");
        assert!(
            urls.iter().all(|u| u.starts_with("https://iiif.")),
            "a relative url has no origin to resolve against: {urls:?}"
        );
    }
}
