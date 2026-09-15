//! The browser half of [`super::schedule`]: `fetch()` with an abort signal.
//!
//! [`BrowserHttpLoader`] takes over `http(s)` bytes from `egui_extras`'
//! `EhttpLoader`, which fires every request the moment it is asked and cannot
//! stop one. Here the schedule decides what runs, and a load it cancels stops
//! downloading instead of finishing with nobody watching. A collection's
//! thumbnails do not keep downloading after the reader has moved on to the
//! next collection.
//!
//! [`ImageLoads`] is the handle for orchestration the loader cannot work out
//! for itself: cancel on navigation, show counts, retune the policy.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::load::{Bytes, BytesLoadResult, BytesPoll, LoadError};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use super::schedule::{Fetched, Finish, LoadCounts, LoadPolicy, Schedule, Start, Ticket, Want};

/// What a loader shares with its handles.
struct Shared {
    schedule: Mutex<Schedule>,
    /// For fetches started outside `load` — from `end_pass`, a completion or
    /// a handle — which have no `Context` of their own to repaint.
    ///
    /// The handle lives in this context's data, so the two hold each other;
    /// both last as long as the app, so the cycle costs nothing.
    ctx: egui::Context,
}

thread_local! {
    /// In-flight fetches' abort controllers. JS objects are not `Send`, and a
    /// loader has to be, so they are kept here rather than in the schedule.
    /// wasm has only the one thread.
    static ABORTS: RefCell<HashMap<Ticket, web_sys::AbortController>> = RefCell::default();
}

fn abort(tickets: impl IntoIterator<Item = Ticket>) {
    ABORTS.with(|aborts| {
        let mut aborts = aborts.borrow_mut();
        for ticket in tickets {
            if let Some(controller) = aborts.remove(&ticket) {
                controller.abort();
            }
        }
    });
}

/// Seconds. Millisecond granularity is plenty against a grace of a second.
fn now() -> f64 {
    js_sys::Date::now() / 1000.0
}

impl Shared {
    /// Start whatever the schedule now has room for.
    fn pump(self: &Arc<Self>) {
        let starts = self.schedule.lock().unwrap().starts();
        for start in starts {
            spawn_fetch(Arc::clone(self), start);
        }
    }
}

fn spawn_fetch(shared: Arc<Shared>, Start { uri, ticket }: Start) {
    let init = web_sys::RequestInit::new();
    match web_sys::AbortController::new() {
        Ok(controller) => {
            init.set_signal(Some(&controller.signal()));
            ABORTS.with(|aborts| aborts.borrow_mut().insert(ticket, controller));
        }
        // The fetch still runs. Cancelling it then only discards the result;
        // it does not stop the download.
        Err(err) => log::warn!("[image_loader] no AbortController: {err:?}"),
    }

    wasm_bindgen_futures::spawn_local(async move {
        let result = fetch_bytes(&uri, &init).await;
        ABORTS.with(|aborts| aborts.borrow_mut().remove(&ticket));

        let failure = result.as_ref().err().cloned();
        let finish = shared.schedule.lock().unwrap().finish(&uri, ticket, result);
        match finish {
            Finish::Stored => {
                if let Some(err) = failure {
                    log::warn!("[image_loader] {err}");
                }
                shared.ctx.request_repaint();
            }
            // Cancelled while it ran, and usually aborted too — the whole
            // point of this loader, so nothing to report.
            Finish::Stale => {}
        }
        // Its place in the budget is free either way.
        shared.pump();
    });
}

async fn fetch_bytes(uri: &str, init: &web_sys::RequestInit) -> Result<Fetched, String> {
    let global = js_sys::global();
    let promise = if let Some(window) = global.dyn_ref::<web_sys::Window>() {
        window.fetch_with_str_and_init(uri, init)
    } else if let Some(worker) = global.dyn_ref::<web_sys::WorkerGlobalScope>() {
        worker.fetch_with_str_and_init(uri, init)
    } else {
        return Err(format!(
            "failed to load {uri:?}: no global scope to fetch from"
        ));
    };

    let response: web_sys::Response = JsFuture::from(promise)
        .await
        .map_err(|e| format!("failed to load {uri:?}: {e:?}"))?
        .dyn_into()
        .map_err(|_| format!("failed to load {uri:?}: fetch did not return a Response"))?;
    if !response.ok() {
        let status = response.status();
        let status_text = response.status_text();
        return Err(format!("failed to load {uri:?}: {status} {status_text}"));
    }

    let mime = response.headers().get("content-type").ok().flatten();
    let body = response
        .array_buffer()
        .map_err(|e| format!("failed to read {uri:?}: {e:?}"))?;
    let body = JsFuture::from(body)
        .await
        .map_err(|e| format!("failed to read {uri:?}: {e:?}"))?;
    let bytes: Arc<[u8]> = js_sys::Uint8Array::new(&body).to_vec().into();
    Ok(Fetched { bytes, mime })
}

