//! TextInput Leptos Component
//!
//! A styled text input field with optional label and placeholder.
//!
//! ## Props
//!
//! - `value` - Current input value (Signal)
//! - `on_change` - Callback when value changes
//! - `label` - Optional label text
//! - `placeholder` - Optional placeholder text
//! - `disabled` - Whether input is disabled
//! - `input_type` - Input type (text, email, password, etc.)
//!
//! ## Usage
//!
//! ```ignore
//! let (name, set_name) = signal(String::new());
//!
//! <TextInput
//!     value=name
//!     on_change=move |v| set_name.set(v)
//!     label="Username"
//!     placeholder="Enter your username"
//! />
//! ```

use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Input type variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputType {
    #[default]
    Text,
    Email,
    Password,
    Number,
    Search,
    Tel,
    Url,
}

impl InputType {
    fn as_str(&self) -> &'static str {
        match self {
            InputType::Text => "text",
            InputType::Email => "email",
            InputType::Password => "password",
            InputType::Number => "number",
            InputType::Search => "search",
            InputType::Tel => "tel",
            InputType::Url => "url",
        }
    }
}

/// Text input component
#[component]
pub fn TextInput(
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
    /// Whether input is disabled
    #[prop(into, optional)]
    disabled: Option<Signal<bool>>,
    /// Input type
    #[prop(optional, default = InputType::Text)]
    input_type: InputType,
    /// Additional class
    #[prop(into, optional)]
    class: Option<String>,
) -> impl IntoView {
    let is_disabled = move || disabled.map(|d| d.get()).unwrap_or(false);

    let input_class = move || {
        let mut classes = vec!["ui-text-input__field"];
        if is_disabled() {
            classes.push("ui-text-input__field--disabled");
        }
        if let Some(ref c) = class {
            classes.push(c);
        }
        classes.join(" ")
    };

    let handle_input = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let input: web_sys::HtmlInputElement = target.unchecked_into();
        on_change.run(input.value());
    };

    view! {
        <div class="ui-text-input">
            {label.map(|l| view! {
                <label class="ui-text-input__label">{l}</label>
            })}
            <input
                type=input_type.as_str()
                class=input_class
                value=move || value.get()
                placeholder=placeholder.unwrap_or_default()
                disabled=is_disabled
                on:input=handle_input
            />
        </div>
    }
}
