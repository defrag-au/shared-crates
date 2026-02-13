//! Framework-agnostic toast notification system for WASM widgets
//!
//! This crate provides the core toast notification data model and logic,
//! independent of any UI framework. Frameworks (Seed, Leptos, etc.) provide
//! their own rendering.
//!
//! ## Core Types
//!
//! - [`Toast`] - A single toast notification
//! - [`ToastKind`] - The type/severity of the toast (Success, Warning, Error, Info)
//! - [`ToastMsg`] - Messages for toast state management
//! - [`HasToasts`] - Trait for types that contain toast state
//!
//! ## Usage
//!
//! ```ignore
//! use ui_toast::{Toast, ToastKind, ToastMsg, HasToasts};
//! use std::collections::VecDeque;
//!
//! struct MyModel {
//!     toasts: VecDeque<Toast>,
//!     next_toast_id: u32,
//! }
//!
//! impl HasToasts for MyModel {
//!     fn toasts(&self) -> &VecDeque<Toast> { &self.toasts }
//!     fn toasts_mut(&mut self) -> &mut VecDeque<Toast> { &mut self.toasts }
//!     fn next_toast_id(&self) -> u32 { self.next_toast_id }
//!     fn set_next_toast_id(&mut self, id: u32) { self.next_toast_id = id; }
//! }
//!
//! // Add a toast
//! model.add_toast("Success!".to_string(), ToastKind::Success);
//! ```

use std::collections::VecDeque;

/// Toast notification
#[derive(Debug, Clone)]
pub struct Toast {
    /// Unique identifier for this toast
    pub id: u32,
    /// The message to display
    pub message: String,
    /// The type/severity of the toast
    pub kind: ToastKind,
    /// Custom icon override (if None, uses default for kind)
    pub icon: Option<String>,
    /// Timestamp when the toast was created
    pub created_at: f64,
}

/// Toast notification type/severity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Warning,
    Error,
    Info,
}

/// Toast message variants for state management
#[derive(Debug, Clone)]
pub enum ToastMsg {
    /// Show a new toast
    Show(String, ToastKind),
    /// Show a toast with a custom icon
    ShowWithIcon(String, ToastKind, String),
    /// Dismiss a specific toast by ID
    Dismiss(u32),
    /// Clean up expired toasts (called automatically after timeout)
    Cleanup,
}

impl Toast {
    /// Duration in milliseconds before toast auto-dismisses
    pub const DURATION_MS: u32 = 5000;

    /// Create a new toast
    pub fn new(id: u32, message: String, kind: ToastKind) -> Self {
        Self {
            id,
            message,
            kind,
            icon: None,
            created_at: js_sys::Date::now(),
        }
    }

    /// Create a new toast with a custom icon
    pub fn with_icon(id: u32, message: String, kind: ToastKind, icon: String) -> Self {
        Self {
            id,
            message,
            kind,
            icon: Some(icon),
            created_at: js_sys::Date::now(),
        }
    }

    /// Check if this toast has expired
    pub fn is_expired(&self) -> bool {
        js_sys::Date::now() - self.created_at > Self::DURATION_MS as f64
    }
}

impl ToastKind {
    /// Get the default icon for this toast kind
    pub fn icon(&self) -> &'static str {
        match self {
            ToastKind::Success => "\u{2713}", // ✓
            ToastKind::Warning => "\u{26A0}", // ⚠
            ToastKind::Error => "\u{2715}",   // ✕
            ToastKind::Info => "\u{2139}",    // ℹ
        }
    }

    /// Get the CSS modifier class for this toast kind
    pub fn css_class(&self) -> &'static str {
        match self {
            ToastKind::Success => "toast--success",
            ToastKind::Warning => "toast--warning",
            ToastKind::Error => "toast--error",
            ToastKind::Info => "toast--info",
        }
    }
}

/// Trait for types that contain toast state
pub trait HasToasts {
    fn toasts(&self) -> &VecDeque<Toast>;
    fn toasts_mut(&mut self) -> &mut VecDeque<Toast>;
    fn next_toast_id(&self) -> u32;
    fn set_next_toast_id(&mut self, id: u32);

    /// Add a toast and return its ID
    fn add_toast(&mut self, message: String, kind: ToastKind) -> u32 {
        let id = self.next_toast_id();
        self.set_next_toast_id(id.wrapping_add(1));
        self.toasts_mut().push_back(Toast::new(id, message, kind));
        id
    }

    /// Add a toast with a custom icon and return its ID
    fn add_toast_with_icon(&mut self, message: String, kind: ToastKind, icon: String) -> u32 {
        let id = self.next_toast_id();
        self.set_next_toast_id(id.wrapping_add(1));
        self.toasts_mut()
            .push_back(Toast::with_icon(id, message, kind, icon));
        id
    }

    /// Remove expired toasts
    fn cleanup_expired_toasts(&mut self) {
        self.toasts_mut().retain(|t| !t.is_expired());
    }

    /// Dismiss a specific toast by ID
    fn dismiss_toast(&mut self, id: u32) {
        self.toasts_mut().retain(|t| t.id != id);
    }
}

// ============================================================================
// Convenience constructors for ToastMsg
// ============================================================================

/// Create a Show message
pub fn show(message: impl Into<String>, kind: ToastKind) -> ToastMsg {
    ToastMsg::Show(message.into(), kind)
}

/// Create a Show message with a custom icon
pub fn show_with_icon(
    message: impl Into<String>,
    kind: ToastKind,
    icon: impl Into<String>,
) -> ToastMsg {
    ToastMsg::ShowWithIcon(message.into(), kind, icon.into())
}

/// Create a success toast message
pub fn success(message: impl Into<String>) -> ToastMsg {
    ToastMsg::Show(message.into(), ToastKind::Success)
}

/// Create an error toast message
pub fn error(message: impl Into<String>) -> ToastMsg {
    ToastMsg::Show(message.into(), ToastKind::Error)
}

/// Create an info toast message
pub fn info(message: impl Into<String>) -> ToastMsg {
    ToastMsg::Show(message.into(), ToastKind::Info)
}

/// Create a warning toast message
pub fn warning(message: impl Into<String>) -> ToastMsg {
    ToastMsg::Show(message.into(), ToastKind::Warning)
}
