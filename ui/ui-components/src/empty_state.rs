//! EmptyState Leptos Component
//!
//! A placeholder for empty content areas with message and optional action.
//!
//! ## Props
//!
//! - `message` - Main message text
//! - `icon` - Optional icon/emoji
//! - `action` - Optional action slot (e.g., button)
//!
//! ## Usage
//!
//! ```ignore
//! <EmptyState message="No items found" icon="📭" />
//!
//! <EmptyState
//!     message="No crew assigned"
//!     icon="👥"
//!     action=view! { <Button on_click=add>"Add Crew"</Button> }
//! />
//! ```

use leptos::prelude::*;

/// Empty state placeholder component
#[component]
pub fn EmptyState(
    /// Main message text
    #[prop(into)]
    message: String,
    /// Optional icon/emoji
    #[prop(into, optional)]
    icon: Option<String>,
    /// Optional action slot
    #[prop(optional)]
    action: Option<Children>,
) -> impl IntoView {
    let action_content = action.map(|a| a());

    view! {
        <div class="ui-empty-state">
            {icon.map(|i| view! {
                <div class="ui-empty-state__icon">{i}</div>
            })}
            <p class="ui-empty-state__message">{message}</p>
            {action_content.map(|a| view! {
                <div class="ui-empty-state__action">{a}</div>
            })}
        </div>
    }
}
