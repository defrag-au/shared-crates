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

        // Retention runs as a BEGIN-PASS PLUGIN rather than from the loader's
        // own `end_pass`, and the difference is not stylistic. Releasing an
        // image means `Context::forget_image`, and `end_pass` on a loader is
        // called from inside `Context::write` — the whole context locked —
        // so forgetting from there deadlocks. On wasm that is not a hang but
        // an outright panic, because `parking_lot` has no thread to park:
        //
        //     parking_lot_core/src/thread_parker/wasm.rs:26
        //     Parking not supported on this platform
        //
        // A plugin callback is handed a `Ui`, so by then the pass has begun
        // and nothing is held. This is egui's own extension point for
        // per-pass work, which makes retention automatic: a consumer installs
        // the loader and gets a bounded cache, with nothing to remember.
        let retention = loads.clone();
        ctx.on_begin_pass(
            "egui-widgets image retention",
            Arc::new(move |_ui: &mut egui::Ui| retention.release_cold()),
        );

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

    /// Where [`crate::image_loader::schedule::Demand::Visible`] and
    /// [`crate::image_loader::schedule::Retain`] happen: every image painted
    /// this pass has just been asked for, so whatever has gone unasked past
    /// the grace is off screen, and whatever is coldest once we are over the
    /// retention cap is what nobody has looked at for longest.
    /// Cancellation only. Retention is [`ImageLoads::release_cold`], and
    /// cannot happen here: releasing an image means `Context::forget_image`,
    /// which takes the same `loaders.bytes` lock egui is holding while it
    /// walks the loaders calling this. Re-entering it deadlocks, and on wasm a
    /// deadlock is not a hang but a panic — `parking_lot` has no thread to
    /// park. No loader callback can mutate the loader set; that is a rule of
    /// the trait, not an accident of ours.
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

    /// Compressed bytes currently held.
    ///
    /// The same figure the `BytesLoader` reports to egui, reachable from the
    /// handle so a caller measuring where its memory went does not have to go
    /// through the loader registry to ask.
    pub fn byte_size(&self) -> usize {
        self.shared.schedule.lock().unwrap().byte_size()
    }

    pub fn policy(&self) -> LoadPolicy {
        self.shared.schedule.lock().unwrap().policy()
    }

    pub fn set_policy(&self, policy: LoadPolicy) {
        self.shared.schedule.lock().unwrap().set_policy(policy);
        self.shared.pump();
    }

    /// Release completed images held beyond the policy's
    /// [`crate::image_loader::schedule::Retain`], coldest first.
    ///
    /// [`BrowserHttpLoader::install`] already runs this at the start of every
    /// pass, so a consumer does not have to. Public for the case the schedule
    /// cannot see: releasing on navigation, rather than a few passes later
    /// once the cap notices.
    ///
    /// Safe only where the context is not already locked — from `update`, a
    /// plugin callback, or an event handler. NOT from inside a loader
    /// callback; see the note in `install`.
    ///
    /// Cheap to call every pass: under the cap it takes the schedule's lock,
    /// finds nothing to do, and returns.
    pub fn release_cold(&self) {
        // Each lock taken and released in its own statement, in order. This
        // callback runs with the pass begun and nothing held, and it stays
        // that way only if it never holds two at once — `forget_image` below
        // reaches for both the loader set and the schedule again.
        // Summed per texture: `bytes_used` is `TextureMeta`'s (width × height
        // × bytes-per-pixel), and the manager has no total of its own. Scoped
        // so the read guard is gone before anything else is taken.
        let texture_bytes: usize = {
            let textures = self.shared.ctx.tex_manager();
            let textures = textures.read();
            textures
                .allocated()
                .map(|(_, meta)| meta.bytes_used())
                .sum()
        };
        let release = self
            .shared
            .schedule
            .lock()
            .unwrap()
            .release_cold(texture_bytes);
        // Through the CONTEXT, not just our own map: this cascades to every
        // loader keyed on the URI, so the decoded image and its texture go
        // with the bytes. Ours is the smallest of the three — see the
        // `schedule` module docs.
        for uri in release {
            self.shared.ctx.forget_image(&uri);
        }
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
