//! The worker half: a raw wasm ABI, and the loop that runs on it.
//!
//! # The whole interface, and why it is this small
//!
//! Four imports and four exports. A service needs to receive bytes, send
//! bytes, log, and seed a CSPRNG — nothing else. wasm-bindgen exists to make
//! arbitrary JS reachable from Rust, and a worker that does arithmetic has no
//! arbitrary JS to reach. Paying for that generality would cost the one
//! property worth having here: that the artifact is a plain wasm module, which
//! is what lets a macroquad page load it as readily as a wasm-bindgen one.
//!
//! **Exports** (called by the host):
//! - `offthread_alloc(len) -> ptr` — give me somewhere to put `len` bytes.
//! - `offthread_dealloc(ptr, len)` — take it back.
//! - `offthread_on_message(ptr, len)` — here is a request; the worker OWNS
//!   these bytes and frees them.
//! - `offthread_start()` — install the handler and announce readiness.
//!
//! **Imports** (provided by the host):
//! - `offthread_post(ptr, len)` — send these bytes to the page. Synchronous:
//!   the host copies before returning, so the caller keeps ownership.
//! - `offthread_log(level, ptr, len)` — a log line, already formatted.
//! - `offthread_random(ptr, len)` — fill this with CSPRNG bytes.
//!
//! # The allocator contract
//!
//! `alloc`/`dealloc` go through [`std::alloc`] with an explicit [`Layout`],
//! NOT through `Vec::with_capacity` + `from_raw_parts`. The latter is the
//! common trick and it is subtly wrong: `with_capacity` promises *at least*
//! the requested capacity, so reconstructing a `Vec` with `capacity == len`
//! can free with a layout that was never allocated. Reading the request as a
//! borrowed slice and freeing with the layout we allocated avoids the question
//! entirely.
//!
//! # Growing memory invalidates the host's view
//!
//! Every call into wasm may grow the memory, which detaches any
//! `ArrayBuffer` the host is holding. The runtime in `js/offthread.js` derives
//! a fresh view after every call for exactly this reason; it is the single
//! most common way a hand-written wasm bridge corrupts data.

use crate::{Call, Reply, Service};
use std::alloc::Layout;
use std::cell::RefCell;

// ---------------------------------------------------------------------------
// Host imports
// ---------------------------------------------------------------------------

#[link(wasm_import_module = "offthread")]
unsafe extern "C" {
    fn offthread_post(ptr: *const u8, len: usize);
    fn offthread_log(level: u32, ptr: *const u8, len: usize);
    fn offthread_random(ptr: *mut u8, len: usize);
}

/// The host's CSPRNG, as `getrandom`'s custom backend.
///
/// `uplc` reaches `getrandom` through `k256 → elliptic-curve → rand_core`, and
/// the obvious way to satisfy it on wasm is the `js` feature — which pulls in
/// wasm-bindgen and defeats this entire crate. `custom` lets the host answer
/// instead, and the host answers with `crypto.getRandomValues`: the same
/// CSPRNG, seeded from OS entropy, reached over a different ABI.
///
/// ⚠️ Registration only takes effect if this crate is reachable from the final
/// binary. The [`crate::entrypoint!`] macro guarantees that by defining the
/// exports in the binary's own crate.
fn host_random(dest: &mut [u8]) -> Result<(), getrandom_02::Error> {
    // SAFETY: the host writes exactly `len` bytes into a buffer we own.
    unsafe { offthread_random(dest.as_mut_ptr(), dest.len()) };
    Ok(())
}

getrandom_02::register_custom_getrandom!(host_random);

/// Send one already-encoded message to the page.
///
/// The host copies during the call, so the buffer stays ours and drops
/// normally afterwards. No leak, and nothing for the host to free.
fn post_bytes(bytes: &[u8]) {
    // SAFETY: a valid pointer and length into memory we own for the call.
    unsafe { offthread_post(bytes.as_ptr(), bytes.len()) };
}

