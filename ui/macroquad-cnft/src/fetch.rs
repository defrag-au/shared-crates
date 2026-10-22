//! `fetch` — GETs and image decodes that a frame loop polls.
//!
//! Two platforms, one surface:
//!
//! - **wasm32**: `js/cnft.js` over the miniquad plugin protocol. The browser
//!   fetches, and the browser decodes images, so any format the IIIF
//!   service picks arrives as RGBA.
//! - **native**: a thread per request with `ureq`, images decoded by
//!   macroquad's `image` (PNG and TGA only, which is why [`crate::art_url`]
//!   asks for PNG off the web). Native is real, not a stub: captures and
//!   soaks run natively against live holdings.
//!
//! A request is a value the caller owns and polls, not an id into a global
//! table. Dropping it abandons it.

use std::task::Poll;

use serde::Deserialize;

/// Why a fetch failed, and so whether it is worth trying again.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FetchError {
    pub stage: Stage,
    /// The HTTP status, or 0 when there was no response.
    pub status: u16,
    pub message: String,
}

/// Where a fetch failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    /// No response at all: offline, DNS, a CORS refusal.
    Network,
    /// A response that was not 2xx.
    Http,
    /// Bytes that are not an image this platform decodes.
    Decode,
}

impl FetchError {
    fn new(stage: Stage, status: u16, message: impl Into<String>) -> Self {
        Self {
            stage,
            status,
            message: message.into(),
        }
    }

    /// Will asking again ever work?
    ///
    /// A 4xx other than a timeout or rate limit will not, and neither will
    /// an undecodable image. A network failure or a 5xx might.
    pub fn is_permanent(&self) -> bool {
        match self.stage {
            Stage::Decode => true,
            Stage::Network => false,
            Stage::Http => (400..500).contains(&self.status) && !matches!(self.status, 408 | 429),
        }
    }
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.stage {
            Stage::Network => write!(f, "network: {}", self.message),
            Stage::Http => write!(f, "HTTP {}: {}", self.status, self.message),
            Stage::Decode => write!(f, "decode: {}", self.message),
        }
    }
}

impl std::error::Error for FetchError {}

/// Decoded pixels: `width * height * 4` bytes of RGBA8.
pub use image_core::DecodedImage;

/// A GET whose body is text. Poll once per frame.
pub struct TextRequest(imp::Text);

/// A GET whose body is decoded to pixels. Poll once per frame.
pub struct ImageRequest(imp::Image);

/// Start a GET. `bearer` is sent as `Authorization: Bearer …` when present.
pub fn get_text(url: &str, bearer: Option<&str>) -> TextRequest {
    TextRequest(imp::Text::start(url, bearer.unwrap_or("")))
}

/// Start fetching and decoding an image.
pub fn get_image(url: &str) -> ImageRequest {
    ImageRequest(imp::Image::start(url))
}

impl TextRequest {
    /// `Ready` exactly once; after that, `Pending` forever.
    pub fn poll(&mut self) -> Poll<Result<String, FetchError>> {
        self.0.poll()
    }
}

