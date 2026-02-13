//! Toast Notification Component
//!
//! A toast notification system for displaying temporary messages.
//!
//! ## Usage
//!
//! ```ignore
//! use ui_components::{ToastProvider, ToastContainer, use_toasts, Toast};
//!
//! #[component]
//! fn App() -> impl IntoView {
//!     view! {
//!         <ToastProvider>
//!             <MyContent />
//!             <ToastContainer />
//!         </ToastProvider>
//!     }
//! }
//!
//! #[component]
//! fn MyContent() -> impl IntoView {
//!     let toasts = use_toasts();
//!
//!     view! {
//!         <button on:click=move |_| toasts.show(Toast::success("Saved!"))>
//!             "Save"
//!         </button>
//!         <button on:click=move |_| toasts.show(Toast::error("Something went wrong"))>
//!             "Fail"
//!         </button>
//!     }
//! }
//! ```

use leptos::prelude::*;
use std::collections::VecDeque;

/// Toast notification kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastKind {
    /// Success notification (green)
    Success,
    /// Warning notification (yellow/amber)
    Warning,
    /// Error notification (red)
    Error,
    /// Informational notification (blue)
    #[default]
    Info,
}

impl ToastKind {
    fn class_suffix(&self) -> &'static str {
        match self {
            ToastKind::Success => "success",
            ToastKind::Warning => "warning",
            ToastKind::Error => "error",
            ToastKind::Info => "info",
        }
    }

    fn default_icon(&self) -> &'static str {
        match self {
            ToastKind::Success => "\u{2713}", // checkmark
            ToastKind::Warning => "\u{26A0}", // warning sign
            ToastKind::Error => "\u{2717}",   // X mark
            ToastKind::Info => "\u{2139}",    // info symbol
        }
    }
}

/// Default duration for toast notifications (3 seconds)
pub const DEFAULT_TOAST_DURATION_MS: u32 = 3000;

/// A toast notification
#[derive(Debug, Clone)]
pub struct Toast {
    /// Unique identifier
    pub id: u32,
    /// Notification type
    pub kind: ToastKind,
    /// Message text
    pub message: String,
    /// Optional custom icon (emoji or character)
    pub icon: Option<String>,
    /// Duration in milliseconds before auto-dismiss
    pub duration_ms: u32,
}

impl Toast {
    /// Create a new toast
    fn new(kind: ToastKind, message: impl Into<String>) -> Self {
        Self {
            id: 0, // Set by ToastContext
            kind,
            message: message.into(),
            icon: None,
            duration_ms: DEFAULT_TOAST_DURATION_MS,
        }
    }

    /// Create a success toast
    pub fn success(message: impl Into<String>) -> Self {
        Self::new(ToastKind::Success, message)
    }

    /// Create an error toast
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(ToastKind::Error, message)
    }

    /// Create a warning toast
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(ToastKind::Warning, message)
    }

    /// Create an info toast
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(ToastKind::Info, message)
    }

    /// Set a custom icon
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Set custom duration in milliseconds
    pub fn with_duration_ms(mut self, duration_ms: u32) -> Self {
        self.duration_ms = duration_ms;
        self
    }

    /// Get the icon to display (custom or default based on kind)
    pub fn display_icon(&self) -> &str {
        self.icon
            .as_deref()
            .unwrap_or_else(|| self.kind.default_icon())
    }
}

/// Context for managing toast notifications
#[derive(Clone)]
pub struct ToastContext {
    toasts: RwSignal<VecDeque<Toast>>,
    next_id: RwSignal<u32>,
}

impl ToastContext {
    /// Create a new toast context
    pub fn new() -> Self {
        Self {
            toasts: RwSignal::new(VecDeque::new()),
            next_id: RwSignal::new(0),
        }
    }

    /// Show a toast notification
    pub fn show(&self, mut toast: Toast) {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        toast.id = id;

        let duration_ms = toast.duration_ms;
        self.toasts.update(|t| t.push_back(toast));

        // Schedule removal
        let toasts = self.toasts;
        set_timeout(
            move || {
                toasts.update(|t| {
                    t.retain(|toast| toast.id != id);
                });
            },
            std::time::Duration::from_millis(duration_ms as u64),
        );
    }

    /// Dismiss a specific toast by ID
    pub fn dismiss(&self, id: u32) {
        self.toasts.update(|t| {
            t.retain(|toast| toast.id != id);
        });
    }

    /// Get the current toasts signal (for rendering)
    pub fn toasts(&self) -> RwSignal<VecDeque<Toast>> {
        self.toasts
    }

    /// Dismiss all toasts
    pub fn clear(&self) {
        self.toasts.update(|t| t.clear());
    }
}

impl Default for ToastContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the toast context from the current scope
///
/// # Panics
///
/// Panics if called outside of a `ToastProvider`
pub fn use_toasts() -> ToastContext {
    expect_context::<ToastContext>()
}

/// Try to get the toast context, returning None if not in a ToastProvider
pub fn try_use_toasts() -> Option<ToastContext> {
    use_context::<ToastContext>()
}

/// Provider component for toast notifications
///
/// Wrap your app (or a section of it) with this to enable toast notifications.
///
/// # Example
///
/// ```ignore
/// <ToastProvider>
///     <App />
///     <ToastContainer />
/// </ToastProvider>
/// ```
#[component]
pub fn ToastProvider(children: Children) -> impl IntoView {
    provide_context(ToastContext::new());
    children()
}

/// Container component that renders toast notifications
///
/// Place this component once inside a `ToastProvider`, typically at the end
/// of your layout so toasts appear above other content.
///
/// # Example
///
/// ```ignore
/// <ToastProvider>
///     <MainLayout />
///     <ToastContainer />
/// </ToastProvider>
/// ```
#[component]
pub fn ToastContainer(
    /// Optional position class override (default: top-right)
    #[prop(into, optional)]
    position: Option<String>,
) -> impl IntoView {
    let ctx = use_toasts();
    let toasts = ctx.toasts();

    let position_class = position.unwrap_or_else(|| "ui-toast-container--top-right".to_string());
    let container_class = format!("ui-toast-container {position_class}");

    view! {
        <div class=container_class>
            <For
                each={move || toasts.get().into_iter().collect::<Vec<_>>()}
                key={|t| t.id}
                let:toast
            >
                <ToastItem toast=toast />
            </For>
        </div>
    }
}

/// Individual toast item component
#[component]
fn ToastItem(toast: Toast) -> impl IntoView {
    let ctx = use_toasts();
    let id = toast.id;
    let kind_class = format!("ui-toast--{}", toast.kind.class_suffix());
    let toast_class = format!("ui-toast {kind_class}");
    let icon = toast.display_icon().to_string();
    let message = toast.message.clone();

    view! {
        <div class=toast_class role="alert">
            <span class="ui-toast__icon">{icon}</span>
            <span class="ui-toast__message">{message}</span>
            <button
                class="ui-toast__dismiss"
                on:click=move |_| ctx.dismiss(id)
                aria-label="Dismiss"
            >
                "\u{2715}"
            </button>
        </div>
    }
}
