//! Custom `getrandom` backend for the miniquad WASM runtime.
//!
//! `getrandom 0.2` errors at compile time on `wasm32-unknown-unknown`
//! unless one of `js`, `custom`, or 0.3's `wasm_js` is on. The `js`
//! feature pulls `wasm-bindgen`, which miniquad's plugin-based bridge
//! doesn't satisfy. The `custom` feature lets us register our own
//! backend — which we route through a miniquad plugin (`js/platform.js`)
//! that calls `crypto.getRandomValues()`.
//!
//! The browser's `crypto.getRandomValues()` is a CSPRNG seeded from
//! OS entropy. Functionally equivalent to the wasm-bindgen path; same
//! security guarantees, different ABI.
//!
//! ## Wiring it into a consumer
//!
//! 1. Add this crate as a dep.
//! 2. In your binary crate's `main.rs` add `extern crate miniquad_platform;`
//!    (or, equivalently, `use miniquad_platform as _;` at the top level).
//!    This forces the custom-getrandom symbol into the wasm exports — the
//!    `register_custom_getrandom!` macro registration only takes effect
//!    if the crate is referenced from a reachable code path.
//! 3. Add the `platform.js` plugin to your HTML shell, BEFORE `gl.js`
//!    runs `load(...)`. The JS file's content is exposed as
//!    [`PLUGIN_JS`] so consumer build scripts can stamp it onto disk:
//!
//!    ```ignore
//!    // build.rs
//!    fs::write("web/platform.js", miniquad_platform::PLUGIN_JS)?;
//!    ```
//!
//! Native builds skip all this — `getrandom`'s native backends work
//! out of the box and the register_custom_getrandom macro is a no-op.

/// JavaScript source for the miniquad plugin. Consumers materialise
/// this into their HTML shell directory (typically via `build.rs`).
pub const PLUGIN_JS: &str = include_str!("../js/platform.js");

#[cfg(target_arch = "wasm32")]
use sapp_jsutils::JsObject;

// `unsafe extern` and `#[unsafe(no_mangle)]` below are edition-2024 syntax:
// the edition made both explicit rather than implicit. Purely a spelling
// change — same ABI, same behaviour.
// `#[link(wasm_import_module = "env")]` is the sanctioned fix for rustc 1.96+,
// which stopped passing `--allow-undefined` to wasm-ld: without it these
// imports do not link at all. Note the failure is at LINK time only — a plain
// `cargo check` still passes, which is how this class of breakage reaches a
// deploy unnoticed.
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn platform_random_bytes(len: u32) -> JsObject;
    fn platform_query_param(key: JsObject) -> JsObject;
    fn platform_location_host() -> JsObject;
    fn platform_location_path() -> JsObject;
    fn platform_reload();
    fn platform_storage_get(key: JsObject) -> JsObject;
    fn platform_storage_set(key: JsObject, value: JsObject);
    fn platform_storage_remove(key: JsObject);
    fn platform_fullscreen_zone(x0: f32, y0: f32, x1: f32, y1: f32);
    fn platform_fullscreen_state() -> u32;
}

/// Whether the page can go fullscreen, and whether it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fullscreen {
    /// No fullscreen here: an iPhone's Safari (only video gets it; Add to
    /// Home Screen is the way there), an embed that does not allow it, or a
    /// native build. Offer no button.
    Unavailable,
    Windowed,
    Full,
}

