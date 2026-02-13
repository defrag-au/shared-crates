//! Optimistic UI operation tracking
//!
//! Provides types for tracking client-initiated operations through their lifecycle:
//! pending -> progress updates -> success/error
//!
//! This enables optimistic UI where the client can show immediate feedback
//! while the server processes the request.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export OpId for internal use
pub use ui_flow_protocol::OpId;

/// Get current time in milliseconds
#[cfg(all(feature = "web-sys-transport", not(feature = "macroquad")))]
fn now_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(feature = "macroquad")]
fn now_ms() -> f64 {
    miniquad::date::now() * 1000.0
}

#[cfg(not(any(feature = "web-sys-transport", feature = "macroquad")))]
fn now_ms() -> f64 {
    0.0 // Fallback - timing features won't work
}

/// Progress update for an in-flight operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionProgress {
    /// The operation this progress relates to
    pub op_id: OpId,
    /// Progress percentage (0-100), if determinable
    pub percent: Option<u8>,
    /// Human-readable status message
    pub message: Option<String>,
}

/// Error result for a failed operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionError {
    /// The operation that failed
    pub op_id: OpId,
    /// Error code for programmatic handling
    pub code: Option<String>,
    /// Human-readable error message
    pub message: String,
}

/// Tracks a pending operation with its metadata
#[derive(Debug, Clone)]
pub struct PendingOperation<T> {
    /// The operation ID
    pub op_id: OpId,
    /// Application-specific data about this operation
    pub data: T,
    /// Current progress (if any updates received)
    pub progress: Option<ActionProgress>,
    /// When the operation was started (JS timestamp)
    pub started_at: f64,
}

impl<T> PendingOperation<T> {
    /// Create a new pending operation
    pub fn new(data: T) -> Self {
        Self {
            op_id: OpId::new(),
            data,
            progress: None,
            started_at: now_ms(),
        }
    }

    /// Create with a specific OpId (useful for testing)
    pub fn with_id(op_id: OpId, data: T) -> Self {
        Self {
            op_id,
            data,
            progress: None,
            started_at: now_ms(),
        }
    }

    /// Update progress for this operation
    pub fn update_progress(&mut self, progress: ActionProgress) {
        self.progress = Some(progress);
    }

    /// Get elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> f64 {
        now_ms() - self.started_at
    }
}

/// Tracker for multiple pending operations
///
/// Provides a convenient way to manage multiple in-flight operations,
/// correlating progress updates and completions with their original requests.
#[derive(Debug, Default)]
pub struct OperationTracker<T> {
    pending: HashMap<OpId, PendingOperation<T>>,
}

impl<T> OperationTracker<T> {
    /// Create a new empty tracker
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
        }
    }

    /// Start tracking a new operation, returning its OpId
    pub fn start(&mut self, data: T) -> OpId {
        let op = PendingOperation::new(data);
        let op_id = op.op_id;
        self.pending.insert(op_id, op);
        op_id
    }

    /// Start tracking with a specific OpId
    pub fn start_with_id(&mut self, op_id: OpId, data: T) {
        let op = PendingOperation::with_id(op_id, data);
        self.pending.insert(op_id, op);
    }

    /// Update progress for an operation
    ///
    /// Returns true if the operation was found and updated
    pub fn update_progress(&mut self, progress: ActionProgress) -> bool {
        if let Some(op) = self.pending.get_mut(&progress.op_id) {
            op.update_progress(progress);
            true
        } else {
            false
        }
    }

    /// Complete an operation successfully, returning its data
    pub fn complete(&mut self, op_id: OpId) -> Option<T> {
        self.pending.remove(&op_id).map(|op| op.data)
    }

    /// Complete an operation with an error, returning its data
    pub fn fail(&mut self, error: &ActionError) -> Option<T> {
        self.pending.remove(&error.op_id).map(|op| op.data)
    }

    /// Check if an operation is pending
    pub fn is_pending(&self, op_id: OpId) -> bool {
        self.pending.contains_key(&op_id)
    }

    /// Get a reference to a pending operation
    pub fn get(&self, op_id: OpId) -> Option<&PendingOperation<T>> {
        self.pending.get(&op_id)
    }

    /// Get the number of pending operations
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Check if there are no pending operations
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Iterate over all pending operations
    pub fn iter(&self) -> impl Iterator<Item = (&OpId, &PendingOperation<T>)> {
        self.pending.iter()
    }

    /// Remove operations that have been pending longer than the timeout
    pub fn cleanup_stale(&mut self, timeout_ms: f64) -> Vec<T> {
        let now = now_ms();
        let stale: Vec<_> = self
            .pending
            .iter()
            .filter(|(_, op)| now - op.started_at > timeout_ms)
            .map(|(id, _)| *id)
            .collect();

        stale
            .into_iter()
            .filter_map(|id| self.pending.remove(&id).map(|op| op.data))
            .collect()
    }
}
