use egui::{Color32, Pos2, Rect, Shape, Stroke};

#[cfg(target_arch = "wasm32")]
pub mod fetch;
pub mod schedule;

// ============================================================================
// IIIF image helpers
// ============================================================================

/// Standard image sizes served by the IIIF worker.
///
/// Using a fixed set of sizes maximises CDN cache hit rates.
///
/// **The widths now come from [`image_core::ImageSize`]**, which is the one
/// definition in the estate. Six independent copies of this enum existed and
/// two had drifted — to 1686 and 1626 — which is invisible in behaviour: the
/// image still arrives, just uncached and slow, forever. This type stays for
/// the dozen frontends that name it, but it no longer *decides* anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssetImageSize {
    /// Fast, pre-cached thumbnails for grids and cards.
    #[default]
    Thumbnail,
    /// High-resolution for detail views and full-screen.
    Large,
}

impl From<AssetImageSize> for image_core::ImageSize {
    fn from(size: AssetImageSize) -> Self {
        match size {
            AssetImageSize::Thumbnail => Self::Thumb,
            AssetImageSize::Large => Self::Full,
        }
    }
}

impl AssetImageSize {
    /// Pixel width sent to the IIIF `{size}` parameter.
    pub fn pixels(self) -> u32 {
        image_core::ImageSize::from(self).px()
    }
}

/// Build a full IIIF asset image URL.
///
/// ```text
/// https://iiif.hodlcroft.com/iiif/3/{policy_id}:{asset_name_hex}/full/{size},/0/default.auto
/// ```
///
/// `.auto` lets the service pick the container from what it knows the art to
/// be — pixel art comes back lossless rather than as a blurred JPEG. See
/// [`image_core::Format`].
pub fn iiif_asset_url(policy_id: &str, asset_name_hex: &str, size: AssetImageSize) -> String {
    image_core::iiif_asset_url(policy_id, asset_name_hex, size.into())
}

/// As [`iiif_asset_url`], against a specific IIIF deployment — the preprod
/// host for an app running against a testnet. See [`iiif_base_for_network`].
pub fn iiif_asset_url_on(
    base: &str,
    policy_id: &str,
    asset_name_hex: &str,
    size: AssetImageSize,
) -> String {
    image_core::iiif_url_on(base, policy_id, asset_name_hex, size.into())
}

/// The IIIF deployment for a chain network string (`cardano:mainnet`,
/// `cardano:preprod`). A preprod policy asked of the mainnet host resolves
/// to nothing, so every network-aware app picks its base with this.
pub use image_core::base_for_network as iiif_base_for_network;
/// The named IIIF deployments.
pub use image_core::hosts as iiif_hosts;

/// Build a IIIF thumbnail URL for an asset image.
///
/// Constructs a IIIF Image API v3 URL that requests a square crop at the
/// given pixel size in JPEG format.
///
/// **Not routed through `image_core::IiifUrl`**, unlike everything else here:
/// `base_url` is already a *per-asset* base (host + `{policy}:{name_hex}`),
/// whereas the builder composes that identifier itself from a host root.
///
/// Because it hand-rolls the tail, it is also the last place in the estate
/// that could pin a format behind the builder's back — so it now emits
/// whatever [`image_core::Format::default`] is, and is on its way out.
#[deprecated(
    since = "0.1.0",
    note = "hand-rolls the IIIF tail. Use `image_core::IiifUrl::new(policy, hex).square_fit(size)`, \
            which takes the policy and hex separately and produces the same shape."
)]
pub fn iiif_thumbnail_url(base_url: &str, size: u32) -> String {
    // IIIF pattern: {base}/full/!{w},{h}/0/default.{fmt}
    // Strip any trailing slash from base
    let base = base_url.trim_end_matches('/');
    format!(
        "{base}/full/!{size},{size}/0/default.{}",
        image_core::Format::default().extension()
    )
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
            stroke: Stroke::new(3.0_f32, color),
        }
    }

    /// Draw the "this will never arrive" mark instead of the spinner.
    ///
    /// A load that FAILED and one still in flight look identical through
    /// `try_load_texture` — `Err(..)` and `Ok(Pending)` are both "not ready" —
    /// and treating them the same is how a single dead thumbnail pins the
    /// whole app at 60fps: a spinner asks for the next frame, forever, for
    /// bytes that are never coming. The loader caches failures deliberately,
    /// so nothing ever retries and nothing ever settles.
    pub fn paint_unavailable(ui: &egui::Ui, rect: Rect, colour: egui::Color32) {
        use crate::theme::{TextSize, ThemeExt as _};
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "?",
            egui::FontId::proportional(ui.text_size(TextSize::Xl2)),
            colour,
        );
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

