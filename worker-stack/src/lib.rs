//! Facade crate for Cloudflare Workers ecosystem
//!
//! This crate provides a single entry point for all worker-related dependencies
//! with pinned versions to ensure compatibility across the entire ecosystem.
//!
//! ## Usage Pattern
//!
//! ### For Libraries (No Macros)
//!
//! If your crate only uses worker types but doesn't use `#[event]` or `#[durable_object]` macros:
//!
//! ```toml
//! [dependencies]
//! worker_stack = { workspace = true }
//! ```
//!
//! ```rust
//! use worker_stack::worker::{Env, Result};
//!
//! pub async fn my_function(env: &Env) -> Result<String> {
//!     // Your code here
//! }
//! ```
//!
//! ### For Workers (With Macros)
//!
//! If your crate uses `#[event]` or `#[durable_object]` macros, you **must** include both:
//!
//! ```toml
//! [dependencies]
//! worker_stack = { workspace = true }
//! worker = { workspace = true }  # Required for macro expansion
//! ```
//!
//! ```rust
//! use worker_stack::worker::*;
//!
//! #[event(fetch)]
//! async fn main(req: Request, env: Env, ctx: Context) -> Result<Response> {
//!     Response::ok("Hello World")
//! }
//! ```
//!
//! ### Why Both Dependencies?
//!
//! The `#[event]` and `#[durable_object]` macros generate code that references the `worker`
//! crate using absolute paths like `::worker::Response`. These macros are procedural and
//! their generated code is fixed at compile time, so the `worker` crate **must exist**
//! in your dependency tree.
//!
//! - `worker_stack`: Provides re-exports and version pinning for imports
//! - `worker`: Required for macro expansion (must match version in worker_stack)
//!
//! See: https://github.com/cloudflare/workers-rs/blob/main/worker-macros/src/event.rs
//!
//! ## Compatible Tools
//!
//! - worker-build: 0.1.11
//!
//! Install with: `cargo install worker-build --version 0.1.11 --locked`

// Re-export all worker ecosystem crates
pub use js_sys;
pub use serde_wasm_bindgen;
pub use wasm_bindgen;
pub use wasm_bindgen_futures;
pub use wasm_bindgen_macro;
pub use web_sys;
pub use worker;

// Re-export worker attribute macros at crate root for ergonomic use
// This allows: #[worker_stack::event(fetch)] or use worker_stack::event; #[event(fetch)]
pub use worker::{durable_object, event};

/// Prelude module that imports commonly used items from the worker ecosystem
pub mod prelude {
    // Worker core types and traits
    pub use worker::{
        console_debug, console_error, console_log, console_warn, durable_object, event, Bucket,
        Context, Date, DateInit, Delay, Env, Error, Request, Response, Result, RouteContext,
        Router,
    };

    // Queue support
    pub use worker::{Message, MessageBatch, Queue};

    // D1 database support
    pub use worker::d1::{D1Database, D1PreparedStatement, D1Result};

    // KV store support
    pub use worker::kv::{KvError, KvStore};

    // Durable Objects support
    pub use worker::{ObjectNamespace, State as DurableObjectState};

    // wasm-bindgen essentials
    pub use wasm_bindgen::prelude::*;
    pub use wasm_bindgen::JsCast;
    pub use wasm_bindgen_futures::JsFuture;

    // JavaScript interop
    pub use js_sys::{Array, Object, Promise, Reflect};
    pub use web_sys::console;

    // Serialization for WASM
    pub use serde_wasm_bindgen::{from_value, to_value};
}
