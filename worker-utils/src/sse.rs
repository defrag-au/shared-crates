//! Server-Sent Events (SSE) streaming helpers for Cloudflare Workers.
//!
//! Provides a simple API for streaming SSE events from a worker handler.
//!
//! # Example
//!
//! ```rust,ignore
//! use worker_utils::sse::{SseStream, sse_response};
//!
//! let (stream, response) = sse_response()?;
//!
//! wasm_bindgen_futures::spawn_local(async move {
//!     stream.send_event("status", &json!({"region": "grid_0_0"}));
//!     stream.send_event("status", &json!({"region": "grid_0_1"}));
//!     stream.send_event("done", &json!({"total": 2}));
//!     stream.close();
//! });
//!
//! Ok(response)
//! ```

use futures_channel::mpsc;
use futures_util::StreamExt;
use serde::Serialize;
use worker_stack::worker::{Error, Response, Result};

/// A sender handle for writing SSE events to a streaming response.
pub struct SseStream {
    sender: mpsc::UnboundedSender<String>,
}

impl SseStream {
    /// Send an SSE event with the given event type and JSON-serializable data.
    ///
    /// Formats as: `event: {event_type}\ndata: {json}\n\n`
    pub fn send_event<T: Serialize>(&self, event_type: &str, data: &T) {
        if let Ok(json) = serde_json::to_string(data) {
            let msg = format!("event: {event_type}\ndata: {json}\n\n");
            let _ = self.sender.unbounded_send(msg);
        }
    }

    /// Send a raw SSE-formatted string (must include trailing `\n\n`).
    pub fn send_raw(&self, message: String) {
        let _ = self.sender.unbounded_send(message);
    }

    /// Close the stream. The response will finish after all buffered events are flushed.
    pub fn close(self) {
        self.sender.close_channel();
    }
}

/// Create an SSE streaming response and a sender handle.
///
/// Returns `(SseStream, Response)`. Use the `SseStream` to send events,
/// and return the `Response` from your handler. The stream must be used
/// inside a `wasm_bindgen_futures::spawn_local` block since the response
/// is returned immediately.
pub fn sse_response() -> Result<(SseStream, Response)> {
    let (sender, receiver) = mpsc::unbounded();

    let mut response = Response::from_stream(receiver.map(Ok::<_, Error>))?;

    response
        .headers_mut()
        .set("Content-Type", "text/event-stream; charset=utf-8")?;
    response
        .headers_mut()
        .set("Cache-Control", "no-cache, no-store, must-revalidate")?;
    response.headers_mut().set("Connection", "keep-alive")?;

    let stream = SseStream { sender };
    Ok((stream, response))
}
