//! ButtonGroup Leptos Component
//!
//! A container for grouping related buttons together.
//!
//! ## Props
//!
//! - `class` - Additional CSS classes
//! - `children` - Button components
//!
//! ## Usage
//!
//! ```ignore
//! <ButtonGroup>
//!     <Button variant=ButtonVariant::Secondary on_click=cancel>"Cancel"</Button>
//!     <Button on_click=save>"Save"</Button>
//! </ButtonGroup>
//! ```

use leptos::prelude::*;

/// Button group container component
#[component]
pub fn ButtonGroup(
    /// Additional CSS classes
    #[prop(into, optional)]
    class: String,
    /// Button components
    children: Children,
) -> impl IntoView {
    let content = children();

    let group_class = if class.is_empty() {
        "ui-button-group".to_string()
    } else {
        format!("ui-button-group {class}")
    };

    view! {
        <div class=group_class role="group">
            {content}
        </div>
    }
}