impl ImageRequest {
    /// `Ready` exactly once; after that, `Pending` forever.
    ///
    /// The pixels are checked against the dimensions before they are handed
    /// over: a truncated transfer reaching the GPU is a worse failure than a
    /// missing image.
    pub fn poll(&mut self) -> Poll<Result<DecodedImage, FetchError>> {
        match self.0.poll() {
            Poll::Ready(Ok(img)) if !img.is_consistent() => Poll::Ready(Err(FetchError::new(
                Stage::Decode,
                0,
                format!(
                    "expected {} bytes of RGBA, got {}",
                    img.expected_len(),
                    img.rgba.len()
                ),
            ))),
            other => other,
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::task::Poll;

    use sapp_jsutils::JsObject;
    use serde::Deserialize;

    use super::{DecodedImage, FetchError, Stage};

    // `wasm_import_module = "env"` is load-bearing since Rust 1.96: rustc no
    // longer passes `--allow-undefined` to wasm-ld, so an extern block whose
    // bodies JS supplies must name its import module. miniquad's plugins
    // register under `env`.
    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        fn cnft_fetch(url: JsObject, bearer: JsObject) -> i32;
        fn cnft_decode_image(url: JsObject) -> i32;
        fn cnft_image_bytes(id: i32) -> JsObject;
        fn cnft_poll(id: i32) -> JsObject;
    }

    #[derive(Deserialize)]
    #[serde(tag = "status", rename_all = "lowercase")]
    enum Polled {
        Pending,
        Ok { data: String },
        Err { data: String },
    }

    #[derive(Deserialize)]
    struct Size {
        width: u32,
        height: u32,
    }

    fn js_to_string(js: JsObject) -> String {
        let mut s = String::new();
        js.to_string(&mut s);
        s
    }

    /// The plugin's poll, with every malformed reply turned into an error
    /// rather than a panic.
    fn poll_id(id: i32) -> Poll<Result<String, FetchError>> {
        let raw = js_to_string(unsafe { cnft_poll(id) });
        match serde_json::from_str::<Polled>(&raw) {
            Ok(Polled::Pending) => Poll::Pending,
            Ok(Polled::Ok { data }) => Poll::Ready(Ok(data)),
            Ok(Polled::Err { data }) => Poll::Ready(Err(serde_json::from_str(&data)
                .unwrap_or_else(|_| FetchError::new(Stage::Network, 0, data)))),
            Err(e) => Poll::Ready(Err(FetchError::new(
                Stage::Network,
                0,
                format!("malformed poll reply: {e}"),
            ))),
        }
    }

    pub struct Text {
        id: Option<i32>,
    }

    impl Text {
        pub fn start(url: &str, bearer: &str) -> Self {
            let id = unsafe { cnft_fetch(JsObject::string(url), JsObject::string(bearer)) };
            Self { id: Some(id) }
        }

        pub fn poll(&mut self) -> Poll<Result<String, FetchError>> {
            let Some(id) = self.id else {
                return Poll::Pending;
            };
            let out = poll_id(id);
            if out.is_ready() {
                self.id = None;
            }
            out
        }
    }

    pub struct Image {
        id: Option<i32>,
    }

    impl Image {
        pub fn start(url: &str) -> Self {
            let id = unsafe { cnft_decode_image(JsObject::string(url)) };
            Self { id: Some(id) }
        }

        pub fn poll(&mut self) -> Poll<Result<DecodedImage, FetchError>> {
            let Some(id) = self.id else {
                return Poll::Pending;
            };
            let out = match poll_id(id) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => Err(e),
                Poll::Ready(Ok(data)) => match serde_json::from_str::<Size>(&data) {
                    Err(e) => Err(FetchError::new(
                        Stage::Decode,
                        0,
                        format!("unexpected decode reply: {e}"),
                    )),
                    Ok(size) => {
                        let mut rgba = Vec::new();
                        unsafe { cnft_image_bytes(id) }.to_byte_buffer(&mut rgba);
                        Ok(DecodedImage::new(size.width, size.height, rgba))
                    }
                },
            };
            self.id = None;
            Poll::Ready(out)
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::io::Read;
    use std::sync::mpsc::{Receiver, TryRecvError, channel};
    use std::task::Poll;

    use super::{DecodedImage, FetchError, Stage};

    /// Responses past this are refused rather than read: a misconfigured URL
    /// pointing at something huge should fail, not fill memory.
    const MAX_BODY: u64 = 32 * 1024 * 1024;

    /// One background request, polled without blocking.
    struct Pending<T> {
        rx: Option<Receiver<Result<T, FetchError>>>,
    }

    impl<T: Send + 'static> Pending<T> {
        fn spawn(work: impl FnOnce() -> Result<T, FetchError> + Send + 'static) -> Self {
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                // The receiver may be gone: the caller dropped the request.
                let _ = tx.send(work());
            });
            Self { rx: Some(rx) }
        }

