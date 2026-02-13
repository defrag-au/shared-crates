//! Page Header Leptos Component
//!
//! A page header with title, optional subtitle, and action buttons slot.
//!
//! ## Props
//!
//! - `title` - Main page title
//! - `subtitle` - Optional subtitle/description
//! - `actions` - Optional slot for action buttons
//!
//! ## Usage
//!
//! ```ignore
//! <PageHeader
//!     title="My Page Title"
//!     subtitle="A description of this page"
//! />
//!
//! // With action buttons
//! <PageHeader
//!     title="Users"
//!     actions=view! {
//!         <Button variant=ButtonVariant::Primary on_click=add_user>
//!             "Add User"
//!         </Button>
//!     }
//! />
//! ```

use leptos::prelude::*;

/// Page header component
#[component]
pub fn PageHeader(
    /// Main page title
    #[prop(into, optional)]
    title: Option<String>,
    /// Optional subtitle/description
    #[prop(into, optional)]
    subtitle: Option<String>,
    /// Optional action buttons slot
    #[prop(optional)]
    actions: Option<Children>,
) -> impl IntoView {
    view! {
        <header class="ui-page-header">
            <div class="ui-page-header__content">
                {title.map(|t| view! {
                    <h1 class="ui-page-header__title">{t}</h1>
                })}

                {subtitle.map(|s| view! {
                    <p class="ui-page-header__subtitle">{s}</p>
                })}
            </div>

            {actions.map(|a| view! {
                <div class="ui-page-header__actions">
                    {a()}
                </div>
            })}
        </header>
    }
}