fn post<E: serde::Serialize, P: serde::Serialize>(reply: &Reply<E, P>) {
    match postcard::to_allocvec(reply) {
        Ok(bytes) => post_bytes(&bytes),
        // Nothing to escalate to: the page's call will resolve on its own
        // terms. Say so here rather than panicking, which would take every
        // later request with it.
        Err(e) => log::error!("offthread: could not encode a reply: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Logging and panics, over the same bridge
// ---------------------------------------------------------------------------

struct HostLog;

impl log::Log for HostLog {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        let line = format!("[{}] {}", record.target(), record.args());
        // SAFETY: a valid pointer and length, copied by the host during the
        // call.
        unsafe { offthread_log(record.level() as u32, line.as_ptr(), line.len()) };
    }

    fn flush(&self) {}
}

/// Route `log` and panics to the host console.
///
/// Called by [`serve`], so a service does not have to remember. Without it a
/// worker is silent — and a silent worker is the worst place for a panic,
/// because the page sees only a call that never answers.
fn install_reporting() {
    let _ = log::set_logger(&HostLog).map(|()| log::set_max_level(log::LevelFilter::Debug));

    std::panic::set_hook(Box::new(|info| {
        let line = format!("offthread: PANIC {info}");
        unsafe { offthread_log(log::Level::Error as u32, line.as_ptr(), line.len()) };
    }));
}

// ---------------------------------------------------------------------------
// Exports
// ---------------------------------------------------------------------------

/// Reserve `len` bytes for the host to write a request into.
///
/// Public because [`crate::entrypoint!`] exports it from the binary's crate;
/// not meant to be called from Rust.
pub fn alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::NonNull::<u8>::dangling().as_ptr();
    }
    match Layout::array::<u8>(len) {
        // SAFETY: a non-zero layout.
        Ok(layout) => unsafe { std::alloc::alloc(layout) },
        Err(_) => std::ptr::null_mut(),
    }
}

/// Release what [`alloc`] handed out.
///
/// # Safety
/// `ptr` must be a pointer [`alloc`] returned for exactly `len` bytes, not yet
/// freed.
pub unsafe fn dealloc(ptr: *mut u8, len: usize) {
    if len == 0 || ptr.is_null() {
        return;
    }
    if let Ok(layout) = Layout::array::<u8>(len) {
        // SAFETY: `ptr` came from `alloc` with this same layout.
        unsafe { std::alloc::dealloc(ptr, layout) };
    }
}

/// The installed handler, type-erased.
///
/// Erased because the export below is `extern "C"` and cannot be generic,
/// while a service is. [`serve`] closes over the concrete types and leaves
/// something that speaks only bytes — which is also all the ABI can carry.
///
/// `FnMut`, so a service may own state across requests (what it decoded for
/// one call and the next reads). Requests are handled one at a time and a
/// handler never re-enters this slot, so the borrow below is never contended.
type Installed = RefCell<Option<Box<dyn FnMut(&[u8])>>>;

thread_local! {
    static HANDLER: Installed = const { RefCell::new(None) };
}

/// Handle one request. The worker owns `ptr` and frees it.
///
/// # Safety
/// `ptr` must be a pointer [`alloc`] returned for exactly `len` bytes, with
/// `len` bytes written into it, not yet freed. Ownership passes here.
pub unsafe fn on_message(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    // Borrowed, not reconstructed into a `Vec` — see the allocator note in the
    // module docs.
    //
    // SAFETY: the host wrote `len` bytes at `ptr`, which came from `alloc`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };

    HANDLER.with(|handler| match handler.borrow_mut().as_mut() {
        Some(run) => run(bytes),
        // Can only happen if the host calls before `offthread_start`, which
        // its runtime does not do — but a silent drop here would look exactly
        // like a hung call from the page.
        None => log::error!("offthread: a request arrived before the service was installed"),
    });

    // SAFETY: the caller's guarantee, passed straight through.
    unsafe { dealloc(ptr, len) };
}

