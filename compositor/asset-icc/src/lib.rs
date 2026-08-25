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
    profile.windows(NAME_UTF16BE.len()).any(|w| w == NAME_UTF16BE)
}

/// Convert Display P3 pixel values to sRGB in place: linearize (P3 shares sRGB's
/// transfer curve), 3×3 primary matrix, re-encode, clamp (relative-colorimetric
/// style per-channel gamut clip). Alpha is untouched (straight-alpha art).
pub fn display_p3_to_srgb(mut img: RgbaImage) -> RgbaImage {
    // Linear Display-P3 → linear sRGB (both D65; CSS Color 4 / color.js values).
    const M: [[f32; 3]; 3] = [
        [1.224_940_2, -0.224_940_2, 0.0],
        [-0.042_056_955, 1.042_057, 0.0],
        [-0.019_637_555, -0.078_636_04, 1.098_273_6],
    ];
    fn eotf(u: f32) -> f32 {
        if u <= 0.04045 { u / 12.92 } else { ((u + 0.055) / 1.055).powf(2.4) }
    }
    fn oetf(l: f32) -> f32 {
        if l <= 0.003_130_8 { 12.92 * l } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 }
    }
    for px in img.pixels_mut() {
        let [r, g, b, a] = px.0;
        let lin = [eotf(r as f32 / 255.0), eotf(g as f32 / 255.0), eotf(b as f32 / 255.0)];
        let out = std::array::from_fn::<_, 3, _>(|i| {
            let v = M[i][0] * lin[0] + M[i][1] * lin[1] + M[i][2] * lin[2];
            (oetf(v.clamp(0.0, 1.0)) * 255.0 + 0.5) as u8
        });
        px.0 = [out[0], out[1], out[2], a];
    }
    img
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
            ([74, 198, 223, 128], "sheen cyan regains chroma, alpha untouched"),
            ([127, 127, 127, 255], "greys are fixed points"),
            ([30, 30, 30, 255], "near-black linework unchanged"),
            ([208, 0, 36, 255], "saturated red (green clips to gamut edge)"),
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
