//! Framework-agnostic widget runtime initialization
//!
//! This module provides initialization functions that work with any frontend framework.
//! Framework-specific context (like Seed's WidgetContext with realtime streams) lives
//! in the framework-specific crates.

use tracing_subscriber::prelude::*;

// Re-export tracing::Level for convenience
pub use tracing::Level;

/// Initialize widget runtime with default settings (TRACE level)
///
/// This sets up:
/// - Panic hooks for better WASM error messages
/// - Tracing/logging to browser console
///
/// Call this once at the start of your widget's initialization.
/// Safe to call multiple times (uses set_once internally).
pub fn init_widget() {
    init_widget_with_level(Level::TRACE);
}

/// Initialize widget runtime with a specific log level
///
/// This sets up:
/// - Panic hooks for better WASM error messages
/// - Tracing/logging to browser console at the specified level
///
/// Call this once at the start of your widget's initialization.
/// Safe to call multiple times (uses set_once/try_init internally).
pub fn init_widget_with_level(level: Level) {
    // Set up panic hooks for better error messages in WASM
    console_error_panic_hook::set_once();

    // Configure tracing - use try_init to avoid panic if already initialized
    let tracing_config = tracing_wasm::WASMLayerConfigBuilder::new()
        .set_max_level(level)
        .build();

    let _ = tracing_subscriber::registry()
        .with(tracing_wasm::WASMLayer::new(tracing_config))
        .try_init();
}

/// Extract a query parameter from the current browser URL
///
/// Returns None if:
/// - Running outside a browser environment
/// - The parameter doesn't exist
/// - There's an error accessing the URL
pub fn get_query_param(name: &str) -> Option<String> {
    let window = web_sys::window()?;
    let location = window.location();
    let search = location.search().ok()?;

    let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
    params.get(name)
}

/// Get the current browser URL's full href
pub fn get_current_url() -> Option<String> {
    let window = web_sys::window()?;
    window.location().href().ok()
}

/// Get the origin (protocol + host) of the current page
pub fn get_origin() -> Option<String> {
    let window = web_sys::window()?;
    window.location().origin().ok()
}

/// Get the pathname of the current URL
pub fn get_pathname() -> Option<String> {
    let window = web_sys::window()?;
    window.location().pathname().ok()
}