/// The page's fullscreen now.
#[cfg(target_arch = "wasm32")]
pub fn fullscreen() -> Fullscreen {
    match unsafe { platform_fullscreen_state() } {
        2 => Fullscreen::Full,
        1 => Fullscreen::Windowed,
        _ => Fullscreen::Unavailable,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn fullscreen() -> Fullscreen {
    Fullscreen::Unavailable
}

/// Where the game's fullscreen button is, as `[left, top, right, bottom]`
/// fractions of the canvas, or `None` while there is none. Call it every
/// frame the button is drawn: a layout moves it.
///
/// A browser grants fullscreen only from inside a gesture's handler, and the
/// frame loop is not one — macroquad's `set_fullscreen` is refused on the
/// web. So `platform.js` listens for the tap itself, and one that ends inside
/// the zone toggles fullscreen there. The game sees the same tap and should
/// do nothing with it.
#[cfg(target_arch = "wasm32")]
pub fn fullscreen_zone(zone: Option<[f32; 4]>) {
    let [x0, y0, x1, y1] = zone.unwrap_or([0.0; 4]);
    unsafe { platform_fullscreen_zone(x0, y0, x1, y1) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn fullscreen_zone(_zone: Option<[f32; 4]>) {}

/// Reload the page: the way back for a game whose connection died while the
/// phone was locked or the tab was hidden, when its transport cannot
/// reconnect in place (quad-net's browser WebSocket cannot). Requires
/// `platform.js`, like the rest of this crate.
#[cfg(target_arch = "wasm32")]
pub fn reload_page() {
    unsafe { platform_reload() };
}

/// Native builds have no page to reload: nothing happens.
#[cfg(not(target_arch = "wasm32"))]
pub fn reload_page() {}

/// Read one parameter from the page's query string.
///
/// The miniquad answer to `web_sys::window().location().search()`, which does
/// not exist here: miniquad's `gl.js` carries no wasm-bindgen glue, so the
/// browser is reachable only through the plugin protocol. Requires
/// `platform.js` to be loaded before the wasm — the same requirement the
/// random-bytes route already has.
///
/// `None` when the parameter is absent *or* empty.
#[cfg(target_arch = "wasm32")]
pub fn launch_query(key: &str) -> Option<String> {
    let js = unsafe { platform_query_param(JsObject::string(key)) };
    let mut value = String::new();
    js.to_string(&mut value);
    (!value.is_empty()).then_some(value)
}

/// Native builds have no page and therefore no query string.
#[cfg(not(target_arch = "wasm32"))]
pub fn launch_query(_key: &str) -> Option<String> {
    None
}

/// The page's hostname, without a port: `buy.example.xyz`.
///
/// For a page served under several names that must know which one it was
/// opened as — one bundle answering for several sites. `None` when there is
/// none (a `file:` page).
#[cfg(target_arch = "wasm32")]
pub fn launch_host() -> Option<String> {
    let js = unsafe { platform_location_host() };
    let mut value = String::new();
    js.to_string(&mut value);
    (!value.is_empty()).then_some(value)
}

/// Native builds have no page and therefore no host.
#[cfg(not(target_arch = "wasm32"))]
pub fn launch_host() -> Option<String> {
    None
}

/// The page's path, from its leading `/`: `/aliens`.
///
/// For a page that answers at several paths and must know which one it was
/// opened at.
#[cfg(target_arch = "wasm32")]
pub fn launch_path() -> Option<String> {
    let js = unsafe { platform_location_path() };
    let mut value = String::new();
    js.to_string(&mut value);
    (!value.is_empty()).then_some(value)
}

/// Native builds have no page and therefore no path.
#[cfg(not(target_arch = "wasm32"))]
pub fn launch_path() -> Option<String> {
    None
}

/// Read one value this origin stored on a previous visit.
///
/// The only durable, synchronous store a page has. A plugin call, because
/// miniquad reaches no browser API without one; `localStorage` itself throws
/// in a private window, under disabled site data, or when the quota is full,
/// and this reads all three as "nothing stored".
///
/// For preference-shaped values only — which wallet the reader picked, a
/// locale. Not for secrets: any script on this origin can read it.
#[cfg(target_arch = "wasm32")]
pub fn storage_get(key: &str) -> Option<String> {
    let js = unsafe { platform_storage_get(JsObject::string(key)) };
    let mut value = String::new();
    js.to_string(&mut value);
    (!value.is_empty()).then_some(value)
}

/// Native builds have no origin to remember anything at.
#[cfg(not(target_arch = "wasm32"))]
pub fn storage_get(_key: &str) -> Option<String> {
    None
}

/// Store `value` under `key` for the next visit.
#[cfg(target_arch = "wasm32")]
pub fn storage_set(key: &str, value: &str) {
    unsafe { platform_storage_set(JsObject::string(key), JsObject::string(value)) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn storage_set(_key: &str, _value: &str) {}

/// Forget `key`. A no-op when nothing is stored, or when storage is unavailable.
#[cfg(target_arch = "wasm32")]
pub fn storage_remove(key: &str) {
    unsafe { platform_storage_remove(JsObject::string(key)) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn storage_remove(_key: &str) {}

#[cfg(target_arch = "wasm32")]
fn webcrypto_getrandom(buf: &mut [u8]) -> Result<(), getrandom::Error> {
    if buf.is_empty() {
        return Ok(());
    }
    let js = unsafe { platform_random_bytes(buf.len() as u32) };
    let mut bytes: Vec<u8> = Vec::with_capacity(buf.len());
    js.to_byte_buffer(&mut bytes);
    if bytes.len() != buf.len() {
        // Either the plugin isn't registered (no platform.js in the
        // HTML shell) or the wasm/js boundary corrupted the transfer.
        // Either way, fail loudly — silently zero-filling here would
        // be a subtle catastrophe (predictable "random" keys).
        return Err(getrandom::Error::UNSUPPORTED);
    }
    buf.copy_from_slice(&bytes);
    Ok(())
}

#[cfg(target_arch = "wasm32")]
getrandom::register_custom_getrandom!(webcrypto_getrandom);

/// Plugin handshake for `gl.js` — silences the
/// `Plugin platform is present in JS bundle, but is not used in the
/// rust code` warning. JS-side `version: 3`, must match.
///
/// Bumped 1 → 2 with the `localStorage` calls, 2 → 3 with the page's host
/// and path, and 3 → 4 with fullscreen: a consumer carrying an older
/// `platform.js` then fails the handshake at instantiate rather than calling a
/// function gl.js would stub out.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
extern "C" fn platform_crate_version() -> u32 {
    4
}
