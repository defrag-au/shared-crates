//! Generic state management for async data fetching
//!
//! This module provides a type-safe way to represent the state of async operations
//! like API calls, avoiding the common pattern of separate `loading: bool` and
//! `data: Option<T>` fields.
//!
//! ## Example
//!
//! ```ignore
//! use ui_core::FetchState;
//!
//! struct Model {
//!     // Instead of: loading: bool, data: Option<UserData>, error: Option<String>
//!     user_data: FetchState<UserData>,
//! }
//!
//! // Use pattern matching to handle states
//! match &model.user_data {
//!     FetchState::Idle => show_fetch_button(),
//!     FetchState::Loading => show_spinner(),
//!     FetchState::Loaded(data) => show_user(data),
//!     FetchState::Failed(err) => show_error(err),
//! }
//! ```

/// Represents the state of an async fetch operation.
/// Use pattern matching to handle the different states.
#[derive(Debug, Clone, PartialEq)]
#[derive(Default)]
pub enum FetchState<T> {
    /// No fetch has been initiated
    #[default]
    Idle,
    /// Fetch is in progress
    Loading,
    /// Fetch completed successfully with data
    Loaded(T),
    /// Fetch failed with an error message
    Failed(String),
}

impl<T> FetchState<T> {
    /// Check if the fetch is in the idle state
    pub fn is_idle(&self) -> bool {
        matches!(self, FetchState::Idle)
    }

    /// Check if the fetch is loading
    pub fn is_loading(&self) -> bool {
        matches!(self, FetchState::Loading)
    }

    /// Check if the fetch completed successfully
    pub fn is_loaded(&self) -> bool {
        matches!(self, FetchState::Loaded(_))
    }

    /// Check if the fetch failed
    pub fn is_failed(&self) -> bool {
        matches!(self, FetchState::Failed(_))
    }

    /// Get the loaded data if available
    pub fn data(&self) -> Option<&T> {
        match self {
            FetchState::Loaded(data) => Some(data),
            _ => None,
        }
    }

    /// Get the error message if failed
    pub fn error(&self) -> Option<&str> {
        match self {
            FetchState::Failed(msg) => Some(msg),
            _ => None,
        }
    }

    /// Map the loaded data to a different type
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> FetchState<U> {
        match self {
            FetchState::Idle => FetchState::Idle,
            FetchState::Loading => FetchState::Loading,
            FetchState::Loaded(data) => FetchState::Loaded(f(data)),
            FetchState::Failed(err) => FetchState::Failed(err),
        }
    }
}

