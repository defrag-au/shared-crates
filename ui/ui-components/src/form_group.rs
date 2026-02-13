//! FormGroup Leptos Component
//!
//! A wrapper for form fields with label, input, and optional error message.
//!
//! ## Props
//!
//! - `label` - Field label text
//! - `error` - Optional error message
//! - `hint` - Optional hint/help text
//! - `required` - Whether field is required (shows indicator)
//! - `children` - The form control (input, select, textarea, etc.)
//!
//! ## Usage
//!
//! ```ignore
//! <FormGroup label="Email" error=email_error required=true>
//!     <TextInput value=email on_change=set_email />
//! </FormGroup>
//!
//! <FormGroup label="Bio" hint="Optional description">
//!     <Textarea value=bio on_change=set_bio />
//! </FormGroup>
//! ```

use leptos::prelude::*;

/// Form group component
#[component]
pub fn FormGroup(
    /// Field label
    #[prop(into)]
    label: String,
    /// Error message (shows in red below field)
    #[prop(into, optional)]
    error: Option<Signal<Option<String>>>,
    /// Hint text (shows in muted color below field)
    #[prop(into, optional)]
    hint: Option<String>,
    /// Whether field is required
    #[prop(optional, default = false)]
    required: bool,
    /// Additional class
    #[prop(into, optional)]
    class: Option<String>,
    /// Form control child
    children: Children,
) -> impl IntoView {
    let has_error = move || error.map(|e| e.get().is_some()).unwrap_or(false);

    let group_class = move || {
        let mut classes = vec!["ui-form-group"];
        if has_error() {
            classes.push("ui-form-group--error");
        }
        if let Some(ref c) = class {
            classes.push(c);
        }
        classes.join(" ")
    };

    view! {
        <div class=group_class>
            <label class="ui-form-group__label">
                {label}
                {if required {
                    Some(view! { <span class="ui-form-group__required">"*"</span> })
                } else {
                    None
                }}
            </label>
            <div class="ui-form-group__control">
                {children()}
            </div>
            {error.map(|e| view! {
                <div class="ui-form-group__error">
                    {move || e.get()}
                </div>
            })}
            {hint.map(|h| view! {
                <div class="ui-form-group__hint">{h}</div>
            })}
        </div>
    }
}
