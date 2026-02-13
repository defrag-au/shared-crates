//! Admin Panel Component
//!
//! Provides admin controls for managing the game state.

use leptos::prelude::*;

/// Admin panel component
#[component]
pub fn AdminPanel(
    /// Callback to reset the game
    on_reset: impl Fn() + 'static + Clone + Send + Sync,
) -> impl IntoView {
    let (confirming, set_confirming) = signal(false);
    let on_reset_clone = on_reset.clone();

    view! {
        <div class="admin-panel">
            <h3>"Admin"</h3>

            <Show
                when=move || confirming.get()
                fallback=move || view! {
                    <button class="btn btn-warning" on:click=move |_| set_confirming.set(true)>
                        "Reset Game"
                    </button>
                }
            >
                {
                    let on_reset = on_reset_clone.clone();
                    view! {
                        <div class="admin-confirm">
                            <p>"Reset all game state?"</p>
                            <button class="btn btn-danger" on:click=move |_| {
                                on_reset();
                                set_confirming.set(false);
                            }>
                                "Yes, Reset"
                            </button>
                            <button class="btn btn-secondary" on:click=move |_| set_confirming.set(false)>
                                "Cancel"
                            </button>
                        </div>
                    }
                }
            </Show>
        </div>
    }
}
