//! macroquad-tui — text-grid primitives for building terminal-style
//! and arcade-style UIs on top of macroquad.
//!
//! Born from `uap-terminal`. Designed for general reuse across any
//! macroquad app that wants a character-grid frontend; the scene
//! abstraction makes it easy to launch arcade-style games on top of
//! a terminal session.
//!
//! Layers:
//!
//! - [`Grid`] — character buffer with cursor + scrollback viewport
//! - [`Cell`] — per-cell `(ch, fg, bg, attrs)`; attributes cover bold /
//!   dim / blink / inverse
//! - [`palette`] — canonical 16-colour EGA palette for game-style
//!   rendering
//! - [`LineEditor`] — single-line editor with history, tab completion
//! - [`TypeQueue`] — typewriter print at configurable rate with
//!   mid-stream speed changes
//! - [`KeyRepeat`] — initial-delay + repeat-rate for held keys
//!   (macroquad doesn't expose key-repeat natively, so consumers wire
//!   this in)
//! - [`scene`] — stackable [`Scene`] trait + [`SceneStack`] for
//!   launching games / overlays on top of a base scene; fixed-step
//!   logic via [`FixedStep`]

pub mod editor;
pub mod grid;
pub mod palette;
pub mod render;
pub mod repeat;
pub mod scene;
pub mod typer;

pub use editor::{CompletionSource, LineEditor};
pub use grid::{Cell, CellAttrs, Grid};
pub use render::{paint_cell, paint_grid, GridMetrics};
pub use repeat::KeyRepeat;
pub use scene::{FixedStep, Scene, SceneCtx, SceneInput, SceneOutcome, SceneStack};
pub use typer::TypeQueue;
