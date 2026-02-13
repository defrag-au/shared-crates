//! Button Leptos Component
//!
//! A styled button with variants, sizes, and loading state.
//!
//! ## Props
//!
//! - `variant` - Visual style (Primary, Secondary, Danger, Warning, Success, Ghost)
//! - `size` - Button size (Sm, Md, Lg)
//! - `disabled` - Whether button is disabled
//! - `loading` - Whether to show loading spinner
//! - `icon` - Optional icon/emoji before text
//! - `on_click` - Click callback
//! - `children` - Button text/content
//!
//! ## Usage
//!
//! ```ignore
//! <Button on_click=move |_| save()>"Save"</Button>
//!
//! <Button
//!     variant=ButtonVariant::Danger
//!     on_click=move |_| delete()
//!     loading=is_deleting
//! >
//!     "Delete"
//! </Button>
//! ```

use leptos::prelude::*;

/// Button visual variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Danger,
    Warning,
    Success,
    Ghost,
}

impl ButtonVariant {
    fn class_suffix(&self) -> &'static str {
        match self {
            ButtonVariant::Primary => "primary",
            ButtonVariant::Secondary => "secondary",
            ButtonVariant::Danger => "danger",
            ButtonVariant::Warning => "warning",
            ButtonVariant::Success => "success",
            ButtonVariant::Ghost => "ghost",
        }
    }
}

/// Button size variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    Sm,
    #[default]
    Md,
    Lg,
}

impl ButtonSize {
    fn class_suffix(&self) -> &'static str {
        match self {
            ButtonSize::Sm => "sm",
            ButtonSize::Md => "md",
            ButtonSize::Lg => "lg",
        }
    }
}

/// Button component
#[component]
pub fn Button(
    /// Visual variant
    #[prop(optional, default = ButtonVariant::Primary)]
    variant: ButtonVariant,
    /// Size variant
    #[prop(optional, default = ButtonSize::Md)]
    size: ButtonSize,
    /// Whether button is disabled
    #[prop(into, optional)]
    disabled: Option<Signal<bool>>,
    /// Whether to show loading spinner
    #[prop(into, optional)]
    loading: Option<Signal<bool>>,
    /// Optional icon/emoji before text
    #[prop(into, optional)]
    icon: Option<String>,
    /// Click callback
    #[prop(into, optional)]
    on_click: Option<Callback<()>>,
    /// Button content
    children: Children,
) -> impl IntoView {
    let content = children();

    let variant_class = format!("ui-button--{}", variant.class_suffix());
    let size_class = format!("ui-button--{}", size.class_suffix());

    let button_class = move || {
        let mut classes = vec!["ui-button", &variant_class, &size_class];
        if loading.map(|l| l.get()).unwrap_or(false) {
            classes.push("ui-button--loading");
        }
        classes.join(" ")
    };

    let is_disabled = move || {
        disabled.map(|d| d.get()).unwrap_or(false) || loading.map(|l| l.get()).unwrap_or(false)
    };

    let handle_click = move |_| {
        if !is_disabled() {
            if let Some(cb) = on_click {
                cb.run(());
            }
        }
    };

    view! {
        <button
            class=button_class
            disabled=is_disabled
            on:click=handle_click
        >
            {loading.map(|l| view! {
                <span class="ui-button__spinner" style=move || if l.get() { "display: inline-block" } else { "display: none" }>
                    "⟳"
                </span>
            })}
            {icon.map(|i| view! { <span class="ui-button__icon">{i}</span> })}
            <span class="ui-button__text">{content}</span>
        </button>
    }
}
