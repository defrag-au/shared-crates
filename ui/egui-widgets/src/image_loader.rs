use egui::{Color32, Pos2, Rect, Shape, Stroke};

/// Build a IIIF thumbnail URL for an asset image.
///
/// Constructs a IIIF Image API v3 URL that requests a square crop at the
/// given pixel size in JPEG format.
pub fn iiif_thumbnail_url(base_url: &str, size: u32) -> String {
    // IIIF pattern: {base}/full/!{w},{h}/0/default.jpg
    // Strip any trailing slash from base
    let base = base_url.trim_end_matches('/');
    format!("{base}/full/!{size},{size}/0/default.jpg")
}

/// A pre-computed spinner shape that can be stamped at multiple positions cheaply.
///
/// Compute once per frame via [`CachedSpinner::new`], then call [`CachedSpinner::paint`]
/// at each pending-image location. Avoids per-instance trig and repeated `request_repaint`.
pub struct CachedSpinner {
    /// Points relative to (0, 0) center.
    points: Vec<Pos2>,
    stroke: Stroke,
}

impl CachedSpinner {
    /// Compute the spinner arc points for this frame. Call once per frame.
    pub fn new(ui: &egui::Ui, radius: f32, color: Color32) -> Self {
        let n_points = (radius.round() as u32).clamp(8, 128);
        let time = ui.input(|i| i.time);
        let start_angle = time * std::f64::consts::TAU;
        let end_angle = start_angle + 240f64.to_radians() * time.sin();
        let points: Vec<Pos2> = (0..n_points)
            .map(|i| {
                let angle = egui::emath::lerp(start_angle..=end_angle, i as f64 / n_points as f64);
                let (sin, cos) = angle.sin_cos();
                Pos2::new(radius * cos as f32, radius * sin as f32)
            })
            .collect();
        Self {
            points,
            stroke: Stroke::new(3.0, color),
        }
    }

    /// Paint the pre-computed spinner centered in `rect`.
    pub fn paint(&self, ui: &egui::Ui, rect: Rect) {
        let center = rect.center().to_vec2();
        let translated: Vec<Pos2> = self.points.iter().map(|p| *p + center).collect();
        ui.painter().add(Shape::line(translated, self.stroke));
    }

    /// Call once per frame to keep the animation running. Only needed if at least
    /// one spinner was actually painted.
    pub fn request_repaint(ui: &egui::Ui) {
        ui.ctx().request_repaint();
    }
}

/// Browser-native image loader for WASM targets.
///
/// Replaces egui_extras' `ImageCrateLoader` which decodes images synchronously
/// on the main thread using the `image` crate (zune-jpeg). That approach blocks
/// the UI, especially for JPEG thumbnails.
///
/// This loader uses the browser's `createImageBitmap()` API which decodes images
/// off the main thread using native platform codecs, then reads pixels back via
/// `OffscreenCanvas` + `getImageData()`.
#[cfg(target_arch = "wasm32")]
pub mod browser {
    use egui::load::{BytesPoll, ImageLoadResult, ImagePoll, LoadError, SizeHint};
    use egui::ColorImage;
    use std::sync::{Arc, Mutex};
    use std::task::Poll;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    type Entry = Poll<Result<Arc<ColorImage>, String>>;

    pub struct BrowserImageLoader {
        cache: Arc<Mutex<std::collections::HashMap<String, Entry>>>,
    }

    impl Default for BrowserImageLoader {
        fn default() -> Self {
            Self {
                cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
            }
        }
    }

    impl BrowserImageLoader {
        pub const ID: &'static str = egui::generate_loader_id!(BrowserImageLoader);
    }

    impl egui::load::ImageLoader for BrowserImageLoader {
        fn id(&self) -> &str {
            Self::ID
        }

