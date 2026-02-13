//! Framework-agnostic loading orchestrator for WASM widgets
//!
//! ## Identity
//!
//! This crate provides the [`Identity`] enum for cross-platform user identity.
//! Identity is passed from the launcher HTML via localStorage and can be read by
//! any WASM framework (Leptos, Seed, macroquad via quad-storage, etc.).
//!
//! ```ignore
//! use ui_loader::Identity;
//!
//! // In macroquad (using quad-storage):
//! let identity = Identity::from_quad_storage();
//!
//! match identity {
//!     Identity::Anonymous => println!("Playing as guest"),
//!     Identity::Discord { user_id, display_name, .. } => {
//!         println!("Welcome, {}!", display_name.unwrap_or(user_id));
//!     }
//! }
//! ```
//!
//! ## Loading Orchestrator (web feature only)
//!
//! With the `web` feature enabled, this crate also provides a loading orchestrator
//! that runs **before** any UI framework starts. It handles the common widget
//! bootstrap sequence:
//!
//! 1. Show loading screen immediately (direct DOM)
//! 2. Parse URL parameters
//! 3. Validate/decode JWT token
//! 4. Fetch initial data (with progress updates)
//! 5. Handle errors (show error screen)
//! 6. Hand off loaded data to the framework

mod identity;

pub use identity::{Identity, IDENTITY_STORAGE_KEY};

// ============================================================================
// Web feature - LoadingOrchestrator and related types
// ============================================================================

#[cfg(feature = "web")]
mod web;

#[cfg(feature = "web")]
pub use web::*;
