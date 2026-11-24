//! The page half for miniquad: spawn a worker, carry bytes, answer by poll.
//!
//! [`Client`](crate::Client) is wasm-bindgen by construction (`web_sys::Worker`
//! and closures), and a macroquad page can never load it: miniquad's `gl.js`
//! carries none of the glue. This is the sibling that half needs — the ABI, the
//! wire types, the handshake and the refusals are shared, and only the transport
//! differs.
//!
//! # Frame pump, not futures
//!
//! There is no executor under miniquad and no reactor to wake a pending future,
//! so this client cannot resolve a call the way the wasm-bindgen one does. It is
//! polled once a frame instead:
//!
//! ```ignore
//! let mut worker = MiniquadClient::<MyService>::spawn("/my-service.js", |event| { … });
//! let id = worker.call(job);
//! // …each frame:
//! worker.poll();
//! if let Some(result) = worker.take(id) { … }
//! ```
//!
//! One request at a time is the SERVICE's property, not this client's: the ABI
//! permits several and the worker answers them in order, but a host that keeps
//! one in flight — as the fastbuy screen does — never has to reason about
//! interleaving.
//!
//! # Refusals are answers, not hangs
//!
//! [`Link::Rejected`] can be reached before a call is made (the worker would not
//! spawn, its first message did not decode, it speaks another protocol) or while
//! one is in flight (the worker died). Both cases answer every outstanding id
//! with an error: a call that never resolves is worse than one that says why,
//! and the caller is expected to have a route that predates the worker.

use std::collections::HashMap;
use std::marker::PhantomData;

use crate::{Call, Link, Reply, Service};

/// A service running off the main thread, for a host with no executor.
///
/// Dropping this terminates the worker.
pub struct MiniquadClient<S: Service> {
    link: Link,
    next_id: u64,
    /// Requests made before the handshake landed. Posted, in order, on `Ready`.
    queued: Vec<(u64, Vec<u8>)>,
    /// Posted, and not yet answered — so a refusal can fail them rather than
    /// leave the caller polling an id nothing will ever resolve.
    in_flight: Vec<u64>,
    /// Answers that have landed and not been taken.
    done: HashMap<u64, Result<S::Response, String>>,
    on_event: Box<dyn FnMut(S::Event)>,
    _service: PhantomData<S>,
}

impl<S: Service> Drop for MiniquadClient<S> {
    fn drop(&mut self) {
        imp::close();
    }
}

impl<S: Service> MiniquadClient<S> {
    /// Spawn the worker at `url`, reporting progress to `on_event`.
    ///
    /// `url` is the caller's to supply for the same reason it is in
    /// [`Client::spawn`](crate::Client::spawn): which path a host serves its
    /// runtime from is the host's business.
    ///
    /// Never fails loudly. A page that cannot spawn a worker is not a page that
    /// cannot build — it is a page that builds the old way — so the failure is
    /// recorded in [`Self::link`] for the caller to read and route around.
    pub fn spawn(url: &str, on_event: impl FnMut(S::Event) + 'static) -> Self {
        let mut client = Self {
            link: Link::Starting,
            next_id: 0,
            queued: Vec::new(),
            in_flight: Vec::new(),
            done: HashMap::new(),
            on_event: Box::new(on_event),
            _service: PhantomData,
        };
        let why = imp::open(url);
        if !why.is_empty() {
            client.reject(why);
        }
        client
    }

    /// Whether this worker can be used, and if not, why not.
    ///
    /// [`Link::Rejected`] means take whichever route existed before there was a
    /// worker. It is final: the worker has been terminated.
    pub fn link(&self) -> Link {
        self.link.clone()
    }

    /// Run one request, returning the id its answer is filed under.
    ///
    /// Encodes immediately, so a request that cannot be encoded is answered as
    /// an error under the returned id rather than being lost.
    pub fn call(&mut self, request: S::Request) -> u64 {
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;

        match (
            self.link(),
            postcard::to_allocvec(&Call { id, body: request }),
        ) {
            (Link::Rejected(why), _) => {
                self.fail(id, format!("the {} worker was refused: {why}", S::NAME));
            }
            (_, Err(e)) => self.fail(id, format!("could not encode the request: {e}")),
            (Link::Live, Ok(bytes)) => {
                imp::post(&bytes);
                self.in_flight.push(id);
            }
            (Link::Starting, Ok(bytes)) => self.queued.push((id, bytes)),
        }
        id
    }

    /// The answer to [`Self::call`], once it lands.
    ///
    /// Taking is destructive: a second `take` for the same id returns `None`.
    pub fn take(&mut self, id: u64) -> Option<Result<S::Response, String>> {
        self.done.remove(&id)
    }

    /// Move whatever the worker has said since the last frame.
    ///
    /// Call once a frame; nothing else in this type reads the transport.
    pub fn poll(&mut self) {
        // A refusal is final: the worker is terminated and the link is a
        // sentence, so a `Ready` still sitting in the page's queue must not be
        // allowed to put it back in service.
        if matches!(self.link, Link::Rejected(_)) {
            return;
        }
        // Draining BEFORE the failure check, so a reply that arrived in the
        // same frame as an unrelated worker error is still delivered.
        while let Some(bytes) = imp::try_recv() {
            self.receive(&bytes);
        }
        if !matches!(self.link, Link::Rejected(_))
            && let Some(why) = imp::failure()
        {
            self.reject(format!("the {} worker failed: {why}", S::NAME));
        }
    }

