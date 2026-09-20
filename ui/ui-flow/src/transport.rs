//! WebSocket transport abstraction
//!
//! This module provides a trait-based abstraction over WebSocket implementations,
//! allowing ui-flow to work with different backends:
//!
//! - `web-sys` (default): Standard browser WebSocket via wasm-bindgen
//! - `quad-net`: WebSocket for macroquad/miniquad applications
//!
//! # Feature Flags
//!
//! - `web-sys` (default): Use web-sys WebSocket implementation
//! - `quad-net`: Use quad-net WebSocket implementation (for macroquad)

/// Events that can occur on a WebSocket connection
#[derive(Debug, Clone)]
pub enum WebSocketEvent {
    /// Connection opened successfully
    Open,
    /// Binary message received
    Message(Vec<u8>),
    /// Connection closed
    Close { code: u16, reason: String },
    /// Error occurred
    Error(String),
}

/// Trait for WebSocket transport implementations
///
/// Implementations must handle the underlying WebSocket connection and
/// provide a polling-based interface for receiving events.
pub trait WebSocketTransport: Sized {
    /// Error type for this transport
    type Error: std::fmt::Debug + std::fmt::Display;

    /// Create a new connection to the given URL
    fn connect(url: &str) -> Result<Self, Self::Error>;

    /// Send binary data over the connection
    fn send(&self, data: &[u8]) -> Result<(), Self::Error>;

    /// Poll for the next event (non-blocking)
    ///
    /// Returns `None` if no events are available.
    /// This should be called regularly (e.g., each frame) to process incoming messages.
    fn poll(&mut self) -> Option<WebSocketEvent>;

    /// Check if the connection is currently open
    fn is_connected(&self) -> bool;

    /// Close the connection
    fn close(&mut self);

    /// Call `wake` whenever an event is enqueued.
    ///
    /// For a host that only draws when something asks it to. A browser
    /// transport is callback-driven — `onmessage` pushes onto a queue — so
    /// without this a reactive host has no idea a message landed, and the
    /// only way to find out is to poll every frame forever. That is a timer
    /// burning frames to check a queue that a callback already filled, and it
    /// kept an otherwise idle app painting continuously.
    ///
    /// Deliberately a bare `Fn()` and not an `egui::Context`: this crate also
    /// backs a macroquad transport, which cannot link wasm-bindgen at all. The
    /// egui host passes a closure that calls `request_repaint`; a game loop
    /// that draws every frame regardless passes nothing.
    ///
    /// Default is a no-op, so a transport that drives its own loop need not
    /// care.
    fn set_wake(&mut self, _wake: Box<dyn Fn()>) {}
}

// Conditionally compile the appropriate backend
#[cfg(all(feature = "web-sys-transport", not(feature = "macroquad")))]
mod web_sys_transport;

#[cfg(all(feature = "web-sys-transport", not(feature = "macroquad")))]
pub use web_sys_transport::WebSysTransport;

#[cfg(feature = "macroquad")]
mod quad_net_transport;

#[cfg(feature = "macroquad")]
pub use quad_net_transport::QuadNetTransport;

// Type alias for the active transport
#[cfg(all(feature = "web-sys-transport", not(feature = "macroquad")))]
pub type DefaultTransport = WebSysTransport;

#[cfg(feature = "macroquad")]
pub type DefaultTransport = QuadNetTransport;