/// What a decode was asked to produce, snapped to a ladder.
///
/// # Why a decoded image is keyed by size at all
///
/// A texture costs width × height × 4 bytes and nothing else — the format it
/// arrived in is irrelevant once decoded. So a 2048px artwork drawn into an
/// 84px grid card costs 16 MB to show 28 KB of pixels, and a page of them is
/// gigabytes. That was never a caching problem; the images were being decoded
/// at a size nobody asked for.
///
/// # Why a ladder rather than the exact size
///
/// Rounding UP to a rung is what lets an 84px card and a 100px card share one
/// decode. Keying on exact requested sizes would fragment the cache and decode
/// the same art repeatedly at near-identical sizes — and every one of those
/// costs a full-size transient in wasm linear memory, which never shrinks.
///
/// The rungs mirror the IIIF service's own derivative widths
/// (`workers/iiif/src/image_size.rs`), so a request usually lands on something
/// already warm in R2 rather than triggering a cold render.
///
/// Lives OUTSIDE the `browser` module below, which is `wasm32`-only, because
/// it is pure arithmetic and the arithmetic is what goes wrong. Gated with the
/// decoder it would compile only for a target the test runner never builds —
/// green on every native run while saying nothing. Same split as
/// `image_loader::schedule` against `image_loader::fetch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DecodeSize {
    /// 128px. Grids and lists on a non-HiDPI display.
    Tile,
    /// 256px. The SAME grids at 2× device pixel ratio, which is most screens
    /// this runs on.
    ///
    /// Added from measurement, not taste. Without it an 84px card needing 168
    /// physical pixels fell through to [`Self::Card`], and the vitals line
    /// showed a texture average of ~660 KB where 64 KB was expected — every
    /// thumbnail paying 400px for 168px of screen, a 5.7× area overshoot.
    /// A ladder with a gap where the common case sits is not a ladder.
    Retina,
    /// 400px. Cards, browsers.
    Card,
    /// 1686px. Detail views.
    Full,
    /// Whatever the source is. For art that must not be resampled at all.
    Native,
}

impl DecodeSize {
    /// The smallest rung that covers `hint`, in physical pixels.
    ///
    /// `Scale` cannot be answered without knowing the source dimensions, which
    /// nothing has until the image is decoded — so it means [`Self::Native`],
    /// matching egui's own reading of it as "do not resize".
    pub fn for_hint(hint: egui::load::SizeHint) -> Self {
        use egui::load::SizeHint;
        let longest = match hint {
            SizeHint::Scale(_) => return Self::Native,
            SizeHint::Width(w) => w,
            SizeHint::Height(h) => h,
            SizeHint::Size { width, height, .. } => width.max(height),
        };
        match longest {
            0..=128 => Self::Tile,
            129..=256 => Self::Retina,
            257..=400 => Self::Card,
            401..=1686 => Self::Full,
            _ => Self::Native,
        }
    }

    /// Pixels to decode to, or `None` to leave the source alone.
    ///
    /// These need NOT match the IIIF service's derivative widths, and
    /// [`Self::Retina`] deliberately does not. The two ladders answer
    /// different questions: the service's rungs decide which object is warm in
    /// R2, which is bandwidth and latency; these decide how big a texture
    /// ends up, which is memory. A 400px download decoded to 256px costs the
    /// bandwidth of the former and the memory of the latter, and memory is the
    /// one measured in gigabytes.
    pub fn px(self) -> Option<u32> {
        match self {
            Self::Tile => Some(128),
            Self::Retina => Some(256),
            Self::Card => Some(400),
            Self::Full => Some(1686),
            Self::Native => None,
        }
    }
}

