//! Select Leptos Component
//!
//! A dropdown select input.
//!
//! ## Props
//!
//! - `value` - Currently selected value
//! - `options` - Available options
//! - `on_change` - Callback when selection changes
//! - `placeholder` - Placeholder text when no value selected
//! - `disabled` - Whether select is disabled
//!
//! ## Usage
//!
//! ```ignore
//! let (selected, set_selected) = signal("option1".to_string());
//!
//! <Select
//!     value=selected
//!     options=vec![
//!         SelectOption::new("option1", "Option 1"),
//!         SelectOption::new("option2", "Option 2"),
//!     ]
//!     on_change=Callback::new(move |v| set_selected.set(v))
//! />
//! ```

use leptos::prelude::*;

/// Option for Select component
#[derive(Debug, Clone, PartialEq)]
pub struct SelectOption {
    /// The value (submitted)
    pub value: String,
    /// The label (displayed)
    pub label: String,
    /// Whether this option is disabled
    pub disabled: bool,
}

impl SelectOption {
    /// Create a new option with value and label
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            disabled: false,
        }
    }

    /// Create a disabled option
    pub fn disabled(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            disabled: true,
        }
    }
}

/// Select dropdown component
#[component]
pub fn Select(
    /// Currently selected value
    #[prop(into)]
    value: Signal<String>,
    /// Available options
    #[prop(into)]
    options: Signal<Vec<SelectOption>>,
    /// Callback when selection changes
    #[prop(into)]
    on_change: Callback<String>,
    /// Placeholder text
    #[prop(into, optional)]
    placeholder: Option<String>,
    /// Whether select is disabled
    #[prop(into, optional)]
    disabled: Option<Signal<bool>>,
) -> impl IntoView {
    let handle_change = move |ev: web_sys::Event| {
        let target = event_target::<web_sys::HtmlSelectElement>(&ev);
        on_change.run(target.value());
    };

    let is_disabled = move || disabled.map(|d| d.get()).unwrap_or(false);

    view! {
        <select
            class="ui-select"
            on:change=handle_change
            disabled=is_disabled
            prop:value=move || value.get()
        >
            {placeholder.map(|p| view! {
                <option value="" disabled=true selected=move || value.get().is_empty()>
                    {p}
                </option>
            })}
            {move || options.get().into_iter().map(|opt| {
                let opt_value = opt.value.clone();
                let is_selected = move || value.get() == opt_value;
                view! {
                    <option
                        value=opt.value.clone()
                        disabled=opt.disabled
                        selected=is_selected
                    >
                        {opt.label}
                    </option>
                }
            }).collect_view()}
        </select>
    }
}
