//! `http` — a request with any method, headers and body, whose reply keeps
//! its status and its bytes, polled once a frame.
//!
//! ```ignore
//! let mut call = Request::post(&url).bearer(&token).json(&body)?.send();
//! // each frame:
//! if let Poll::Ready(reply) = call.poll() {
//!     match reply {
//!         Ok(r) if r.status == 401 => sign_out(),
//!         Ok(r) => self.data = r.json::<Bundle>()?,
//!         Err(e) => self.offline(e),
//!     }
//! }
//! ```
//!
//! **Any status is a reply.** A 401 or a 404 is an answer the app acts on, so
//! it arrives as a [`Response`] with that status; only no reply at all
//! (offline, a DNS failure, a CORS refusal) is an [`HttpError`]. That is the
//! difference from `quad-net`, whose reply carries no status, and from
//! `macroquad-cnft`'s GET, which reads every non-2xx as a failure.
//!
//! The web build fetches through `js/app.js`; natively a thread runs each
//! request with `ureq`. A request is a value the caller owns: dropping it
//! abandons the reply.

use std::task::Poll;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// The verbs an app's API takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

/// A request being built. [`Request::send`] starts it.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn new(method: Method, url: &str) -> Self {
        Self {
            method,
            url: url.to_string(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn get(url: &str) -> Self {
        Self::new(Method::Get, url)
    }

    pub fn post(url: &str) -> Self {
        Self::new(Method::Post, url)
    }

    pub fn put(url: &str) -> Self {
        Self::new(Method::Put, url)
    }

    pub fn delete(url: &str) -> Self {
        Self::new(Method::Delete, url)
    }

    /// One header. A newline in a name or value would split it in the page's
    /// encoding, so one is refused there by being dropped here.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        if !name.contains('\n') && !value.contains('\n') {
            self.headers.push((name.to_string(), value.to_string()));
        }
        self
    }

    /// `Authorization: Bearer token`.
    pub fn bearer(self, token: &str) -> Self {
        self.header("Authorization", &format!("Bearer {token}"))
    }

    /// A raw body and its content type.
    pub fn body(mut self, bytes: Vec<u8>, content_type: &str) -> Self {
        self.body = bytes;
        self.header("Content-Type", content_type)
    }

    /// `value` as a JSON body.
    pub fn json<T: Serialize>(self, value: &T) -> Result<Self, serde_json::Error> {
        let bytes = serde_json::to_vec(value)?;
        Ok(self.body(bytes, "application/json"))
    }

    /// Start it. Poll the result once a frame.
    pub fn send(self) -> Pending {
        Pending(imp::Call::start(self))
    }
}

/// A reply: its status, and its body as bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Response {
    /// A 2xx status.
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The body as text, invalid UTF-8 replaced.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The body decoded as JSON into `T`.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }
}

/// No reply at all: offline, a DNS failure, a CORS refusal, a dropped
/// connection. Worth trying again later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpError(pub String);

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no reply: {}", self.0)
    }
}

impl std::error::Error for HttpError {}

/// A request on its way. Poll once a frame.
pub struct Pending(imp::Call);

impl Pending {
    /// `Ready` exactly once; after that, `Pending` forever.
    pub fn poll(&mut self) -> Poll<Result<Response, HttpError>> {
        self.0.poll()
    }
}

