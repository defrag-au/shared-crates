//! IIIF URLs and the sizes that are actually cached.

/// The IIIF deployment most consumers talk to.
///
/// Others exist (`iiif.augmints.xyz`, `iiif.cnft.dev`, and the
/// `https://iiif-service` dummy host used over a Cloudflare service binding),
/// which is exactly why [`iiif_url_on`] takes a base — but a default stops
/// every caller inventing one.
pub const DEFAULT_IIIF_BASE: &str = "https://iiif.hodlcroft.com/iiif/3";

/// A size the IIIF service keeps **warm**.
///
/// # This enum exists to prevent one specific, invisible bug
///
/// The service pre-generates derivatives at these widths. Ask for anything
/// else and you still get an image — resized on the fly, every request, cache
/// missing forever. Nothing fails, so nothing gets noticed; it just costs
/// latency indefinitely.
///
/// It has already happened at least twice: one enum in the estate drifted to
/// `1686`, another to `1626`, and a storybook asked for 48 and 32. Six
/// independent copies of this enum is why. There is now one, and adding a
/// variant means confirming the service actually warms it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ImageSize {
    /// 400px wide. Thumbnails, grids, pickers.
    #[default]
    Thumb,
    /// 1646px wide. Detail views and print-ish renders.
    Full,
}

impl ImageSize {
    /// Pixel width, as IIIF's `{w},` parameter takes it.
    pub const fn px(self) -> u32 {
        match self {
            // Do not "tidy" these to round numbers. They match derivatives the
            // service generates; a nearby value is a permanent cache miss.
            Self::Thumb => 400,
            Self::Full => 1646,
        }
    }

    /// Every warm size, for callers that want to prefetch or validate.
    pub const ALL: [ImageSize; 2] = [Self::Thumb, Self::Full];

    /// Is this width one the service keeps warm?
    ///
    /// For validating a size that arrived as a number — from config, a URL, or
    /// another service — rather than as this enum.
    pub fn is_warm(px: u32) -> bool {
        Self::ALL.iter().any(|s| s.px() == px)
    }
}

/// Image URL for a Cardano asset on the default IIIF deployment.
///
/// `policy_id` is the 56-char hex policy; `asset_name_hex` is the **hex**
/// asset name, not the decoded display name. For CIP-68 collections the caller
/// should already have swapped the user token's label for the reference
/// token's — that is where the image lives, and it is a chain-level detail
/// this crate deliberately doesn't re-derive.
pub fn iiif_asset_url(policy_id: &str, asset_name_hex: &str, size: ImageSize) -> String {
    iiif_url_on(DEFAULT_IIIF_BASE, policy_id, asset_name_hex, size)
}

/// As [`iiif_asset_url`], against a specific deployment.
///
/// `base` is everything up to and including `/iiif/3`, with no trailing slash
/// — e.g. `https://iiif-service/iiif/3` for a Cloudflare service binding, or a
/// same-origin proxy path for a surface under a restrictive CSP.
pub fn iiif_url_on(base: &str, policy_id: &str, asset_name_hex: &str, size: ImageSize) -> String {
    // The trailing comma in `{w},` is IIIF's "this width, height automatic".
    // It is not a typo and dropping it is a 400 from the service.
    format!(
        "{}/{policy_id}:{asset_name_hex}/full/{},/0/default.jpg",
        base.trim_end_matches('/'),
        size.px()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: &str = "285c0b8e91ba323da4ca083c9db837e111dafbf3143ece4d03eba8f4";
    const ASSET: &str = "546f6f6c6865616430323230";

    #[test]
    fn warm_sizes_are_exactly_the_two_the_service_generates() {
        // The regression this crate exists to prevent. If someone "rounds"
        // these, every image silently starts missing the cache.
        assert_eq!(ImageSize::Thumb.px(), 400);
        assert_eq!(ImageSize::Full.px(), 1646);

        assert!(ImageSize::is_warm(400));
        assert!(ImageSize::is_warm(1646));
        // The values that have actually shipped in this estate by mistake.
        assert!(!ImageSize::is_warm(1686), "the drift that shipped");
        assert!(!ImageSize::is_warm(1626), "the drift before that");
        assert!(!ImageSize::is_warm(48));
        assert!(!ImageSize::is_warm(32));
    }

    #[test]
    fn url_shape_matches_what_the_service_serves() {
        assert_eq!(
            iiif_asset_url(POLICY, ASSET, ImageSize::Thumb),
            format!("https://iiif.hodlcroft.com/iiif/3/{POLICY}:{ASSET}/full/400,/0/default.jpg")
        );
    }

    #[test]
    fn a_custom_base_is_used_verbatim_minus_a_trailing_slash() {
        // Service bindings and same-origin proxies both matter: a Discord
        // Activity cannot reach an absolute IIIF host at all under its CSP.
        assert_eq!(
            iiif_url_on("https://iiif-service/iiif/3", POLICY, ASSET, ImageSize::Full),
            format!("https://iiif-service/iiif/3/{POLICY}:{ASSET}/full/1646,/0/default.jpg")
        );
        // A trailing slash on the base must not produce a doubled separator,
        // which some IIIF servers 404 rather than normalise.
        assert_eq!(
            iiif_url_on("https://example.test/iiif/3/", POLICY, ASSET, ImageSize::Thumb),
            iiif_url_on("https://example.test/iiif/3", POLICY, ASSET, ImageSize::Thumb),
        );
    }

    #[test]
    fn the_width_comma_survives() {
        // `full/400,/` — the comma means "this width, height automatic".
        // Losing it is a 400 from the service, and it is the single easiest
        // character to drop when hand-editing a format string.
        assert!(iiif_asset_url(POLICY, ASSET, ImageSize::Thumb).contains("/full/400,/0/"));
    }
}