        fn load(&self, ctx: &egui::Context, uri: &str, _size_hint: SizeHint) -> ImageLoadResult {
            // Check cache first
            if let Some(entry) = self.cache.lock().unwrap().get(uri).cloned() {
                return match entry {
                    Poll::Ready(Ok(image)) => Ok(ImagePoll::Ready { image }),
                    Poll::Ready(Err(err)) => Err(LoadError::Loading(err)),
                    Poll::Pending => Ok(ImagePoll::Pending { size: None }),
                };
            }

            // Try to get bytes from the existing BytesLoader (EhttpLoader)
            match ctx.try_load_bytes(uri) {
                Ok(BytesPoll::Ready { bytes, .. }) => {
                    // Mark as pending in cache before spawning async decode
                    self.cache
                        .lock()
                        .unwrap()
                        .insert(uri.to_owned(), Poll::Pending);

                    let cache = self.cache.clone();
                    let uri_owned = uri.to_owned();
                    let ctx = ctx.clone();

                    // Spawn async browser decode — runs off main thread
                    wasm_bindgen_futures::spawn_local(async move {
                        let result = browser_decode_image(&bytes).await;
                        let entry = match &result {
                            Ok(image) => Poll::Ready(Ok(Arc::new(image.clone()))),
                            Err(err) => Poll::Ready(Err(err.clone())),
                        };
                        cache.lock().unwrap().insert(uri_owned, entry);
                        ctx.request_repaint();
                    });

                    Ok(ImagePoll::Pending { size: None })
                }
                Ok(BytesPoll::Pending { size }) => Ok(ImagePoll::Pending { size }),
                Err(err) => Err(err),
            }
        }

        fn forget(&self, uri: &str) {
            self.cache.lock().unwrap().remove(uri);
        }

        fn forget_all(&self) {
            self.cache.lock().unwrap().clear();
        }

        fn byte_size(&self) -> usize {
            self.cache
                .lock()
                .unwrap()
                .values()
                .filter_map(|entry| {
                    if let Poll::Ready(Ok(image)) = entry {
                        Some(image.pixels.len() * 4) // RGBA
                    } else {
                        None
                    }
                })
                .sum()
        }
    }

    /// Decode image bytes using the browser's native `createImageBitmap` API.
    ///
    /// This runs the actual decode off the main thread (browser handles scheduling),
    /// then reads the pixels back via `OffscreenCanvas` + `getImageData()`.
    async fn browser_decode_image(bytes: &[u8]) -> Result<ColorImage, String> {
        // Create a Blob from the raw bytes
        let uint8_array = js_sys::Uint8Array::from(bytes);
        let blob_parts = js_sys::Array::new();
        blob_parts.push(&uint8_array);

        let blob_opts = web_sys::BlobPropertyBag::new();
        // Let the browser detect the format — createImageBitmap handles it
        blob_opts.set_type("image/jpeg");

        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&blob_parts, &blob_opts)
            .map_err(|e| format!("Failed to create Blob: {e:?}"))?;

        // createImageBitmap(blob) — browser decodes off main thread
        let global = js_sys::global();
        let promise = if let Some(window) = global.dyn_ref::<web_sys::Window>() {
            window
                .create_image_bitmap_with_blob(&blob)
                .map_err(|e| format!("createImageBitmap failed: {e:?}"))?
        } else if let Some(worker) = global.dyn_ref::<web_sys::WorkerGlobalScope>() {
            worker
                .create_image_bitmap_with_blob(&blob)
                .map_err(|e| format!("createImageBitmap failed: {e:?}"))?
        } else {
            return Err("No global scope available for createImageBitmap".into());
        };

        let bitmap_js = JsFuture::from(promise)
            .await
            .map_err(|e| format!("createImageBitmap rejected: {e:?}"))?;

        let bitmap: web_sys::ImageBitmap = bitmap_js
            .dyn_into()
            .map_err(|_| "Result is not an ImageBitmap".to_string())?;

        let width = bitmap.width();
        let height = bitmap.height();

        if width == 0 || height == 0 {
            return Err("Image has zero dimensions".into());
        }

        // Draw bitmap onto an OffscreenCanvas to extract pixel data
        let canvas = web_sys::OffscreenCanvas::new(width, height)
            .map_err(|e| format!("Failed to create OffscreenCanvas: {e:?}"))?;

        let ctx_obj = canvas
            .get_context("2d")
            .map_err(|e| format!("Failed to get 2d context: {e:?}"))?
            .ok_or("get_context returned None")?;

        let ctx_2d: web_sys::OffscreenCanvasRenderingContext2d = ctx_obj
            .dyn_into()
            .map_err(|_| "Context is not OffscreenCanvasRenderingContext2d".to_string())?;

        // Draw the decoded bitmap onto the canvas
        ctx_2d
            .draw_image_with_image_bitmap(&bitmap, 0.0, 0.0)
            .map_err(|e| format!("drawImage failed: {e:?}"))?;

        // Read back the RGBA pixels
        let image_data = ctx_2d
            .get_image_data(0.0, 0.0, width as f64, height as f64)
            .map_err(|e| format!("getImageData failed: {e:?}"))?;

        let rgba = image_data.data().0;

        Ok(ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            &rgba,
        ))
    }
}
