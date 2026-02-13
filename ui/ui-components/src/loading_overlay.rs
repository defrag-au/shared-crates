//! LoadingOverlay Leptos Component
//!
//! A full-screen loading overlay with spinner and optional message.
//!
//! ## Props
//!
//! - `loading` - Signal controlling visibility
//! - `message` - Optional loading message
//! - `children` - Content behind the overlay (optional)
//!
//! ## Usage
//!
//! ```ignore
//! // Standalone loading overlay
//! <LoadingOverlay loading=is_loading message="Loading data..." />
//!
//! // Wrapping content
//! <LoadingOverlay loading=is_loading message="Saving...">
//!     <div>"Your content here"</div>
//! </LoadingOverlay>
//! ```

use leptos::prelude::*;

/// Full-screen loading overlay with spinner
#[component]
pub fn LoadingOverlay(
    /// Signal controlling visibility
    #[prop(into)]
    loading: Signal<bool>,
    /// Optional loading message
    #[prop(into, optional)]
    message: Option<String>,
    /// Content behind the overlay (optional)
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    let message = message.unwrap_or_else(|| "Loading...".to_string());

    view! {
        {children.map(|c| c())}
        <Show when=move || loading.get()>
            <div class="ui-loading-overlay">
                <div class="ui-loading-overlay__content">
                    <div class="ui-loading-overlay__spinner"></div>
                    <p class="ui-loading-overlay__message">{message.clone()}</p>
                </div>
            </div>
        </Show>
    }
}

/// Inline spinner component for use within content
#[component]
pub fn Spinner(
    /// Size variant
    #[prop(into, optional)]
    size: SpinnerSize,
) -> impl IntoView {
    let class = match size {
        SpinnerSize::Sm => "ui-spinner ui-spinner--sm",
        SpinnerSize::Md => "ui-spinner ui-spinner--md",
        SpinnerSize::Lg => "ui-spinner ui-spinner--lg",
    };

    view! {
        <div class=class></div>
    }
}

/// Spinner size variants
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SpinnerSize {
    Sm,
    #[default]
    Md,
    Lg,
}