/// Browser-native image loader for WASM targets.
///
/// Replaces egui_extras' `ImageCrateLoader`, which decodes images
/// synchronously on the main thread using the `image` crate (zune-jpeg). That
/// approach blocks the UI, especially for JPEG thumbnails.
///
/// This loader uses the browser's `createImageBitmap()` API, which decodes off
/// the main thread using native platform codecs — and, given a [`DecodeSize`],
/// resizes WHILE decoding so the full-resolution bitmap is never materialised
/// at all. Pixels are then read back via `OffscreenCanvas` + `getImageData()`.
#[cfg(target_arch = "wasm32")]
pub mod browser {
    use egui::ColorImage;
    use egui::load::{BytesPoll, ImageLoadResult, ImagePoll, LoadError, SizeHint};
    use std::sync::{Arc, Mutex};
    use std::task::Poll;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    type Entry = Poll<Result<Arc<ColorImage>, String>>;

    use super::DecodeSize;

    /// One decoded image: a URI AND the size it was decoded at.
    ///
    /// Both, because the same art is legitimately held at more than one size —
    /// a grid card and the detail view behind it. Keying on the URI alone
    /// means whichever loaded first wins, so either the grid gets a 1686px
    /// texture or the detail view gets a 128px one blown up.
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    struct Decoded {
        uri: String,
        size: DecodeSize,
    }

    pub struct BrowserImageLoader {
        cache: Arc<Mutex<std::collections::HashMap<Decoded, Entry>>>,
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

        fn load(&self, ctx: &egui::Context, uri: &str, size_hint: SizeHint) -> ImageLoadResult {
            // The hint is the whole reason this loader is cheap. Ignoring it —
            // which this did — decodes every image at its native resolution
            // regardless of how small it is drawn, so a grid of 84px cards
            // held full-size artwork and the tab ran to gigabytes.
            let want = DecodeSize::for_hint(size_hint);
            let key = Decoded {
                uri: uri.to_owned(),
                size: want,
            };

            // Check cache first
            if let Some(entry) = self.cache.lock().unwrap().get(&key).cloned() {
                return match entry {
                    Poll::Ready(Ok(image)) => Ok(ImagePoll::Ready { image }),
                    Poll::Ready(Err(ref err)) => Err(LoadError::Loading(err.clone())),
                    Poll::Pending => Ok(ImagePoll::Pending { size: None }),
                };
            }

            // Data URLs: handle directly without going through BytesLoader.
            // This handles both base64 PNG/JPEG and URL-encoded SVGs.
            if uri.starts_with("data:") {
                self.cache
                    .lock()
                    .unwrap()
                    .insert(key.clone(), Poll::Pending);

                let cache = self.cache.clone();
                let uri_owned = uri.to_owned();
                let ctx = ctx.clone();

                wasm_bindgen_futures::spawn_local(async move {
                    let result = browser_decode_data_url(&uri_owned, want).await;
                    if let Err(ref err) = result {
                        log::warn!(
                            "[image_loader] data URL decode failed for {}: {err}",
                            &uri_owned[..uri_owned.len().min(60)]
                        );
                    }
                    let entry = match result {
                        Ok(image) => Poll::Ready(Ok(Arc::new(image))),
                        Err(err) => Poll::Ready(Err(err)),
                    };
                    cache.lock().unwrap().insert(key, entry);
                    ctx.request_repaint();
                });

                return Ok(ImagePoll::Pending { size: None });
            }

            // HTTP URLs: get bytes from the existing BytesLoader (EhttpLoader)
            match ctx.try_load_bytes(uri) {
                Ok(BytesPoll::Ready { bytes, .. }) => {
                    // Mark as pending in cache before spawning async decode
                    self.cache
                        .lock()
                        .unwrap()
                        .insert(key.clone(), Poll::Pending);

                    let cache = self.cache.clone();
                    let uri_owned = uri.to_owned();
                    let ctx = ctx.clone();

                    // Spawn async browser decode — runs off main thread
                    wasm_bindgen_futures::spawn_local(async move {
                        let result = browser_decode_image(&bytes, want).await;
                        if let Err(ref err) = result {
                            log::warn!(
                                "[image_loader] decode failed for {}: {err}",
                                &uri_owned[..uri_owned.len().min(60)]
                            );
                        }
                        let entry = match result {
                            Ok(image) => Poll::Ready(Ok(Arc::new(image))),
                            Err(err) => Poll::Ready(Err(err)),
                        };
                        cache.lock().unwrap().insert(key, entry);
                        ctx.request_repaint();
                    });

                    Ok(ImagePoll::Pending { size: None })
                }
                Ok(BytesPoll::Pending { size }) => Ok(ImagePoll::Pending { size }),
                Err(err) => Err(err),
            }
        }