/// Fetches `http(s)` bytes for egui's image loaders on a [`LoadPolicy`]. See
/// the module docs.
///
/// There is no public constructor: [`BrowserHttpLoader::install`] is the only
/// way in, so there is always a handle to reach it with ([`loads`]).
/// [`crate::install_assets`] installs it with the default policy.
pub struct BrowserHttpLoader {
    shared: Arc<Shared>,
}

impl BrowserHttpLoader {
    pub const ID: &'static str = egui::generate_loader_id!(BrowserHttpLoader);

    /// Register the loader on `ctx`, ahead of any http loader already there,
    /// and return its handle.
    ///
    /// Idempotent: if it is already installed this returns the existing handle
    /// and leaves its policy alone. Use [`ImageLoads::set_policy`] to change it.
    pub fn install(ctx: &egui::Context, policy: LoadPolicy) -> ImageLoads {
        if let Some(loads) = loads(ctx) {
            return loads;
        }
        let shared = Arc::new(Shared {
            schedule: Mutex::new(Schedule::new(policy)),
            ctx: ctx.clone(),
        });
        ctx.add_bytes_loader(Arc::new(Self {
            shared: Arc::clone(&shared),
        }));
        let loads = ImageLoads { shared };
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(Self::ID), loads.clone()));
        loads
    }
}

impl egui::load::BytesLoader for BrowserHttpLoader {
    fn id(&self) -> &str {
        Self::ID
    }

    fn load(&self, _ctx: &egui::Context, uri: &str) -> BytesLoadResult {
        if !(uri.starts_with("http://") || uri.starts_with("https://")) {
            return Err(LoadError::NotSupported);
        }
        let want = self.shared.schedule.lock().unwrap().want(uri, now());
        self.shared.pump();
        match want {
            Want::Ready(Fetched { bytes, mime }) => Ok(BytesPoll::Ready {
                size: None,
                bytes: Bytes::Shared(bytes),
                mime,
            }),
            Want::Failed(err) => Err(LoadError::Loading(err)),
            Want::Pending => Ok(BytesPoll::Pending { size: None }),
        }
    }

    /// `ctx.forget_image(uri)` lands here: a pending load is cancelled, a
    /// cached one dropped, and either way the next ask fetches afresh.
    fn forget(&self, uri: &str) {
        let ticket = self.shared.schedule.lock().unwrap().forget(uri);
        abort(ticket);
        self.shared.pump();
    }

    fn forget_all(&self) {
        let tickets = self.shared.schedule.lock().unwrap().forget_all();
        abort(tickets);
    }

    /// Where [`crate::image_loader::schedule::Demand::Visible`] happens: every
    /// image painted this pass has just been asked for, so whatever has gone
    /// unasked past the grace is off screen.
    fn end_pass(&self, _pass_index: u64) {
        let tickets = self.shared.schedule.lock().unwrap().end_pass(now());
        abort(tickets);
        self.shared.pump();
    }

    fn byte_size(&self) -> usize {
        self.shared.schedule.lock().unwrap().byte_size()
    }

    fn has_pending(&self) -> bool {
        self.shared.schedule.lock().unwrap().has_pending()
    }
}

/// A handle on the page's image fetches, for what the loader cannot work out
/// by itself. For example: drop a collection's thumbnails the moment the
/// reader navigates away, instead of a grace period later.
#[derive(Clone)]
pub struct ImageLoads {
    shared: Arc<Shared>,
}

impl ImageLoads {
    pub fn counts(&self) -> LoadCounts {
        self.shared.schedule.lock().unwrap().counts()
    }

    pub fn policy(&self) -> LoadPolicy {
        self.shared.schedule.lock().unwrap().policy()
    }

    pub fn set_policy(&self, policy: LoadPolicy) {
        self.shared.schedule.lock().unwrap().set_policy(policy);
        self.shared.pump();
    }

    /// Abandon every pending load. Anything still on screen is asked for again
    /// on the next pass and starts over, at the front of the queue.
    pub fn cancel_pending(&self) {
        let tickets = self.shared.schedule.lock().unwrap().cancel_pending();
        abort(tickets);
    }

    /// Abandon the pending loads whose URI matches.
    pub fn cancel_where(&self, matches: impl FnMut(&str) -> bool) {
        let tickets = self.shared.schedule.lock().unwrap().cancel_where(matches);
        abort(tickets);
        self.shared.pump();
    }
}

/// The handle [`BrowserHttpLoader::install`] left on `ctx`, if it ran.
pub fn loads(ctx: &egui::Context) -> Option<ImageLoads> {
    ctx.data(|d| d.get_temp(egui::Id::new(BrowserHttpLoader::ID)))
}
