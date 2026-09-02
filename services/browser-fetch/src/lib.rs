//! JSON fetch for wasm frontends.
//!
//! Ten hand-rolled copies of this existed in `cnft.dev-workers` before it was
//! extracted, and they had already drifted apart in a way that cost real
//! debugging time: most reported a bare `HTTP 500` and discarded the response
//! body, so the server's actual explanation — which every one of our origins
//! sends as plain text — never reached anyone. One copy read it. That is the
//! behaviour this crate standardises.
//!
//! # The error type is the point
//!
//! [`Error`] separates three outcomes a `Result<T, String>` collapses into one:
//!
//! - [`Error::Network`] — the request never completed. Offline, CORS, DNS.
//! - [`Error::Status`] — the server answered, and said no. **Carries the body.**
//! - [`Error::Decode`] — it answered with something we could not read.
//!
//! Callers act on these differently and should be able to. A `404` on a policy
//! means "nobody has indexed this yet" and deserves an invitation; a decode
//! failure on the same endpoint means the wire has drifted and deserves a bug
//! report. Told apart only by substring-matching a `String`, both render as the
//! same red box.

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Why a fetch did not produce a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The request never reached a server, or the response never arrived.
    ///
    /// Retryable in principle, and the only variant where retrying is sensible
    /// without changing anything.
    Network(String),
    /// The server answered with a non-2xx status.
    ///
    /// `body` is the response text, empty when there was none or it could not
    /// be read. This is the field the copies used to throw away.
    Status { status: u16, body: String },
    /// The response arrived but was not the shape the caller asked for.
    ///
    /// Almost always wire drift between a frontend and an origin that were
    /// deployed at different times — which is exactly why it must not look
    /// like a server error.
    Decode(String),
}