        /// Every size of `uri`, not just one.
        ///
        /// The caller named an image, not a decode. A `forget` that dropped
        /// only the rung it happened to think of would leave the others live
        /// and a "forgotten" image still on screen.
        fn forget(&self, uri: &str) {
            self.cache.lock().unwrap().retain(|key, _| key.uri != uri);
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

    /// Decode a data URL (base64 PNG/JPEG or URL-encoded SVG) using the browser's
    /// native `Image()` element + canvas.
    ///
    /// This handles SVGs correctly (unlike `createImageBitmap` which rejects them
    /// in many browsers). Works for all image formats the browser supports.
    /// Data URLs go through `HtmlImageElement` rather than `createImageBitmap`
    /// — no blob to hand over — so `want` only bounds the canvas they are
    /// drawn into. These are icons and inline SVGs, small by nature, which is
    /// why the cheaper path is left alone.
    async fn browser_decode_data_url(
        data_url: &str,
        want: DecodeSize,
    ) -> Result<ColorImage, String> {
        use wasm_bindgen::closure::Closure;

        // Create an HtmlImageElement and set src to the data URL
        let img = web_sys::HtmlImageElement::new()
            .map_err(|e| format!("Failed to create Image element: {e:?}"))?;

        // Wait for the image to load via a Promise
        let load_promise = js_sys::Promise::new(&mut |resolve, reject| {
            let on_load = Closure::<dyn FnMut()>::new({
                let resolve = resolve.clone();
                move || {
                    let _ = resolve.call0(&wasm_bindgen::JsValue::NULL);
                }
            });
            let on_error = Closure::<dyn FnMut()>::new({
                let reject = reject.clone();
                move || {
                    let _ = reject.call1(
                        &wasm_bindgen::JsValue::NULL,
                        &wasm_bindgen::JsValue::from_str("Image load failed"),
                    );
                }
            });
            img.set_onload(Some(on_load.as_ref().unchecked_ref()));
            img.set_onerror(Some(on_error.as_ref().unchecked_ref()));
            // Prevent closures from being dropped while the image loads
            on_load.forget();
            on_error.forget();
        });

        img.set_src(data_url);

        JsFuture::from(load_promise)
            .await
            .map_err(|e| format!("Image load rejected: {e:?}"))?;

        let natural_width = img.natural_width();
        let natural_height = img.natural_height();

        if natural_width == 0 || natural_height == 0 {
            return Err("Image has zero dimensions".into());
        }

        // Clamp to the rung, keeping the aspect ratio. Only ever DOWN: asking
        // the canvas to upscale would cost memory to invent detail that is not
        // in the source.
        let (width, height) = match want.px() {
            Some(px) if natural_width.max(natural_height) > px => {
                let scale = px as f64 / natural_width.max(natural_height) as f64;
                (
                    ((natural_width as f64 * scale).round() as u32).max(1),
                    ((natural_height as f64 * scale).round() as u32).max(1),
                )
            }
            _ => (natural_width, natural_height),
        };

        // Render to OffscreenCanvas to extract RGBA pixels
        let canvas = web_sys::OffscreenCanvas::new(width, height)
            .map_err(|e| format!("Failed to create OffscreenCanvas: {e:?}"))?;

        let ctx_obj = canvas
            .get_context("2d")
            .map_err(|e| format!("Failed to get 2d context: {e:?}"))?
            .ok_or("get_context returned None")?;

        let ctx_2d: web_sys::OffscreenCanvasRenderingContext2d = ctx_obj
            .dyn_into()
            .map_err(|_| "Context is not OffscreenCanvasRenderingContext2d".to_string())?;

        ctx_2d
            .draw_image_with_html_image_element_and_dw_and_dh(
                &img,
                0.0,
                0.0,
                width as f64,
                height as f64,
            )
            .map_err(|e| format!("drawImage failed: {e:?}"))?;

        let image_data = ctx_2d
            .get_image_data(0.0, 0.0, width as f64, height as f64)
            .map_err(|e| format!("getImageData failed: {e:?}"))?;

        let rgba = image_data.data().0;

        Ok(ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            &rgba,
        ))
    }