        fn poll(&mut self) -> Poll<Result<T, FetchError>> {
            let Some(rx) = &self.rx else {
                return Poll::Pending;
            };
            let out = match rx.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return Poll::Pending,
                Err(TryRecvError::Disconnected) => Err(FetchError::new(
                    Stage::Network,
                    0,
                    "request thread ended without a result",
                )),
            };
            self.rx = None;
            Poll::Ready(out)
        }
    }

    /// A GET's body as bytes, with ureq's errors sorted into stages.
    fn get(url: &str, bearer: &str) -> Result<Vec<u8>, FetchError> {
        let mut req = ureq::get(url);
        if !bearer.is_empty() {
            req = req.set("Authorization", &format!("Bearer {bearer}"));
        }
        let resp = match req.call() {
            Ok(resp) => resp,
            Err(ureq::Error::Status(status, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                let body: String = body.chars().take(500).collect();
                return Err(FetchError::new(Stage::Http, status, body));
            }
            Err(e) => return Err(FetchError::new(Stage::Network, 0, e.to_string())),
        };
        let mut bytes = Vec::new();
        resp.into_reader()
            .take(MAX_BODY + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| FetchError::new(Stage::Network, 0, e.to_string()))?;
        if bytes.len() as u64 > MAX_BODY {
            return Err(FetchError::new(
                Stage::Network,
                0,
                format!("response larger than {MAX_BODY} bytes"),
            ));
        }
        Ok(bytes)
    }

    pub struct Text(Pending<String>);

    impl Text {
        pub fn start(url: &str, bearer: &str) -> Self {
            let (url, bearer) = (url.to_string(), bearer.to_string());
            Self(Pending::spawn(move || {
                let bytes = get(&url, &bearer)?;
                String::from_utf8(bytes)
                    .map_err(|e| FetchError::new(Stage::Decode, 200, e.to_string()))
            }))
        }

        pub fn poll(&mut self) -> Poll<Result<String, FetchError>> {
            self.0.poll()
        }
    }

    pub struct Image(Pending<DecodedImage>);

    impl Image {
        pub fn start(url: &str) -> Self {
            let url = url.to_string();
            Self(Pending::spawn(move || {
                let bytes = get(&url, "")?;
                // CPU only: macroquad's `Image` is the `image` crate's decode,
                // with no GPU context needed, so it is safe off the main
                // thread. It reads PNG and TGA, nothing else.
                let img = macroquad::texture::Image::from_file_with_format(&bytes, None)
                    .map_err(|e| FetchError::new(Stage::Decode, 200, format!("{e:?}")))?;
                Ok(DecodedImage::new(
                    u32::from(img.width),
                    u32::from(img.height),
                    img.bytes,
                ))
            }))
        }

        pub fn poll(&mut self) -> Poll<Result<DecodedImage, FetchError>> {
            self.0.poll()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(stage: Stage, status: u16) -> FetchError {
        FetchError::new(stage, status, "")
    }

    #[test]
    fn only_failures_that_could_succeed_later_are_retried() {
        assert!(err(Stage::Http, 404).is_permanent());
        assert!(err(Stage::Http, 400).is_permanent());
        assert!(err(Stage::Decode, 200).is_permanent());

        assert!(!err(Stage::Http, 503).is_permanent());
        assert!(!err(Stage::Http, 429).is_permanent(), "rate limited");
        assert!(!err(Stage::Http, 408).is_permanent(), "timed out");
        assert!(!err(Stage::Network, 0).is_permanent());
    }

    #[test]
    fn the_plugin_failure_shape_parses() {
        // What `fail()` in js/cnft.js writes.
        let e: FetchError =
            serde_json::from_str(r#"{"stage":"http","status":404,"message":"Not Found"}"#).unwrap();
        assert_eq!(e, FetchError::new(Stage::Http, 404, "Not Found"));
        assert_eq!(e.to_string(), "HTTP 404: Not Found");
    }
}