impl Error {
    /// The HTTP status, when the server got far enough to send one.
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Status { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// Did the server say "no such thing"?
    ///
    /// Broken out because it is the one status frontends routinely branch on:
    /// a 404 is usually an empty state to invite the reader into, not an error
    /// to apologise for.
    pub fn is_not_found(&self) -> bool {
        self.status() == Some(404)
    }

    /// Is retrying the same request plausibly worthwhile?
    ///
    /// Network failures and 5xx, yes. A 4xx will answer the same way forever —
    /// retrying one is how a frontend turns its own bad request into a loop.
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Network(_) => true,
            Error::Status { status, .. } => *status >= 500,
            Error::Decode(_) => false,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Network(m) => write!(f, "network: {m}"),
            // The body goes in the message, not just the status — a caller that
            // only ever logs `{e}` still gets the server's explanation.
            Error::Status { status, body } if body.is_empty() => write!(f, "HTTP {status}"),
            Error::Status { status, body } => write!(f, "HTTP {status}: {body}"),
            Error::Decode(m) => write!(f, "decode: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// One request header, as a name/value pair.
///
/// Generic rather than bearer-only because the estate is not bearer-only: the
/// collection-ownership admin surface sends `Authorization: Bearer <jwt>` for a
/// Discord session and `X-Debug-Token: <value>` for the operator bypass, and a
/// helper that could only express the first would have left those call sites
/// with their own copy — which is the thing this crate exists to end.
pub type Header<'a> = (&'a str, &'a str);

/// `Authorization: Bearer <token>`, as a header pair.
///
/// A function rather than each caller formatting it, so "Bearer " is spelled
/// once. Note it takes the RAW token — a value that already says `Bearer ` will
/// end up saying it twice, which reads as a malformed token to every origin.
pub fn bearer(token: &str) -> (String, String) {
    ("Authorization".to_string(), format!("Bearer {token}"))
}

/// GET a URL and deserialize the JSON body.
pub async fn get_json<T: DeserializeOwned>(url: &str) -> Result<T, Error> {
    request(url, "GET", None, &[]).await
}

/// GET with a bearer token. `None` sends no `Authorization` header at all,
/// rather than an empty one — an origin that gates on the header's presence
/// treats those differently.
pub async fn get_json_auth<T: DeserializeOwned>(
    url: &str,
    token: Option<&str>,
) -> Result<T, Error> {
    match token {
        Some(t) => {
            let (name, value) = bearer(t);
            request(url, "GET", None, &[(&name, &value)]).await
        }
        None => request(url, "GET", None, &[]).await,
    }
}

/// GET with arbitrary headers — the escape hatch for auth that is not a bearer.
pub async fn get_json_with<T: DeserializeOwned>(
    url: &str,
    headers: &[Header<'_>],
) -> Result<T, Error> {
    request(url, "GET", None, headers).await
}

/// POST a JSON body and deserialize the JSON response.
pub async fn post_json<T: DeserializeOwned, B: Serialize>(url: &str, body: &B) -> Result<T, Error> {
    let encoded = serde_json::to_string(body).map_err(|e| Error::Decode(e.to_string()))?;
    request(url, "POST", Some(encoded), &[]).await
}

/// POST with a bearer token.
pub async fn post_json_auth<T: DeserializeOwned, B: Serialize>(
    url: &str,
    body: &B,
    token: Option<&str>,
) -> Result<T, Error> {
    match token {
        Some(t) => {
            let (name, value) = bearer(t);
            send_json_with("POST", url, Some(body), &[(&name, &value)]).await
        }
        None => post_json(url, body).await,
    }
}

/// Any method, any headers, optional JSON body — what the bearer helpers are
/// shorthand for.
///
/// `body: Option<&B>` rather than a separate no-body function because PATCH and
/// DELETE take one about as often as they don't, and two near-identical
/// functions is how the copies started.
pub async fn send_json_with<T: DeserializeOwned, B: Serialize>(
    method: &str,
    url: &str,
    body: Option<&B>,
    headers: &[Header<'_>],
) -> Result<T, Error> {
    let encoded = match body {
        Some(b) => Some(serde_json::to_string(b).map_err(|e| Error::Decode(e.to_string()))?),
        None => None,
    };
    request(url, method, encoded, headers).await
}

/// Any method, any headers, optional body — response DISCARDED.
///
/// For the PUT/DELETE/POST calls where the status is the entire answer.
/// Distinct from [`send_json_with`] because `()` does not deserialize from an
/// arbitrary JSON body, so "I don't care what came back" cannot be spelled as a
/// return type — it has to be its own function.
pub async fn send_with<B: Serialize>(
    method: &str,
    url: &str,
    body: Option<&B>,
    headers: &[Header<'_>],
) -> Result<(), Error> {
    let encoded = match body {
        Some(b) => Some(serde_json::to_string(b).map_err(|e| Error::Decode(e.to_string()))?),
        None => None,
    };
    send(url, method, encoded, headers).await.map(|_| ())
}

/// DELETE, with an optional bearer. Returns nothing — the status IS the answer.
pub async fn delete(url: &str, token: Option<&str>) -> Result<(), Error> {
    match token {
        Some(t) => {
            let (name, value) = bearer(t);
            send_with::<()>("DELETE", url, None, &[(&name, &value)]).await
        }
        None => send_with::<()>("DELETE", url, None, &[]).await,
    }
}

/// GET raw bytes — artifacts, images, anything not JSON.
///
/// The browser handles content-encoding transparently, so a gzipped artifact
/// arrives decompressed and this returns exactly what the origin stored.
pub async fn get_bytes(url: &str) -> Result<Vec<u8>, Error> {
    use wasm_bindgen_futures::JsFuture;
    let resp = send(url, "GET", None, &[]).await?;
    let buf = JsFuture::from(
        resp.array_buffer()
            .map_err(|e| Error::Decode(format!("{e:?}")))?,
    )
    .await
    .map_err(|e| Error::Decode(format!("{e:?}")))?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}

/// Fire a GET and never read the response — a cache warm.
///
/// Deliberately not a `Result`: the caller does not await an answer and there
/// is nothing to do about a failure. The point is to make the edge start work
/// while the reader is still deciding, and reading a 240 KB body nobody wants
/// would defeat it.
pub fn warm(url: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    // The promise is dropped, not awaited. The request is still sent — the
    // browser does not cancel an in-flight fetch just because nothing holds
    // its promise.
    let _ = window.fetch_with_str(url);
}

/// GET/POST JSON. Everything typed is a shape over [`send`].
async fn request<T: DeserializeOwned>(
    url: &str,
    method: &str,
    body: Option<String>,
    headers: &[Header<'_>],
) -> Result<T, Error> {
    use wasm_bindgen_futures::JsFuture;
    let resp = send(url, method, body, headers).await?;
    let json = JsFuture::from(resp.json().map_err(|e| Error::Decode(format!("{e:?}")))?)
        .await
        .map_err(|e| Error::Decode(format!("{e:?}")))?;
    serde_wasm_bindgen::from_value(json).map_err(|e| Error::Decode(e.to_string()))
}

/// The one implementation: build the request, send it, and turn a non-2xx into
/// an [`Error::Status`] that carries the body.
async fn send(
    url: &str,
    method: &str,
    body: Option<String>,
    extra: &[Header<'_>],
) -> Result<web_sys::Response, Error> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or_else(|| Error::Network("no window".into()))?;

    let opts = web_sys::RequestInit::new();
    opts.set_method(method);
    if let Some(body) = &body {
        opts.set_body(&wasm_bindgen::JsValue::from_str(body));
    }

    let headers = web_sys::Headers::new().map_err(|e| Error::Network(format!("{e:?}")))?;
    if body.is_some() {
        headers
            .set("Content-Type", "application/json")
            .map_err(|e| Error::Network(format!("{e:?}")))?;
    }
    for (name, value) in extra {
        headers
            .set(name, value)
            .map_err(|e| Error::Network(format!("{e:?}")))?;
    }
    opts.set_headers(&headers);

    let request = web_sys::Request::new_with_str_and_init(url, &opts)
        .map_err(|e| Error::Network(format!("{e:?}")))?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| Error::Network(format!("{e:?}")))?;
    let resp: web_sys::Response = resp_value
        .dyn_into()
        .map_err(|_| Error::Network("response cast failed".into()))?;

    if !resp.ok() {
        // READ THE BODY. Best-effort — a failure to read it must not replace
        // the status, which is the part we definitely know.
        let body = match resp.text() {
            Ok(promise) => JsFuture::from(promise)
                .await
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default(),
            Err(_) => String::new(),
        };
        return Err(Error::Status {
            status: resp.status(),
            body,
        });
    }
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The body reaches the message. This is the whole reason the crate exists:
    /// every origin in the estate explains itself in the response text, and the
    /// copies this replaces logged only the number.
    #[test]
    fn a_status_error_carries_the_servers_explanation() {
        let e = Error::Status {
            status: 422,
            body: "this is a smart contract, not a wallet".into(),
        };
        assert_eq!(
            e.to_string(),
            "HTTP 422: this is a smart contract, not a wallet"
        );
    }

    /// An empty body must not produce a dangling colon — the status alone is a
    /// complete sentence.
    #[test]
    fn an_empty_body_leaves_the_status_alone() {
        let e = Error::Status {
            status: 500,
            body: String::new(),
        };
        assert_eq!(e.to_string(), "HTTP 500");
    }

    /// A 404 is usually an empty state to invite the reader into, not a failure
    /// to apologise for, so it is worth asking about directly.
    #[test]
    fn not_found_is_distinguishable_without_string_matching() {
        assert!(Error::Status {
            status: 404,
            body: String::new()
        }
        .is_not_found());
        assert!(!Error::Status {
            status: 500,
            body: String::new()
        }
        .is_not_found());
        assert!(!Error::Network("offline".into()).is_not_found());
    }

    /// Retrying a 4xx is how a frontend turns its own bad request into a loop.
    #[test]
    fn only_network_and_server_failures_are_retryable() {
        assert!(Error::Network("offline".into()).is_retryable());
        assert!(Error::Status {
            status: 503,
            body: String::new()
        }
        .is_retryable());
        assert!(!Error::Status {
            status: 400,
            body: String::new()
        }
        .is_retryable());
        assert!(
            !Error::Decode("missing field".into()).is_retryable(),
            "wire drift will not fix itself on a retry"
        );
    }

    /// Decode failures must not read as server errors: the server did its job,
    /// the two sides were deployed at different times.
    #[test]
    fn a_decode_failure_reads_as_ours_not_theirs() {
        let e = Error::Decode("missing field `direction`".into());
        assert_eq!(e.to_string(), "decode: missing field `direction`");
        assert_eq!(e.status(), None);
    }
}