    fn receive(&mut self, bytes: &[u8]) {
        let reply: Reply<S::Event, S::Response> = match postcard::from_bytes(bytes) {
            Ok(reply) => reply,
            Err(e) => {
                // BEFORE the handshake this is not a bad message, it is a worker
                // whose wire no longer matches ours — the version check's own
                // failure mode. Treat it as the mismatch it is, or every call
                // waits for a `Ready` that cannot arrive.
                if matches!(self.link, Link::Starting) {
                    self.reject(format!(
                        "its first message did not decode ({e}); the worker is not this build's"
                    ));
                } else {
                    log::error!("{}: undecodable reply: {e}", S::NAME);
                }
                return;
            }
        };

        match reply {
            Reply::Ready { service, .. } if service != S::NAME => self.reject(format!(
                "it serves `{service}`, not `{}` — wrong worker URL",
                S::NAME
            )),
            Reply::Ready { version, .. } if version != S::PROTOCOL => self.reject(format!(
                "it speaks {version}, this page speaks {} — a stale cached worker",
                S::PROTOCOL
            )),
            Reply::Ready { .. } => {
                self.link = Link::Live;
                for (id, bytes) in std::mem::take(&mut self.queued) {
                    imp::post(&bytes);
                    self.in_flight.push(id);
                }
            }
            Reply::Event(event) => (self.on_event)(event),
            Reply::Done { id, result } => {
                self.in_flight.retain(|waiting| *waiting != id);
                self.done.insert(id, result);
            }
        }
    }

    fn fail(&mut self, id: u64, message: String) {
        self.done.insert(id, Err(message));
    }

    /// Give up on the worker, and answer everything waiting on it.
    ///
    /// Terminating is part of the refusal, not tidiness: a worker this page
    /// cannot talk to would otherwise keep posting into a queue nobody drains.
    fn reject(&mut self, why: String) {
        // Loud, and once. This is the diagnosis — without it the symptom is
        // "everything got slow", which sends the reader to the wrong place.
        log::error!("offthread: refusing the {} worker — {why}", S::NAME);
        self.link = Link::Rejected(why.clone());
        imp::close();

        let queued = std::mem::take(&mut self.queued)
            .into_iter()
            .map(|(id, _)| id);
        for id in queued.chain(std::mem::take(&mut self.in_flight)) {
            self.fail(id, format!("the {} worker was refused: {why}", S::NAME));
        }
    }
}

// ---------------------------------------------------------------------------
// The transport
// ---------------------------------------------------------------------------

/// Plugin handshake for `gl.js` — silences the "plugin is present in the JS
/// bundle, but is not used in the rust code" warning, and fails loudly when a
/// stale `offthread-client.js` is served beside a newer wasm.
/// `offthread-client.js` declares `version: 1`, and must match.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
extern "C" fn offthread_crate_version() -> u32 {
    1
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use sapp_jsutils::JsObject;

    // `unsafe extern` and the module link are edition-2024 spelling and
    // rustc 1.96's requirement, the same as `miniquad-platform`'s imports:
    // without `#[link(wasm_import_module = "env")]` these do not link at all.
    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        fn offthread_client_open(url: JsObject) -> JsObject;
        fn offthread_client_post(bytes: JsObject);
        fn offthread_client_try_recv() -> JsObject;
        fn offthread_client_failed() -> i32;
        fn offthread_client_why() -> JsObject;
        fn offthread_client_close();
    }

    fn text(js: JsObject) -> String {
        let mut s = String::new();
        js.to_string(&mut s);
        s
    }

    /// Spawn the worker, returning the reason it could not be spawned.
    ///
    /// Empty means it was. The empty-string convention is `miniquad-platform`'s
    /// `launch_query`'s: one JsObject back, and "absent" needs no second call.
    pub fn open(url: &str) -> String {
        text(unsafe { offthread_client_open(JsObject::string(url)) })
    }

    pub fn post(bytes: &[u8]) {
        unsafe { offthread_client_post(JsObject::buffer(bytes)) }
    }

    pub fn try_recv() -> Option<Vec<u8>> {
        let js = unsafe { offthread_client_try_recv() };
        if js.is_nil() {
            return None;
        }
        let mut bytes = Vec::new();
        js.to_byte_buffer(&mut bytes);
        Some(bytes)
    }

    /// A worker that failed after it was spawned — a module that would not
    /// load, a message that would not read. Asked every frame because the
    /// interesting failures arrive as events, not from the constructor.
    pub fn failure() -> Option<String> {
        (unsafe { offthread_client_failed() } != 0).then(|| text(unsafe { offthread_client_why() }))
    }

    pub fn close() {
        unsafe { offthread_client_close() }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    // No page, no worker. `open` says so rather than pretending, so a native
    // run takes the fallback route exactly as a refused browser would.

    pub fn open(_url: &str) -> String {
        "there is no web worker outside a browser".to_string()
    }

    pub fn post(_bytes: &[u8]) {}

    pub fn try_recv() -> Option<Vec<u8>> {
        None
    }

    pub fn failure() -> Option<String> {
        None
    }

    pub fn close() {}
}
