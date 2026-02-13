//! Counter component

use leptos::prelude::*;

/// Counter display with increment/decrement buttons
#[component]
pub fn Counter<F1, F2>(
    /// Current counter value
    value: Signal<u64>,
    /// Called when increment button is clicked
    on_increment: F1,
    /// Called when decrement button is clicked
    on_decrement: F2,
    /// Whether buttons are disabled
    disabled: Signal<bool>,
) -> impl IntoView
where
    F1: Fn() + 'static,
    F2: Fn() + 'static,
{
    view! {
        <div class="card counter">
            <h2>"Counter"</h2>
            <div class="value">{move || value.get()}</div>
            <div class="buttons">
                <button
                    on:click=move |_| on_decrement()
                    disabled=move || disabled.get()
                >
                    "- Decrement"
                </button>
                <button
                    class="primary"
                    on:click=move |_| on_increment()
                    disabled=move || disabled.get()
                >
                    "+ Increment"
                </button>
            </div>
        </div>
    }
}
