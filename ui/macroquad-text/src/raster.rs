//! The raster arithmetic: which whole-pixel size macroquad rasterises a glyph
//! at, and the scale that brings it back to exactly the size asked for.

/// Pixels per point [`MacroquadMeasure`](crate::MacroquadMeasure) rasterises
/// at: fine enough that rounding the raster size moves a width by well under a
/// pixel.
pub(crate) const MEASURE_SCALE: f32 = 4.0;

/// The glyph size to rasterise `size` points at `k` pixels per point, and the
/// scale that brings it back to exactly `size`.
///
/// macroquad rasterises at whole pixel sizes. Scaling by `1 / k` would draw
/// at `round(size * k) / k` points, which is not the size the layout measured
/// at (fractional sizes, such as a title shrunk to fit, come out wider than
/// measured and swallow the spaces between words); scaling by
/// `size / round(size * k)` draws at exactly `size`, whatever `k` is.
pub(crate) fn raster(size: f32, k: f32) -> (u16, f32) {
    let px = (size * k).round().clamp(1.0, u16::MAX as f32);
    (px as u16, size.max(0.0) / px)
}

/// The font size and scale to hand macroquad so `size` points rasterise at
/// `pixel_scale` device pixels per point, on a window whose DPI scale is `dpi`.
///
/// macroquad multiplies the font size it is given by the window's DPI scale
/// before it rasterises (`ceil(font_size * dpi)`). `pixel_scale` already
/// includes that scale, so it comes out here: passing `size * pixel_scale`
/// straight through rasterised every glyph at `dpi` times the size it lands
/// at, four times the atlas area on a 2x display, and downsampled it.
pub(crate) fn draw_raster(size: f32, pixel_scale: f32, dpi: f32) -> (u16, f32) {
    raster(size, pixel_scale.max(0.01) / dpi.max(0.01))
}

/// The size macroquad caches a glyph under for a font size it was given: the
/// key its atlas is filled by.
pub(crate) fn atlas_size(font_size: u16, dpi: f32) -> u16 {
    (font_size as f32 * dpi).ceil().clamp(1.0, u16::MAX as f32) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_raster_draws_at_exactly_the_size_asked_at_any_pixel_scale() {
        for size in [9.0, 10.25, 11.6, 12.9, 31.5] {
            for k in [1.0, 1.5, 2.0, 2.625, 3.0, MEASURE_SCALE] {
                let (px, scale) = raster(size, k);
                assert_eq!(px, (size * k).round() as u16);
                let drawn = px as f32 * scale;
                assert!((drawn - size).abs() < 1e-4, "{size} at {k}x drew {drawn}");
            }
        }
        assert_eq!(raster(0.0, 2.0).0, 1, "never a zero-pixel raster");
    }

    #[test]
    fn a_draw_rasterises_at_the_device_pixels_it_lands_on_not_dpi_times_more() {
        // 10 points at 2.31 device px per point on a 2x display: about 23
        // device pixels, not the 46 the DPI-blind raster asked for.
        let (font_size, scale) = draw_raster(10.0, 2.31, 2.0);
        let atlas = atlas_size(font_size, 2.0);
        assert_eq!(font_size, 12);
        assert_eq!(atlas, 24);
        // What macroquad draws: the atlas raster back in points, scaled.
        let drawn = atlas as f32 / 2.0 * scale;
        assert!((drawn - 10.0).abs() < 1e-4, "drew {drawn}");
        // On a 1x display nothing changes.
        assert_eq!(draw_raster(10.0, 1.5, 1.0), raster(10.0, 1.5));
    }
}