    /// Decode image bytes using the browser's native `createImageBitmap` API.
    ///
    /// This runs the actual decode off the main thread (browser handles scheduling),
    /// then reads the pixels back via `OffscreenCanvas` + `getImageData()`.
    async fn browser_decode_image(bytes: &[u8], want: DecodeSize) -> Result<ColorImage, String> {
        // ⚠️ NO function-wide scope here, and there cannot be one.
        //
        // A `profiling::scope!` is an RAII guard, and this function awaits the
        // browser's decode in the middle. puffin records one stream for the
        // single wasm thread, so a guard held across `.await` stays open while
        // OTHER decode tasks start their own — and the chart then shows
        // `image_loader::decode` nested inside itself, once per task in
        // flight, each "lasting" as long as the browser took to answer.
        //
        // A real capture read: 24 levels of self-nesting, 211ms of self time
        // in a 76ms frame (277% — the tell that it cannot be wall time), and
        // the egui pass itself recorded INSIDE the 14th decode, because a
        // frame that began while a guard was open is charged to that guard.
        //
        // So the scopes below cover the SYNCHRONOUS segments only. They are
        // the ones that can actually stall a frame; the await is the browser
        // decoding off the main thread, which costs us nothing to wait for.

        // Create a Blob from the raw bytes
        let uint8_array = js_sys::Uint8Array::from(bytes);
        let blob_parts = js_sys::Array::new();
        blob_parts.push(&uint8_array);

        let blob_opts = web_sys::BlobPropertyBag::new();
        // Let the browser detect the format — createImageBitmap handles it
        blob_opts.set_type("image/jpeg");

        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&blob_parts, &blob_opts)
            .map_err(|e| format!("Failed to create Blob: {e:?}"))?;

        // createImageBitmap(blob) — browser decodes off main thread, and
        // RESIZES WHILE DECODING when asked to. That is the difference
        // between a 128px texture and a 2048px one for the same grid card,
        // and the browser never materialises the full bitmap at all: the
        // saving is on every stage below, not just the texture that survives.
        let global = js_sys::global();
        let promise = match want.px() {
            Some(px) => {
                let options = web_sys::ImageBitmapOptions::new();
                options.set_resize_width(px);
                options.set_resize_height(px);
                // Aspect ratio is kept by the caller drawing it to fit; what
                // matters here is the CEILING on decoded pixels. `Pixelated`
                // would wreck photographic art, and the pixel-art path does
                // its own scaling upstream in the IIIF service.
                options.set_resize_quality(web_sys::ResizeQuality::High);
                if let Some(window) = global.dyn_ref::<web_sys::Window>() {
                    window
                        .create_image_bitmap_with_blob_and_image_bitmap_options(&blob, &options)
                        .map_err(|e| format!("createImageBitmap failed: {e:?}"))?
                } else if let Some(worker) = global.dyn_ref::<web_sys::WorkerGlobalScope>() {
                    worker
                        .create_image_bitmap_with_blob_and_image_bitmap_options(&blob, &options)
                        .map_err(|e| format!("createImageBitmap failed: {e:?}"))?
                } else {
                    return Err("No global scope available for createImageBitmap".into());
                }
            }
            None => {
                if let Some(window) = global.dyn_ref::<web_sys::Window>() {
                    window
                        .create_image_bitmap_with_blob(&blob)
                        .map_err(|e| format!("createImageBitmap failed: {e:?}"))?
                } else if let Some(worker) = global.dyn_ref::<web_sys::WorkerGlobalScope>() {
                    worker
                        .create_image_bitmap_with_blob(&blob)
                        .map_err(|e| format!("createImageBitmap failed: {e:?}"))?
                } else {
                    return Err("No global scope available for createImageBitmap".into());
                }
            }
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

        // The readback. THIS is the part worth watching: it is synchronous
        // from here to the end, it runs on the main thread, and it copies
        // width × height × 4 bytes out of the canvas and again into a
        // `ColorImage`. No await inside, so the scope means what it says.
        profiling::scope!("image_loader::readback");

        SCRATCH.with(|scratch| {
            let mut scratch = scratch.borrow_mut();
            // Safe to hold the borrow across the whole readback: there is no
            // `.await` below, so no second decode task can re-enter.
            let scratch = scratch.get_or_try_init()?;
            scratch.resize(width, height);

            // Draw the decoded bitmap onto the canvas
            scratch
                .ctx
                .draw_image_with_image_bitmap(&bitmap, 0.0, 0.0)
                .map_err(|e| format!("drawImage failed: {e:?}"))?;

            // Read back the RGBA pixels
            let image_data = scratch
                .ctx
                .get_image_data(0.0, 0.0, width as f64, height as f64)
                .map_err(|e| format!("getImageData failed: {e:?}"))?;

            let rgba = image_data.data().0;

            Ok(ColorImage::from_rgba_unmultiplied(
                [width as usize, height as usize],
                &rgba,
            ))
        })
    }

