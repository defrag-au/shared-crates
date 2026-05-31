//! macroquad-tui — text-grid primitives for building terminal-style UIs on
//! top of macroquad.
//!
//! Born from `uap-terminal`. Kept as a workspace-local crate so the API
//! can evolve alongside its first real consumer; when a second one
//! arrives (or this stabilises) it can be lifted to a shared location
//! without restructuring callers.
//!
//! Layers:
//!
//! - [`Grid`] — fixed character buffer with cursor + scroll
//! - [`LineEditor`] — single-line editor with history, tab completion
//! - [`TypeQueue`] — typewriter print at configurable rate with
//!   mid-stream speed changes
//! - [`KeyRepeat`] — initial-delay + repeat-rate for held keys (macroquad
//!   doesn't expose key-repeat natively, so consumers wire this in)

pub mod editor;
pub mod grid;
pub mod repeat;
pub mod typer;

pub use editor::{CompletionSource, LineEditor};
pub use grid::{Cell, Grid};
pub use repeat::KeyRepeat;
pub use typer::TypeQueue;
