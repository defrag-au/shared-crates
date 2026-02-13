//! Alert Leptos Component
//!
//! A dismissible alert/message box for feedback.
//!
//! ## Props
//!
//! - `variant` - Visual style (Success, Warning, Error, Info)
//! - `title` - Optional title text
//! - `dismissible` - Whether to show close button
//! - `on_dismiss` - Callback when dismissed
//! - `children` - Alert message content
//!
//! ## Usage
//!
//! ```ignore
//! <Alert variant=AlertVariant::Success>
//!     "Operation completed successfully!"
//! </Alert>
//!
//! <Alert
//!     variant=AlertVariant::Error
//!     title="Error"
//!     dismissible=true
//!     on_dismiss=move |_| clear_error()
//! >
//!     "Something went wrong. Please try again."
//! </Alert>
//! ```

use leptos::prelude::*;

/// Alert visual variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlertVariant {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl AlertVariant {
    fn class_suffix(&self) -> &'static str {
        match self {
            AlertVariant::Info => "info",
            AlertVariant::Success => "success",
            AlertVariant::Warning => "warning",
            AlertVariant::Error => "error",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            AlertVariant::Info => "ℹ",
            AlertVariant::Success => "✓",
            AlertVariant::Warning => "⚠",
            AlertVariant::Error => "✕",
        }
    }
}

/// Alert component
#[component]
pub fn Alert(
    /// Visual variant
    #[prop(optional, default = AlertVariant::Info)]
    variant: AlertVariant,
    /// Optional title
    #[prop(into, optional)]
    title: Option<String>,
    /// Whether alert can be dismissed
    #[prop(optional, default = false)]
    dismissible: bool,
    /// Dismiss callback
    #[prop(into, optional)]
    on_dismiss: Option<Callback<()>>,
    /// Additional class
    #[prop(into, optional)]
    class: Option<String>,
    /// Alert content
    children: Children,
) -> impl IntoView {
    let (visible, set_visible) = signal(true);

    let variant_class = format!("ui-alert--{}", variant.class_suffix());
    let alert_class = move || {
        let mut classes = vec!["ui-alert", &variant_class];
        if let Some(ref c) = class {
            classes.push(c);
        }
        if !visible.get() {
            classes.push("ui-alert--hidden");
        }
        classes.join(" ")
    };

    let handle_dismiss = move |_| {
        set_visible.set(false);
        if let Some(cb) = on_dismiss {
            cb.run(());
        }
    };

    // Render children once
    let content = children();

    view! {
        <div class=alert_class>
            <span class="ui-alert__icon">{variant.icon()}</span>
            <div class="ui-alert__content">
                {title.map(|t| view! {
                    <div class="ui-alert__title">{t}</div>
                })}
                <div class="ui-alert__message">
                    {content}
                </div>
            </div>
            {if dismissible {
                Some(view! {
                    <button
                        class="ui-alert__close"
                        on:click=handle_dismiss
                        aria-label="Dismiss"
                    >
                        "×"
                    </button>
                })
            } else {
                None
            }}
        </div>
    }
}
