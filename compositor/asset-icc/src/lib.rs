//! ICC-aware asset image loading.
//!
//! Procreate exports embed a **Display P3** profile; the raw pixel numbers are P3
//! coordinates, and reading them as sRGB quietly desaturates every chromatic pixel
//! (~⅓ of a saturated cyan's chroma; greys and near-blacks are identical in both
//! spaces, which is why grey-keyed region matching never breaks). [`open_rgba`]
//! honours the profile at decode so downstream pipelines and previews see the
//! colours the artist approved on their canvas.
//!
//! Scope is deliberately narrow: Display P3 is detected by name and converted with
//! a fixed matrix (P3 shares sRGB's transfer curve). Other embedded profiles pass
//! through untouched — sRGB-profiled files are already correct, and anything more
//! exotic should be a conscious decision, not a silent transform.

use anyhow::{Context, Result};
use image::RgbaImage;
use std::path::Path;

/// Content-sniffed, ICC-aware decode to straight-alpha RGBA. The sniffing
/// (`with_guessed_format`) matters because artist drops recurrently mislabel
/// extensions (webp bytes in a `.png`), and the extension-keyed decoder chokes.
pub fn open_rgba(path: &Path) -> Result<RgbaImage> {
    let mut decoder = image::ImageReader::open(path)
        .with_context(|| format!("failed to open image {}", path.display()))?
        .with_guessed_format()
        .with_context(|| format!("failed to sniff image format {}", path.display()))?
        .into_decoder()
        .with_context(|| format!("failed to decode image {}", path.display()))?;
    use image::ImageDecoder;
    let icc = decoder.icc_profile().ok().flatten();
    let img = image::DynamicImage::from_decoder(decoder)
        .with_context(|| format!("failed to decode image {}", path.display()))?
        .to_rgba8();
    Ok(match icc {
        Some(profile) if is_display_p3(&profile) => display_p3_to_srgb(img),
        _ => img,
    })
}

/// `true` if an embedded ICC profile is Apple's "Display P3" (the profile every
/// Procreate export carries). The description tag is UTF-16BE inside the `mluc`
/// record, so match that byte pattern rather than pulling in a CMS.
pub fn is_display_p3(profile: &[u8]) -> bool {
    const NAME_UTF16BE: &[u8] = b"\x00D\x00i\x00s\x00p\x00l\x00a\x00y\x00 \x00P\x003";
    profile
        .windows(NAME_UTF16BE.len())
        .any(|w| w == NAME_UTF16BE)
}

/// Convert Display P3 pixel values to sRGB in place: linearize (P3 shares sRGB's
/// transfer curve), 3×3 primary matrix, re-encode, clamp (relative-colorimetric
/// style per-channel gamut clip). Alpha is untouched (straight-alpha art).
///
/// Both transfer curves are tabulated per call. The old per-pixel loop spent six
/// `powf` on every pixel — 25 M transcendentals for a 2048² layer, and the
/// hottest loop in the compositor — while both ends of the conversion are 8-bit
/// here anyway. The re-encode table is exact rather than approximate, because
/// downstream keys (the chatterbox screen key) match on exact bytes: a cell
/// carries the byte at its left edge plus the true boundary where that byte
/// changes, and the curve is shallow enough (< 0.41 bytes per cell) for one
/// compare to settle it.
pub fn display_p3_to_srgb(mut img: RgbaImage) -> RgbaImage {
    // Linear Display-P3 → linear sRGB (both D65; CSS Color 4 / color.js values).
    const M: [[f32; 3]; 3] = [
        [1.224_940_2, -0.224_940_2, 0.0],
        [-0.042_056_955, 1.042_057, 0.0],
        [-0.019_637_555, -0.078_636_04, 1.098_273_6],
    ];
    let lut = TransferLut::new();
    for px in img.pixels_mut() {
        let [r, g, b, a] = px.0;
        let lin = [lut.linear(r), lut.linear(g), lut.linear(b)];
        let out = std::array::from_fn::<_, 3, _>(|i| {
            let v = M[i][0] * lin[0] + M[i][1] * lin[1] + M[i][2] * lin[2];
            lut.byte(v)
        });
        px.0 = [out[0], out[1], out[2], a];
    }
    img
}

