//! Card Leptos Component
//!
//! A generic card container with optional accent bar and slot support.
//!
//! ## Props
//!
//! - `class` - Additional CSS classes
//! - `accent_color` - Optional accent color for top bar
//! - `header` - Optional header slot content
//! - `footer` - Optional footer slot content
//! - `children` - Main body content
//!
//! ## Usage
//!
//! ```ignore
//! <Card accent_color="#4caf50".to_string()>
//!     <p>"Card content goes here"</p>
//! </Card>
//!
//! // With header and footer
//! <Card
//!     header=view! { <h3>"Title"</h3> }
//!     footer=view! { <Button on_click=save>"Save"</Button> }
//! >
//!     <p>"Body content"</p>
//! </Card>
//! ```

use leptos::prelude::*;

/// Generic card container component
#[component]
pub fn Card(
    /// Additional CSS classes
    #[prop(into, optional)]
    class: String,
    /// Accent color for top bar
    #[prop(into, optional)]
    accent_color: Option<String>,
    /// Header slot content
    #[prop(optional)]
    header: Option<Children>,
    /// Footer slot content
    #[prop(optional)]
    footer: Option<Children>,
    /// Main body content
    children: Children,
) -> impl IntoView {
    let card_class = if class.is_empty() {
        "ui-card".to_string()
    } else {
        format!("ui-card {class}")
    };

    view! {
        <div class=card_class>
            {accent_color.map(|color| view! {
                <div class="ui-card__accent" style=format!("background-color: {color}")></div>
            })}

            {header.map(|h| view! {
                <div class="ui-card__header">
                    {h()}
                </div>
            })}

            <div class="ui-card__body">
                {children()}
            </div>

            {footer.map(|f| view! {
                <div class="ui-card__footer">
                    {f()}
                </div>
            })}
        </div>
    }
}
