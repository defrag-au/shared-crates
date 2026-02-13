//! Modal Navigation Context
//!
//! Provides a context for nested modals to coordinate with a parent ModalStack.
//!
//! When a `Modal` is rendered inside a `ModalStack`, it detects the `ModalNavigation`
//! context and registers itself with the stack. The stack then manages breadcrumbs
//! and navigation while the Modal renders its content inline.

use leptos::prelude::*;
use std::sync::Arc;

/// Unique identifier for a mounted modal view
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ModalViewId(u64);

impl ModalViewId {
    /// Generate a new unique ID
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for ModalViewId {
    fn default() -> Self {
        Self::new()
    }
}

/// Callback to notify a Modal that it's been externally closed
pub type OnExternalClose = Arc<dyn Fn() + Send + Sync>;

/// Information about a mounted modal view (just the title, content renders inline)
#[derive(Clone)]
pub struct MountedView {
    pub id: ModalViewId,
    pub title: String,
    /// Called when the modal is closed externally (e.g., back button)
    pub on_external_close: Option<OnExternalClose>,
}

/// Context for modal navigation - allows nested Modals to coordinate with a parent ModalStack
#[derive(Clone)]
pub struct ModalNavigation {
    /// Mount a new view onto the stack (registers title + close callback), returns the view ID
    mount_fn: Arc<dyn Fn(String, Option<OnExternalClose>) -> ModalViewId + Send + Sync>,
    /// Unmount a view by ID
    unmount_fn: Arc<dyn Fn(ModalViewId) + Send + Sync>,
}

impl ModalNavigation {
    /// Create a new ModalNavigation context
    pub fn new<M, U>(mount: M, unmount: U) -> Self
    where
        M: Fn(String, Option<OnExternalClose>) -> ModalViewId + Send + Sync + 'static,
        U: Fn(ModalViewId) + Send + Sync + 'static,
    {
        Self {
            mount_fn: Arc::new(mount),
            unmount_fn: Arc::new(unmount),
        }
    }

    /// Mount content as a new view in the modal stack
    /// Returns a view ID that can be used to unmount
    ///
    /// `on_external_close` is called if the modal is closed externally (e.g., back button)
    pub fn mount(&self, title: String, on_external_close: Option<OnExternalClose>) -> ModalViewId {
        (self.mount_fn)(title, on_external_close)
    }

    /// Unmount a view by its ID
    pub fn unmount(&self, id: ModalViewId) {
        (self.unmount_fn)(id)
    }
}

/// Try to get the ModalNavigation context if inside a ModalStack
pub fn use_modal_navigation() -> Option<ModalNavigation> {
    use_context::<ModalNavigation>()
}
