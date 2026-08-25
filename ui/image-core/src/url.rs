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

/// The other IIIF deployments in the estate.
///
/// Named rather than retyped: four hosts were in use, and a literal in a
/// module somewhere is how a fifth appears.
pub mod hosts {
    /// The main deployment, and [`super::DEFAULT_IIIF_BASE`].
    pub const HODLCROFT: &str = "https://iiif.hodlcroft.com/iiif/3";
    pub const AUGMINTS: &str = "https://iiif.augmints.xyz/iiif/3";
    pub const CNFT_DEV: &str = "https://iiif.cnft.dev/iiif/3";
    /// Dummy host for a Cloudflare **service binding** — resolved by the
    /// binding, never by DNS, so the hostname is arbitrary but must be stable.
    pub const SERVICE_BINDING: &str = "https://iiif-service/iiif/3";
}

/// Encoding IIIF should return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    /// Photographic art. The right default and what the warm derivatives are.
    #[default]
    Jpg,
    /// Lossless. Only where transparency actually matters — a PNG of a
    /// photographic render is several times the bytes for no visible gain.
    Png,
}

impl Format {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Jpg => "jpg",
            Self::Png => "png",
        }
    }
}

/// How to size the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeSpec {
    /// A width the service keeps warm. Use this.
    Warm(ImageSize),
    /// Fit inside a square — IIIF's `!{w},{h}`. Preserves aspect ratio and
    /// never exceeds either edge.
    SquareFit(u32),
    /// An arbitrary width.
    ///
    /// **Cold on every request.** The service resizes on the fly and caches
    /// nothing, so this is permanently slower with no visible symptom. Only
    /// correct for a *generator* creating derivatives, or a one-off. If you
    /// are rendering something a user waits for, you want [`Self::Warm`].
    Custom(u32),
}

impl SizeSpec {
    fn render(self) -> String {
        match self {
            Self::Warm(size) => format!("{},", size.px()),
            Self::SquareFit(px) => format!("!{px},{px}"),
            Self::Custom(px) => format!("{px},"),
        }
    }
}

/// Builder for the IIIF shapes that aren't the common case.
///
/// Reach for [`iiif_asset_url`] first; this exists so the handful of genuinely
/// different call sites — a PNG, a square crop, a generator's own size — can
/// still go through one implementation instead of hand-rolling a format
/// string and drifting.
#[derive(Debug, Clone)]
pub struct IiifUrl<'a> {
    base: &'a str,
    /// Already in IIIF's identifier form — either `{policy}:{hex}` or a single
    /// opaque segment such as a CIP-14 fingerprint.
    identifier: std::borrow::Cow<'a, str>,
    size: SizeSpec,
    format: Format,
}

impl<'a> IiifUrl<'a> {
    pub fn new(policy_id: &'a str, asset_name_hex: &'a str) -> Self {
        Self::from_identifier(format!("{policy_id}:{asset_name_hex}"))
    }

    /// Build from an identifier the caller already holds.
    ///
    /// The service also accepts a **CIP-14 fingerprint** (`asset1…`) as a
    /// single segment with no colon, and some callers only ever have that —
    /// the graphics worker's `AssetIdentifier`, the gallery's hero image id.
    /// They would otherwise have to hand-roll the URL, which is how the
    /// estate acquired thirteen builders in the first place.
    ///
    /// Note the fingerprint form costs the service a lookup to resolve back to
    /// policy + name, and can 404 for an asset it has never seen; prefer
    /// [`Self::new`] when both parts are available.
    pub fn from_identifier(identifier: impl Into<std::borrow::Cow<'a, str>>) -> Self {
        Self {
            base: DEFAULT_IIIF_BASE,
            identifier: identifier.into(),
            size: SizeSpec::Warm(ImageSize::Thumb),
            format: Format::Jpg,
        }
    }

    pub fn on(mut self, base: &'a str) -> Self {
        self.base = base;
        self
    }

    pub fn size(mut self, size: ImageSize) -> Self {
        self.size = SizeSpec::Warm(size);
        self
    }

    /// See [`SizeSpec::Custom`] — cold on every request.
    pub fn custom_px(mut self, px: u32) -> Self {
        self.size = SizeSpec::Custom(px);
        self
    }

    pub fn square_fit(mut self, px: u32) -> Self {
        self.size = SizeSpec::SquareFit(px);
        self
    }

    pub fn format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    pub fn build(&self) -> String {
        format!(
            "{}/{}/full/{}/0/default.{}",
            self.base.trim_end_matches('/'),
            self.identifier,
            self.size.render(),
            self.format.extension()
        )
    }
}

