//! The page half: spawn a worker, correlate replies, forward progress.
//!
//! This is the wasm-bindgen half, and the only one. `web_sys::Worker` and
//! closures are wasm-bindgen by construction, so a macroquad/miniquad host
//! would need a sibling of THIS FILE — posting through the plugin protocol
//! instead — and nothing else. The ABI, the wire types and `serve` are already
//! runtime-agnostic.

use crate::{Call, Link, Reply, Service};
use futures::channel::oneshot;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::rc::Rc;
use wasm_bindgen::JsCast;

type Waiters<S> =
    Rc<RefCell<HashMap<u64, oneshot::Sender<Result<<S as Service>::Response, String>>>>>;

/// A request encoded and waiting for the handshake to land.
///
/// Carries its own `id` rather than sitting in a `(u64, Vec<u8>)`: the id is
/// how the waiter is found again, and it is worth more than a tuple position
/// if the backlog ever has to be ANSWERED rather than sent — which is exactly
/// what a refused handshake does.
struct Queued {
    id: u64,
    bytes: Vec<u8>,
}

/// A service running off the main thread.
///
/// Dropping this terminates the worker.
pub struct Client<S: Service> {
    worker: web_sys::Worker,
    waiters: Waiters<S>,
    next_id: Cell<u64>,
    queued: Rc<RefCell<Vec<Queued>>>,
    link: Rc<RefCell<Link>>,
    _service: PhantomData<S>,
}

impl<S: Service> Drop for Client<S> {
    fn drop(&mut self) {
        self.worker.terminate();
    }
}

