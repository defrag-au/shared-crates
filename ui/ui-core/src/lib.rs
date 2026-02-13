//! Framework-agnostic utilities for WASM widgets
//!
//! This crate provides shared functionality that works with any frontend framework
//! (Seed, Leptos, Yew, etc.). It contains no framework-specific code.
//!
//! ## Modules
//!
//! - [`auth`] - Authentication state management
//! - [`color`] - Color utilities (contrast detection, luminance)
//! - [`error`] - Error types with HTTP status handling
//! - [`fetch_state`] - Generic async fetch state management
//! - [`http`] - HTTP helpers using gloo-net
//! - [`runtime`] - Widget initialization (panic hooks, tracing)
//! - [`token`] - JWT token parsing
//! - [`urls`] - URL building utilities

pub mod auth;
pub mod color;
pub mod error;
pub mod fetch_state;
pub mod http;
pub mod runtime;
pub mod token;
pub mod urls;

// Re-export commonly used types
pub use auth::{AuthContext, AuthState};
pub use error::WidgetError;
pub use fetch_state::FetchState;
pub use runtime::{init_widget, init_widget_with_level};
pub use token::{decode_token_claims, WidgetClaims};
