//! `Fetch` and `Load` — a request whose reply is read into the type a screen
//! wants, and that request's whole life as a screen needs to know it: not
//! asked yet, on its way, here, or failed with a reason to show.
//!
//! ```ignore
//! let mut bundle: Load<Bundle> = Load::start_json(Request::get(&url));
//! // each frame:
//! if let Some(b) = bundle.poll() { rebuild(b) }      // the frame it lands
//! match &bundle {
//!     Load::Ready(b) => draw(b),
//!     Load::Failed(why) => Message::new("Unavailable", why).show_in(&p, body),
//!     _ => Message::new("Loading", "…").busy().show_in(&p, body),
//! };
//! ```
//!
//! A failure is a sentence, not an error type: what the reader is told. A
//! reply with a non-2xx status is a failure here (a JSON body expected and
//! not given); a host that acts on a status (a 401) reads it from
//! [`Response`] through [`Fetch::with`].

use std::task::Poll;

use serde::de::DeserializeOwned;

use crate::http::{Pending, Request, Response};

/// A request on its way, and how its reply is read.
pub struct Fetch<T> {
    pending: Pending,
    read: fn(&Response) -> Result<T, String>,
}

/// Why a reply was not what was asked for, in words a screen can show.
pub fn failure(r: &Response) -> String {
    let text = r.text();
    let detail: String = text.trim().chars().take(160).collect();
    match (r.status, detail.is_empty()) {
        (404, _) => "not found".to_string(),
        (status, true) => format!("the server answered {status}"),
        (status, false) => format!("the server answered {status}: {detail}"),
    }
}

impl<T: DeserializeOwned> Fetch<T> {
    /// Send `req`, and read a 2xx reply's body as JSON into `T`.
    pub fn json(req: Request) -> Self {
        Self {
            pending: req.send(),
            read: |r| {
                if !r.ok() {
                    return Err(failure(r));
                }
                r.json::<T>()
                    .map_err(|e| format!("the reply could not be read: {e}"))
            },
        }
    }
}

impl<T> Fetch<T> {
    /// Send `req`, and read its reply with `read`: bytes that are not JSON, or
    /// a status the host acts on.
    pub fn with(req: Request, read: fn(&Response) -> Result<T, String>) -> Self {
        Self {
            pending: req.send(),
            read,
        }
    }

    /// `Ready` exactly once.
    pub fn poll(&mut self) -> Poll<Result<T, String>> {
        match self.pending.poll() {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => Poll::Ready(Err(format!("offline? {}", e.0))),
            Poll::Ready(Ok(r)) => Poll::Ready((self.read)(&r)),
        }
    }
}

/// Something loaded over the network, as a screen needs to know it.
#[derive(Default)]
pub enum Load<T> {
    /// Not asked for yet.
    #[default]
    Idle,
    Loading(Fetch<T>),
    Ready(T),
    Failed(String),
}

impl<T> Load<T> {
    /// Ask for it with `fetch`, forgetting anything it held.
    pub fn start(&mut self, fetch: Fetch<T>) {
        *self = Load::Loading(fetch);
    }

    /// Advance a frame. `Some` with the value on the frame it arrives, and
    /// only then: the moment to rebuild what depends on it.
    pub fn poll(&mut self) -> Option<&T> {
        let Load::Loading(fetch) = self else {
            return None;
        };
        match fetch.poll() {
            Poll::Pending => None,
            Poll::Ready(Ok(v)) => {
                *self = Load::Ready(v);
                self.ready()
            }
            Poll::Ready(Err(e)) => {
                *self = Load::Failed(e);
                None
            }
        }
    }

    pub fn ready(&self) -> Option<&T> {
        match self {
            Load::Ready(v) => Some(v),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Load::Loading(_))
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Load::Failed(e) => Some(e),
            _ => None,
        }
    }
}

impl<T: DeserializeOwned> Load<T> {
    /// A load of `req`'s JSON reply, already on its way.
    pub fn start_json(req: Request) -> Self {
        Load::Loading(Fetch::json(req))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_says_what_the_server_said() {
        let r = |status, body: &str| Response {
            status,
            body: body.as_bytes().to_vec(),
        };
        assert_eq!(failure(&r(404, "")), "not found");
        assert_eq!(failure(&r(503, "")), "the server answered 503");
        assert_eq!(
            failure(&r(500, " durable object reset \n")),
            "the server answered 500: durable object reset"
        );
    }

    #[test]
    fn an_idle_load_has_nothing_and_says_nothing() {
        let mut l: Load<u32> = Load::default();
        assert!(l.poll().is_none());
        assert!(!l.is_loading() && l.ready().is_none() && l.error().is_none());
    }
}
