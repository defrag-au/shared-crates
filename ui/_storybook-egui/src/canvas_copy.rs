//! Copy the storybook's own canvas to the clipboard.
//!
//! ## Why `ViewportCommand::Screenshot` and not the canvas element
//!
//! Reading the `<canvas>` directly (`toDataURL`, `toBlob`) is the obvious move
//! and it does not work: egui renders through WebGL, a WebGL drawing buffer is
//! not preserved after the frame it was drawn in, and a read-back of one that
//! was not created with `preserveDrawingBuffer` returns a blank image. That flag
//! belongs to whoever created the context, which is eframe, not us.
//!
//! So the pixels come from eframe: `ViewportCommand::Screenshot` makes the
//! painter read its own framebuffer back and hand the frame over as an
//! [`egui::Event::Screenshot`]. Only the last step — putting a PNG where the
//! reader wants it — needs the browser.
//!
//! ## Why the PNG is encoded here
//!
//! `image` is already a dependency (the `html_stage` story parses art with it),
//! and an encoder a test can call is worth more than one the browser happens to
//! provide: the failure mode is a blank or truncated file, which is invisible
//! until someone pastes it.

use egui::ColorImage;

/// Encode a captured frame as a PNG.
///
/// `None` only when the image's dimensions do not match its own pixel buffer —
/// which would be eframe's bug rather than ours, and is worth saying out loud
/// rather than writing a truncated file over.
pub fn encode_png(image: &ColorImage) -> Option<Vec<u8>> {
    let [width, height] = image.size;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for pixel in &image.pixels {
        // `to_srgba_unmultiplied`, not the raw channels: egui stores
        // PREMULTIPLIED alpha, and a PNG carrying premultiplied values renders
        // lighter than what was on screen.
        rgba.extend_from_slice(&pixel.to_srgba_unmultiplied());
    }

    let buffer = image::RgbaImage::from_raw(width as u32, height as u32, rgba)?;
    let mut png = Vec::new();
    buffer
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(png)
}

/// Hand a PNG to the browser: the clipboard first, a tab to save it from if the
/// clipboard refuses (permission, or an unfocused document). Returns the line
/// the chrome shows.
#[cfg(target_arch = "wasm32")]
pub async fn offer_png(png: Vec<u8>, name: String) -> String {
    match browser::offer_png_js(png, &name).await {
        Ok(value) => value
            .as_string()
            .unwrap_or_else(|| "copied to the clipboard".to_string()),
        Err(error) => format!("could not offer the png: {error:?}"),
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(inline_js = r#"
// Async, because `clipboard.write` is a promise and `ClipboardItem` needs a blob
// rather than a data URL. The fallback is a tab rather than a download: a
// download goes to a directory this code cannot see (and is not ours to write
// in), while a tab shows the reader the image they just asked for.
export async function offerPng(bytes, name) {
    const blob = new Blob([bytes], { type: 'image/png' });
    try {
        await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
        return 'copied to the clipboard';
    } catch (clipboardRefused) {
        const url = URL.createObjectURL(blob);
        const opened = window.open(url, '_blank');
        if (!opened) return 'clipboard refused, and the pop-up was blocked — ' + name;
        return 'clipboard refused — opened ' + name + ' in a tab to save';
    }
}
"#)]
    extern "C" {
        #[wasm_bindgen(js_name = offerPng, catch)]
        pub async fn offer_png_js(bytes: Vec<u8>, name: &str) -> Result<JsValue, JsValue>;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The magic bytes AND the dimensions, read back out of the IHDR chunk. The
    /// failure this catches — a blank or truncated PNG — looks like success
    /// until someone pastes it somewhere.
    #[test]
    fn encode_png_writes_a_png_of_the_captured_size() {
        let image = ColorImage::filled([3, 2], egui::Color32::from_rgb(1, 2, 3));
        let png = encode_png(&image).expect("encodes");

        assert_eq!(
            &png[..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
            "not a PNG"
        );
        // IHDR is the first chunk: 8 magic bytes, then length(4) + type(4),
        // then width and height as big-endian u32.
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 3);
        assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 2);
    }

    /// A dimension mismatch is reported rather than half-encoded: a caller that
    /// cannot tell the difference would offer a truncated file.
    #[test]
    fn a_pixel_count_that_does_not_match_the_size_is_refused() {
        let mut image = ColorImage::filled([2, 2], egui::Color32::BLACK);
        image.pixels.pop();
        assert!(encode_png(&image).is_none());
    }
}