impl<S: Service> Client<S> {
    /// Spawn the worker at `url`, reporting progress to `on_event`.
    ///
    /// `url` is the caller's to supply: which path a host serves its runtime
    /// from is the host's business, and this crate will not guess at a layout
    /// in somebody else's `dist`.
    ///
    /// `Ok` means the worker was SPAWNED, not that it is usable — the
    /// handshake has not happened yet. Ask [`Self::link`] for that.
    pub fn spawn(url: &str, on_event: impl Fn(S::Event) + 'static) -> Result<Self, String> {
        let worker =
            web_sys::Worker::new(url).map_err(|e| format!("{} worker at {url}: {e:?}", S::NAME))?;

        let waiters: Waiters<S> = Rc::new(RefCell::new(HashMap::new()));
        let queued: Rc<RefCell<Vec<Queued>>> = Rc::new(RefCell::new(Vec::new()));
        let link = Rc::new(RefCell::new(Link::Starting));

        let cb_waiters = waiters.clone();
        let cb_queued = queued.clone();
        let cb_link = link.clone();
        let cb_worker = worker.clone();
        let onmessage = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
            move |event: web_sys::MessageEvent| {
                let Some(bytes) = as_bytes(&event.data()) else {
                    return;
                };
                let reply: Reply<S::Event, S::Response> = match postcard::from_bytes(&bytes) {
                    Ok(reply) => reply,
                    Err(e) => {
                        // Undecodable BEFORE the handshake is not a bad
                        // message, it is a worker whose wire no longer matches
                        // ours — the version check's own failure mode. Treat
                        // it as the mismatch it is, or every call queues
                        // forever waiting for a `Ready` that cannot arrive.
                        if matches!(*cb_link.borrow(), Link::Starting) {
                            reject::<S>(
                                &cb_link,
                                &cb_queued,
                                &cb_waiters,
                                format!(
                                    "its first message did not decode ({e}); the worker \
                                     is not this build's"
                                ),
                            );
                        } else {
                            log::error!("{}: undecodable reply: {e}", S::NAME);
                        }
                        return;
                    }
                };

                match reply {
                    Reply::Ready { service, .. } if service != S::NAME => reject::<S>(
                        &cb_link,
                        &cb_queued,
                        &cb_waiters,
                        format!(
                            "it serves `{service}`, not `{}` — wrong worker URL",
                            S::NAME
                        ),
                    ),
                    Reply::Ready { version, .. } if version != S::PROTOCOL => reject::<S>(
                        &cb_link,
                        &cb_queued,
                        &cb_waiters,
                        format!(
                            "it speaks {version}, this page speaks {} — a stale cached worker",
                            S::PROTOCOL
                        ),
                    ),
                    Reply::Ready { .. } => {
                        *cb_link.borrow_mut() = Link::Live;
                        for request in cb_queued.borrow_mut().drain(..) {
                            let array = js_sys::Uint8Array::from(request.bytes.as_slice());
                            let _ = cb_worker.post_message(&array);
                        }
                    }
                    Reply::Event(event) => on_event(event),
                    Reply::Done { id, result } => {
                        // A reply with no waiter is a call whose caller went
                        // away — drop it rather than treating it as an error.
                        if let Some(tx) = cb_waiters.borrow_mut().remove(&id) {
                            let _ = tx.send(result);
                        }
                    }
                }
            },
        );
        worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        Ok(Self {
            worker,
            waiters,
            next_id: Cell::new(0),
            queued,
            link,
            _service: PhantomData,
        })
    }

    /// Whether this worker can be used, and if not, why not.
    ///
    /// Callers should consult this rather than assume a spawned worker works:
    /// [`Link::Rejected`] means fall back to whatever route existed before
    /// there was a worker, NOT that the work is impossible.
    pub fn link(&self) -> Link {
        self.link.borrow().clone()
    }

    /// Run one request. The future resolves when THIS call answers.
    ///
    /// On a rejected worker it fails immediately rather than queueing — the
    /// caller is expected to have taken another route already, and a call that
    /// hangs forever is worse than one that says why it cannot run.
    pub fn call(
        &self,
        request: S::Request,
    ) -> impl std::future::Future<Output = Result<S::Response, String>> + 'static {
        let id = self.next_id.get().wrapping_add(1);
        self.next_id.set(id);

        let (tx, rx) = oneshot::channel();

        match (
            self.link(),
            postcard::to_allocvec(&Call { id, body: request }),
        ) {
            (Link::Rejected(why), _) => {
                let _ = tx.send(Err(format!("the {} worker was refused: {why}", S::NAME)));
            }
            (_, Err(e)) => {
                let _ = tx.send(Err(format!("could not encode the request: {e}")));
            }
            (Link::Live, Ok(bytes)) => {
                self.waiters.borrow_mut().insert(id, tx);
                let array = js_sys::Uint8Array::from(bytes.as_slice());
                if let Err(e) = self.worker.post_message(&array) {
                    // Take our own waiter back out and answer it here — the
                    // worker never saw this call, so nothing else will.
                    if let Some(tx) = self.waiters.borrow_mut().remove(&id) {
                        let _ = tx.send(Err(format!("post failed: {e:?}")));
                    }
                }
            }
            (Link::Starting, Ok(bytes)) => {
                self.waiters.borrow_mut().insert(id, tx);
                self.queued.borrow_mut().push(Queued { id, bytes });
            }
        }

        async move {
            rx.await
                .unwrap_or_else(|_| Err("the worker went away".to_string()))
        }
    }
}

fn as_bytes(value: &wasm_bindgen::JsValue) -> Option<Vec<u8>> {
    value.dyn_ref::<js_sys::Uint8Array>().map(|a| a.to_vec())
}

/// Take the link out of service and answer everything waiting on it.
///
/// Both halves matter. Recording the reason is what makes the next `call` fail
/// fast; ANSWERING the queue is what stops the calls already made from waiting
/// on a handshake that is never going to arrive.
fn reject<S: Service>(
    link: &Rc<RefCell<Link>>,
    queued: &Rc<RefCell<Vec<Queued>>>,
    waiters: &Waiters<S>,
    why: String,
) {
    // Loud, and once. This is the diagnosis — without it the symptom is
    // "everything got slow", which sends the reader to the wrong place.
    log::error!("offthread: refusing the {} worker — {why}", S::NAME);
    *link.borrow_mut() = Link::Rejected(why.clone());

    let waiting: Vec<u64> = queued.borrow_mut().drain(..).map(|q| q.id).collect();
    for id in waiting {
        if let Some(tx) = waiters.borrow_mut().remove(&id) {
            let _ = tx.send(Err(format!("the {} worker was refused: {why}", S::NAME)));
        }
    }
}
