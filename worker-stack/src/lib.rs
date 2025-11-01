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