/// sRGB EOTF: encoded value → linear light.
fn eotf(u: f32) -> f32 {
    if u <= 0.04045 {
        u / 12.92
    } else {
        ((u + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB OETF: linear light → encoded value.
fn oetf(l: f32) -> f32 {
    if l <= 0.003_130_8 {
        12.92 * l
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

/// Linear values per unit in the re-encode table. 8 K cells, and at the curve's
/// steepest point (the toe, 12.92 × 255 ≈ 3 295 bytes per unit) a cell is still
/// only 0.41 bytes wide — so a cell spans at most one byte boundary and
/// [`TransferLut::byte`]'s single compare settles it exactly.
const STEPS: usize = 8_192;

/// Both sRGB transfer curves, sampled once per call so the pixel loop does no
/// transcendental work.
struct TransferLut {
    /// Encoded → linear. Exact: the 8-bit domain is the whole table.
    linear: [f32; 256],
    /// Linear → encoded: the byte at each cell's left edge.
    byte_at: [u8; STEPS],
    /// Linear value where byte `k` begins — where the curve reaches
    /// `(k - 0.5) / 255`, which is `eotf((k - 0.5) / 255)` because `eotf` and
    /// `oetf` are inverses. `edge[0]` sits below 0 and `edge[256]` above 1.
    edge: [f32; 257],
}

impl TransferLut {
    fn new() -> Self {
        Self {
            linear: std::array::from_fn(|u| eotf(u as f32 / 255.0)),
            byte_at: std::array::from_fn(|i| (oetf(i as f32 / STEPS as f32) * 255.0 + 0.5) as u8),
            edge: std::array::from_fn(|k| eotf((k as f32 - 0.5) / 255.0)),
        }
    }

    fn linear(&self, u: u8) -> f32 {
        self.linear[u as usize]
    }

    /// The byte the old per-pixel formula emitted for this linear value — via the
    /// table instead of a `powf`.
    fn byte(&self, l: f32) -> u8 {
        let l = l.clamp(0.0, 1.0);
        let cell = ((l * STEPS as f32) as usize).min(STEPS - 1);
        let k = self.byte_at[cell];
        if self.edge[k as usize + 1] <= l {
            k + 1
        } else {
            k
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// Pinned against lcms2 (relative colorimetric) on the actual Procreate
    /// profile: greys are fixed points (why the region keys never broke), chroma
    /// comes back on saturated art, alpha passes through.
    #[test]
    fn display_p3_conversion_matches_cms_reference() {
        let mut img = RgbaImage::new(4, 1);
        img.put_pixel(0, 0, Rgba([110, 195, 220, 128])); // the b-pass sheen cyan
        img.put_pixel(1, 0, Rgba([127, 127, 127, 255])); // #7F7F7F grey key
        img.put_pixel(2, 0, Rgba([30, 30, 30, 255])); // linework
        img.put_pixel(3, 0, Rgba([190, 30, 45, 255])); // saturated red
        let out = display_p3_to_srgb(img);
        let expect: [([u8; 4], &str); 4] = [
            (
                [74, 198, 223, 128],
                "sheen cyan regains chroma, alpha untouched",
            ),
            ([127, 127, 127, 255], "greys are fixed points"),
            ([30, 30, 30, 255], "near-black linework unchanged"),
            (
                [208, 0, 36, 255],
                "saturated red (green clips to gamut edge)",
            ),
        ];
        for (x, (want, why)) in expect.iter().enumerate() {
            let got = out.get_pixel(x as u32, 0).0;
            for c in 0..4 {
                assert!(
                    (got[c] as i32 - want[c] as i32).abs() <= 2,
                    "{why}: px {x} channel {c}: got {got:?}, want {want:?}"
                );
            }
        }
    }

    /// The tabulated transfer curves are exact, not approximate — downstream keys
    /// (the chatterbox screen key) match on exact bytes. Sweep the table against
    /// the direct formula at every cell edge and inside the cells, and pin the
    /// invariant the one-compare lookup rests on.
    #[test]
    fn tabulated_transfer_curves_are_exact() {
        fn direct_byte(l: f32) -> u8 {
            (oetf(l.clamp(0.0, 1.0)) * 255.0 + 0.5) as u8
        }
        let lut = TransferLut::new();

        for u in 0..=u8::MAX {
            assert_eq!(lut.linear(u), eotf(u as f32 / 255.0), "linearise {u}");
        }

        // At most one byte boundary per cell — the property `byte` relies on.
        let mut boundaries = 0;
        for i in 0..STEPS {
            let (a, b) = (
                lut.byte(i as f32 / STEPS as f32),
                lut.byte((i + 1) as f32 / STEPS as f32),
            );
            assert!(b as i32 - a as i32 <= 1, "cell {i}: {a} → {b}");
            boundaries += (b - a) as usize;
        }
        assert_eq!(
            boundaries, 255,
            "byte 0 climbs to byte 255 across the table"
        );

        for i in 0..=STEPS {
            let l = i as f32 / STEPS as f32;
            assert_eq!(lut.byte(l), direct_byte(l), "cell edge {i} ({l})");
        }
        for i in 0..STEPS {
            for f in [0.25, 0.5, 0.75] {
                let l = (i as f32 + f) / STEPS as f32;
                assert_eq!(lut.byte(l), direct_byte(l), "cell {i} at {f}");
            }
        }
    }

    // NB the "chatterbox screen key survives P3 load" guard test lives in the
    // hodlcroft compositor repo (mugz-mesh) — it pins THIS conversion against
    // that project's real assets, which don't ship with this crate.

    #[test]
    fn p3_profile_detection_is_name_keyed() {
        // The mluc description is UTF-16BE — a profile carrying "Display P3"
        // matches; an sRGB-named profile does not.
        let mut p3 = vec![0u8; 32];
        p3.extend_from_slice(b"\x00D\x00i\x00s\x00p\x00l\x00a\x00y\x00 \x00P\x003");
        assert!(is_display_p3(&p3));
        let mut srgb = vec![0u8; 32];
        srgb.extend_from_slice(b"\x00s\x00R\x00G\x00B");
        assert!(!is_display_p3(&srgb));
    }
}