/// Headers as the page's plugin reads them: `name\nvalue\n` pairs.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn header_lines(headers: &[(String, String)]) -> String {
    headers.iter().map(|(k, v)| format!("{k}\n{v}\n")).collect()
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::task::Poll;

    use sapp_jsutils::JsObject;

    use super::{HttpError, Request, Response};

    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        fn app_http(method: JsObject, url: JsObject, headers: JsObject, body: JsObject) -> i32;
        fn app_http_state(id: i32) -> u32;
        fn app_http_status(id: i32) -> u32;
        fn app_http_take(id: i32) -> JsObject;
        fn app_http_error(id: i32) -> JsObject;
        fn app_http_forget(id: i32);
    }

    pub struct Call {
        id: Option<i32>,
    }

    impl Call {
        pub fn start(req: Request) -> Self {
            let id = unsafe {
                app_http(
                    JsObject::string(req.method.as_str()),
                    JsObject::string(&req.url),
                    JsObject::string(&super::header_lines(&req.headers)),
                    JsObject::buffer(&req.body),
                )
            };
            Self { id: Some(id) }
        }

        pub fn poll(&mut self) -> Poll<Result<Response, HttpError>> {
            let Some(id) = self.id else {
                return Poll::Pending;
            };
            let out = match unsafe { app_http_state(id) } {
                0 => return Poll::Pending,
                1 => {
                    let status = unsafe { app_http_status(id) } as u16;
                    let mut body = Vec::new();
                    unsafe { app_http_take(id) }.to_byte_buffer(&mut body);
                    Ok(Response { status, body })
                }
                2 => {
                    let mut message = String::new();
                    unsafe { app_http_error(id) }.to_string(&mut message);
                    Err(HttpError(message))
                }
                _ => Err(HttpError("the page lost the request".to_string())),
            };
            self.id = None;
            Poll::Ready(out)
        }
    }

    impl Drop for Call {
        fn drop(&mut self) {
            if let Some(id) = self.id {
                unsafe { app_http_forget(id) }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::io::Read;
    use std::sync::mpsc::{Receiver, TryRecvError, channel};
    use std::task::Poll;

    use super::{HttpError, Method, Request, Response};

    /// Replies past this are refused rather than read: a URL pointing at
    /// something huge should fail, not fill memory.
    const MAX_BODY: u64 = 64 * 1024 * 1024;

    pub struct Call {
        rx: Option<Receiver<Result<Response, HttpError>>>,
    }

    impl Call {
        pub fn start(req: Request) -> Self {
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                // The receiver may be gone: the caller dropped the request.
                let _ = tx.send(run(req));
            });
            Self { rx: Some(rx) }
        }

        pub fn poll(&mut self) -> Poll<Result<Response, HttpError>> {
            let Some(rx) = &self.rx else {
                return Poll::Pending;
            };
            let out = match rx.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return Poll::Pending,
                Err(TryRecvError::Disconnected) => Err(HttpError(
                    "request thread ended without a reply".to_string(),
                )),
            };
            self.rx = None;
            Poll::Ready(out)
        }
    }

    fn run(req: Request) -> Result<Response, HttpError> {
        let mut call = ureq::request(req.method.as_str(), &req.url);
        for (k, v) in &req.headers {
            call = call.set(k, v);
        }
        let sent = match req.method {
            Method::Get | Method::Delete if req.body.is_empty() => call.call(),
            _ => call.send_bytes(&req.body),
        };
        let resp = match sent {
            Ok(resp) => resp,
            // A status is a reply, whatever it is.
            Err(ureq::Error::Status(_, resp)) => resp,
            Err(e) => return Err(HttpError(e.to_string())),
        };
        let status = resp.status();
        let mut body = Vec::new();
        resp.into_reader()
            .take(MAX_BODY + 1)
            .read_to_end(&mut body)
            .map_err(|e| HttpError(e.to_string()))?;
        if body.len() as u64 > MAX_BODY {
            return Err(HttpError(format!("reply larger than {MAX_BODY} bytes")));
        }
        Ok(Response { status, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_json_body_carries_its_content_type() {
        #[derive(Serialize)]
        struct Names<'a> {
            stakes: &'a [&'a str],
        }
        let req = Request::post("https://example.test/resolve")
            .bearer("t0k")
            .json(&Names { stakes: &["a"] })
            .unwrap();
        assert_eq!(req.body, br#"{"stakes":["a"]}"#);
        assert_eq!(
            req.headers,
            vec![
                ("Authorization".to_string(), "Bearer t0k".to_string()),
                ("Content-Type".to_string(), "application/json".to_string()),
            ]
        );
    }

    #[test]
    fn a_header_that_would_split_the_encoding_is_dropped() {
        let req = Request::get("u")
            .header("X-A", "one\ntwo")
            .header("X-B", "ok");
        assert_eq!(header_lines(&req.headers), "X-B\nok\n");
    }

    #[test]
    fn any_status_is_a_reply() {
        let r = Response {
            status: 401,
            body: b"{\"error\":\"expired\"}".to_vec(),
        };
        assert!(!r.ok());
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct Failure {
            error: String,
        }
        assert_eq!(
            r.json::<Failure>().unwrap(),
            Failure {
                error: "expired".to_string()
            }
        );
    }
}