    thread_local! {
        /// One canvas, reused by every decode on the (only) wasm thread.
        ///
        /// Built lazily so a frontend that never decodes an image never
        /// allocates one.
        static SCRATCH: std::cell::RefCell<ScratchCanvas> =
            const { std::cell::RefCell::new(ScratchCanvas { inner: None }) };
    }

    /// The canvas every decode draws through, and the 2D context bound to it.
    struct ScratchCanvas {
        inner: Option<(
            web_sys::OffscreenCanvas,
            web_sys::OffscreenCanvasRenderingContext2d,
        )>,
    }

    /// Borrowed view of an initialised [`ScratchCanvas`].
    struct Scratch<'a> {
        canvas: &'a web_sys::OffscreenCanvas,
        ctx: &'a web_sys::OffscreenCanvasRenderingContext2d,
    }

    impl ScratchCanvas {
        fn get_or_try_init(&mut self) -> Result<Scratch<'_>, String> {
            if self.inner.is_none() {
                // 1×1 to start; `resize` grows it to whatever the first image
                // needs. The dimensions here are never used for a readback.
                let canvas = web_sys::OffscreenCanvas::new(1, 1)
                    .map_err(|e| format!("Failed to create OffscreenCanvas: {e:?}"))?;

                // `willReadFrequently: true` is the reason this function
                // exists.
                //
                // Without it the browser is entitled to keep the 2D canvas on
                // the GPU, and then EVERY `getImageData` is a synchronous
                // GPU→CPU readback — it has to flush the pipeline and stall
                // until the surface comes back. Measured here at ~1.9ms for a
                // 256×256 thumbnail, which is roughly 250KB of pixels; the
                // copy is not what costs that. The flag asks for a CPU-backed
                // surface instead, where `getImageData` is a memcpy.
                //
                // Built with `Reflect::set` rather than a serialised literal
                // because it is a JS options bag, not data we own.
                let opts = js_sys::Object::new();
                js_sys::Reflect::set(
                    &opts,
                    &wasm_bindgen::JsValue::from_str("willReadFrequently"),
                    &wasm_bindgen::JsValue::TRUE,
                )
                .map_err(|e| format!("Failed to set willReadFrequently: {e:?}"))?;

                let ctx_obj = canvas
                    .get_context_with_context_options("2d", &opts)
                    .map_err(|e| format!("Failed to get 2d context: {e:?}"))?
                    .ok_or("get_context returned None")?;

                let ctx: web_sys::OffscreenCanvasRenderingContext2d = ctx_obj
                    .dyn_into()
                    .map_err(|_| "Context is not OffscreenCanvasRenderingContext2d".to_string())?;

                self.inner = Some((canvas, ctx));
            }

            let (canvas, ctx) = self.inner.as_ref().expect("just initialised");
            Ok(Scratch { canvas, ctx })
        }
    }

    impl Scratch<'_> {
        /// Size the canvas to this image, if it is not already.
        ///
        /// Only ever grows in practice — the rung ladder means a handful of
        /// distinct sizes — and assigning width/height clears the surface,
        /// which is what we want: no bleed from the previous image around a
        /// smaller one. Skipped when the size already matches, because the
        /// assignment reallocates the backing store even when it is a no-op.
        fn resize(&self, width: u32, height: u32) {
            if self.canvas.width() != width {
                self.canvas.set_width(width);
            }
            if self.canvas.height() != height {
                self.canvas.set_height(height);
            }
        }
    }
}

/// The rung arithmetic, which is pure and therefore testable off wasm.
///
/// Worth testing because every mistake here is expensive and silent: round
/// DOWN and art is blurry, round UP a rung and a grid quietly costs ten times
/// the memory, and neither surfaces as a failure anywhere.
#[cfg(test)]
mod decode_size_tests {
    use super::DecodeSize;
    use egui::load::SizeHint;