/// R2 object key for a cached derivative.
///
/// The same path shape as the URL, minus the host — which is deliberate on the
/// service's part, and why it belongs beside the URL builder rather than being
/// re-spelled in each worker that reads or writes the bucket. Two workers had
/// their own copies and had already diverged.
pub fn iiif_cache_key(policy_id: &str, asset_name_hex: &str, px: u32) -> String {
    format!("iiif-cache/{policy_id}/{asset_name_hex}/full/{px},/0/default.jpg")
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
            iiif_url_on(
                "https://iiif-service/iiif/3",
                POLICY,
                ASSET,
                ImageSize::Full
            ),
            format!("https://iiif-service/iiif/3/{POLICY}:{ASSET}/full/1646,/0/default.jpg")
        );
        // A trailing slash on the base must not produce a doubled separator,
        // which some IIIF servers 404 rather than normalise.
        assert_eq!(
            iiif_url_on(
                "https://example.test/iiif/3/",
                POLICY,
                ASSET,
                ImageSize::Thumb
            ),
            iiif_url_on(
                "https://example.test/iiif/3",
                POLICY,
                ASSET,
                ImageSize::Thumb
            ),
        );
    }

    #[test]
    fn the_builder_reproduces_every_shape_found_in_the_estate() {
        // These are the exact shapes the migration replaces. If the builder
        // can't reproduce them byte for byte, migrating a call site silently
        // changes what it fetches.

        // bot-db: the only PNG caller.
        assert_eq!(
            IiifUrl::new(POLICY, ASSET)
                .custom_px(500)
                .format(Format::Png)
                .build(),
            format!("https://iiif.hodlcroft.com/iiif/3/{POLICY}:{ASSET}/full/500,/0/default.png")
        );

        // egui-widgets' square-fit thumbnail.
        assert_eq!(
            IiifUrl::new(POLICY, ASSET).square_fit(48).build(),
            format!("https://iiif.hodlcroft.com/iiif/3/{POLICY}:{ASSET}/full/!48,48/0/default.jpg")
        );

        // game-sessions, on the third host.
        assert_eq!(
            IiifUrl::new(POLICY, ASSET).on(hosts::CNFT_DEV).build(),
            format!("https://iiif.cnft.dev/iiif/3/{POLICY}:{ASSET}/full/400,/0/default.jpg")
        );

        // The service-binding form used inside workers.
        assert_eq!(
            IiifUrl::new(POLICY, ASSET)
                .on(hosts::SERVICE_BINDING)
                .build(),
            format!("https://iiif-service/iiif/3/{POLICY}:{ASSET}/full/400,/0/default.jpg")
        );

        // And the plain default agrees with the free function, so the two
        // entry points can never diverge.
        assert_eq!(
            IiifUrl::new(POLICY, ASSET).size(ImageSize::Full).build(),
            iiif_asset_url(POLICY, ASSET, ImageSize::Full)
        );
    }

    #[test]
    fn an_opaque_identifier_is_emitted_verbatim() {
        // A CIP-14 fingerprint has no colon, so it cannot be composed from
        // policy + name. Two call sites hold only this form.
        assert_eq!(
            IiifUrl::from_identifier("asset1abcdefghijklmnop")
                .custom_px(1200)
                .build(),
            "https://iiif.hodlcroft.com/iiif/3/asset1abcdefghijklmnop/full/1200,/0/default.jpg"
        );
        // And the two constructors agree where both apply.
        assert_eq!(
            IiifUrl::from_identifier(format!("{POLICY}:{ASSET}")).build(),
            IiifUrl::new(POLICY, ASSET).build()
        );
    }

    #[test]
    fn the_r2_cache_key_matches_the_url_path() {
        // The bucket key is the URL path minus the host — by the service's
        // design. Two workers kept their own copies and had diverged.
        let key = iiif_cache_key(POLICY, ASSET, 400);
        assert_eq!(
            key,
            format!("iiif-cache/{POLICY}/{ASSET}/full/400,/0/default.jpg")
        );
        // The tail after the identifier is identical in both — that shared
        // suffix is the thing that must not drift between them.
        let suffix = "/full/400,/0/default.jpg";
        assert!(key.ends_with(suffix));
        assert!(iiif_asset_url(POLICY, ASSET, ImageSize::Thumb).ends_with(suffix));
    }

    #[test]
    fn the_width_comma_survives() {
        // `full/400,/` — the comma means "this width, height automatic".
        // Losing it is a 400 from the service, and it is the single easiest
        // character to drop when hand-editing a format string.
        assert!(iiif_asset_url(POLICY, ASSET, ImageSize::Thumb).contains("/full/400,/0/"));
    }
}
