//! Textarea Leptos Component
//!
//! A styled multi-line text input.
//!
//! ## Props
//!
//! - `value` - Current textarea value (Signal)
//! - `on_change` - Callback when value changes
//! - `label` - Optional label text
//! - `placeholder` - Optional placeholder text
//! - `disabled` - Whether textarea is disabled
//! - `rows` - Number of visible rows (default: 4)
//!
//! ## Usage
//!
//! ```ignore
//! let (content, set_content) = signal(String::new());
//!
//! <Textarea
//!     value=content
//!     on_change=move |v| set_content.set(v)
//!     label="Description"
//!     placeholder="Enter description..."
//!     rows=6
//! />
//! ```

use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Textarea component
#[component]
pub fn Textarea(
    /// Current value
    #[prop(into)]
    value: Signal<String>,
    /// Change callback
    #[prop(into)]
    on_change: Callback<String>,
    /// Optional label
    #[prop(into, optional)]
    label: Option<String>,
    /// Placeholder text
    #[prop(into, optional)]
    placeholder: Option<String>,
    /// Whether textarea is disabled
    #[prop(into, optional)]
    disabled: Option<Signal<bool>>,
    /// Number of visible rows
    #[prop(optional, default = 4)]
    rows: u32,
    /// Additional class
    #[prop(into, optional)]
    class: Option<String>,
) -> impl IntoView {
    let is_disabled = move || disabled.map(|d| d.get()).unwrap_or(false);

    let textarea_class = move || {
        let mut classes = vec!["ui-textarea__field"];
        if is_disabled() {
            classes.push("ui-textarea__field--disabled");
        }
        if let Some(ref c) = class {
            classes.push(c);
        }
        classes.join(" ")
    };

    let handle_input = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let textarea: web_sys::HtmlTextAreaElement = target.unchecked_into();
        on_change.run(textarea.value());
    };

    view! {
        <div class="ui-textarea">
            {label.map(|l| view! {
                <label class="ui-textarea__label">{l}</label>
            })}
            <textarea
                class=textarea_class
                rows=rows
                placeholder=placeholder.unwrap_or_default()
                disabled=is_disabled
                on:input=handle_input
            >
                {move || value.get()}
            </textarea>
        </div>
    }
}