/// Install `handler` and announce readiness. Called by `offthread_start`.
///
/// `handler` is handed each decoded request and an [`Emitter`] for progress.
/// It may be `async`, but see the module docs on this crate: every await must
/// resolve immediately. It may own state that outlives a request (`FnMut`):
/// a service that decodes something once and answers several requests from
/// it keeps it in the closure.
pub fn serve<S, F, Fut>(mut handler: F)
where
    S: Service,
    F: FnMut(S::Request, Emitter<S>) -> Fut + 'static,
    Fut: std::future::Future<Output = Result<S::Response, String>>,
{
    install_reporting();

    HANDLER.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |bytes: &[u8]| {
            let call: Call<S::Request> = match postcard::from_bytes(bytes) {
                Ok(call) => call,
                Err(e) => {
                    // No id, so nothing to answer — the page's version check
                    // is what turns this into a diagnosis.
                    log::error!("offthread: undecodable request: {e}");
                    return;
                }
            };

            let result = match run_now(handler(call.body, Emitter::new())) {
                Some(result) => result,
                None => Err(format!(
                    "{} awaited something that was not ready; this worker runs \
                     compute only and has no reactor to wake it",
                    S::NAME
                )),
            };

            post::<S::Event, S::Response>(&Reply::Done {
                id: call.id,
                result,
            });
        }));
    });

    // ONLY once the handler is installed — a `Ready` sent before it would be
    // a promise the worker cannot keep.
    post::<S::Event, S::Response>(&Reply::Ready {
        service: S::NAME.to_owned(),
        version: S::PROTOCOL,
    });
}

/// Drive a future to completion, or give up immediately.
///
/// One poll completes any future whose awaits are all already-ready, which is
/// every legitimate handler here. A `None` means the handler tried to wait for
/// something, and there is nothing in a worker to wake it — so reporting that
/// beats spinning forever on a thread with no UI to show for it.
fn run_now<T>(future: impl std::future::Future<Output = T>) -> Option<T> {
    use futures::FutureExt as _;
    future.now_or_never()
}

/// A handler's channel for progress.
pub struct Emitter<S: Service> {
    _service: std::marker::PhantomData<S>,
}

impl<S: Service> Emitter<S> {
    fn new() -> Self {
        Self {
            _service: std::marker::PhantomData,
        }
    }

    /// Report progress. Fire-and-forget: the page may not be listening, and a
    /// handler should not slow down to find out.
    pub fn emit(&self, event: S::Event) {
        post::<S::Event, S::Response>(&Reply::Event(event));
    }
}

impl<S: Service> Clone for Emitter<S> {
    fn clone(&self) -> Self {
        Self::new()
    }
}

/// Define a worker binary's exports.
///
/// Every `#[no_mangle]` symbol is defined HERE, in the binary's own crate,
/// rather than in this one. A `#[no_mangle]` function in a dependency is
/// reachable from nothing and the linker is entitled to drop it — which
/// produces a module that builds cleanly and then fails to instantiate for
/// want of its exports. Registering `getrandom`'s custom backend has the same
/// requirement, and the same fix.
///
/// ```ignore
/// offthread::entrypoint!(CartBuild, |request, emitter| async move {
///     cart_actions::local::build_job(request, &emitter).await
/// });
/// ```
#[macro_export]
macro_rules! entrypoint {
    ($service:ty, $handler:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn offthread_alloc(len: usize) -> *mut u8 {
            $crate::abi::alloc(len)
        }

        /// # Safety
        /// The host must pass back exactly what `offthread_alloc` returned.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn offthread_dealloc(ptr: *mut u8, len: usize) {
            unsafe { $crate::abi::dealloc(ptr, len) }
        }

        /// # Safety
        /// The host must pass a buffer from `offthread_alloc` with `len` bytes
        /// written into it. Ownership passes to the module.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn offthread_on_message(ptr: *mut u8, len: usize) {
            unsafe { $crate::abi::on_message(ptr, len) }
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn offthread_start() {
            $crate::abi::serve::<$service, _, _>($handler)
        }
    };
}