    #[test]
    fn a_hint_takes_the_smallest_rung_that_covers_it() {
        let size = |px| {
            DecodeSize::for_hint(SizeHint::Size {
                width: px,
                height: px,
                maintain_aspect_ratio: true,
            })
        };
        // A grid card, at 1x and at 2x device pixel ratio. The 2x case is the
        // common one and the whole reason `Retina` exists: it used to land on
        // `Card` and cost 640 KB a thumbnail to show 168 pixels.
        assert_eq!(size(84), DecodeSize::Tile);
        assert_eq!(size(168), DecodeSize::Retina);
        // Exactly on a rung stays on it rather than stepping up.
        assert_eq!(size(128), DecodeSize::Tile);
        assert_eq!(size(256), DecodeSize::Retina);
        assert_eq!(size(400), DecodeSize::Card);
        assert_eq!(size(1686), DecodeSize::Full);
        // One pixel over has to step up, or the image is upscaled.
        assert_eq!(size(129), DecodeSize::Retina);
        assert_eq!(size(257), DecodeSize::Card);
        assert_eq!(size(401), DecodeSize::Full);
        assert_eq!(size(1687), DecodeSize::Native);
    }

    /// The measured regression, stated as arithmetic: what a 2× grid card
    /// costs per texture. Any future rung change that pushes an ordinary
    /// thumbnail back onto a 400px decode fails here rather than showing up
    /// months later as a gigabyte of texture.
    #[test]
    fn a_retina_grid_card_costs_a_quarter_of_a_megabyte_not_two_thirds() {
        let card_at_2x = DecodeSize::for_hint(SizeHint::Size {
            width: 168,
            height: 168,
            maintain_aspect_ratio: true,
        });
        let px = card_at_2x.px().expect("a grid card is resized") as u64;
        let bytes = px * px * 4;
        assert_eq!(bytes, 256 * 1024, "256px RGBA");
        assert!(
            bytes < 400 * 400 * 4,
            "must stay below what the 400px rung would have cost"
        );
    }

    /// The longest edge decides, so a wide banner is not quietly squashed
    /// into a rung chosen by its height.
    #[test]
    fn the_longest_edge_chooses_the_rung() {
        assert_eq!(
            DecodeSize::for_hint(SizeHint::Size {
                width: 900,
                height: 100,
                maintain_aspect_ratio: true,
            }),
            DecodeSize::Full
        );
        assert_eq!(DecodeSize::for_hint(SizeHint::Width(300)), DecodeSize::Card);
        assert_eq!(DecodeSize::for_hint(SizeHint::Height(64)), DecodeSize::Tile);
        assert_eq!(
            DecodeSize::for_hint(SizeHint::Width(200)),
            DecodeSize::Retina
        );
    }

    /// `Scale` is relative to a source size nothing knows until the decode
    /// has happened, so it can only mean "do not resize" — the same
    /// reading egui gives it.
    #[test]
    fn a_scale_hint_decodes_natively() {
        assert_eq!(
            DecodeSize::for_hint(SizeHint::Scale(1.0.into())),
            DecodeSize::Native
        );
        assert_eq!(
            DecodeSize::for_hint(SizeHint::Scale(0.25.into())),
            DecodeSize::Native
        );
        assert_eq!(DecodeSize::Native.px(), None, "native resizes nothing");
    }

    /// Most rungs match the IIIF service's derivative widths
    /// (`workers/iiif/src/image_size.rs`), because a URL asking for a width
    /// the service does not keep warm still renders — it just misses the R2
    /// object and pays a cold render on every first request, which fails
    /// silently as latency.
    ///
    /// `Retina` is the deliberate exception, and it costs nothing: these
    /// widths govern the DECODE, not the URL. A 400px derivative fetched warm
    /// and decoded down to 256px pays the service's cache hit and a quarter of
    /// the texture. Conflating the two ladders is what produced the 5.7×
    /// overshoot in the first place.
    #[test]
    fn the_rungs_match_the_services_warm_derivatives_except_where_deliberate() {
        assert_eq!(DecodeSize::Tile.px(), Some(128));
        assert_eq!(DecodeSize::Card.px(), Some(400));
        assert_eq!(DecodeSize::Full.px(), Some(1686));
        assert_eq!(
            DecodeSize::Retina.px(),
            Some(256),
            "decode-only rung; the service warms 128/400/1686 and that is fine"
        );
    }
}
