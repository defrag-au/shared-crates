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

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn platform_random_bytes(len: u32) -> JsObject;
}

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
/// rust code` warning. JS-side `version: 1`, must match.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
extern "C" fn platform_crate_version() -> u32 {
    1
}
