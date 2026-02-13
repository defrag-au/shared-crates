//! Tabs Leptos Component
//!
//! A tabbed interface component with tab list and panels.
//!
//! ## Props
//!
//! - `active` - Signal for the currently active tab value
//! - `on_change` - Callback when tab selection changes
//! - `tabs` - Vector of tab definitions (value, label pairs)
//! - `children` - Tab panel content (should use TabPanel components)
//!
//! ## Usage
//!
//! ```ignore
//! let (active_tab, set_active_tab) = signal("overview".to_string());
//!
//! <Tabs
//!     active=active_tab
//!     on_change=Callback::new(move |tab| set_active_tab.set(tab))
//!     tabs=vec![
//!         ("overview".to_string(), "Overview".to_string()),
//!         ("details".to_string(), "Details".to_string()),
//!         ("settings".to_string(), "Settings".to_string()),
//!     ]
//! >
//!     <TabPanel value="overview">"Overview content"</TabPanel>
//!     <TabPanel value="details">"Details content"</TabPanel>
//!     <TabPanel value="settings">"Settings content"</TabPanel>
//! </Tabs>
//! ```

use leptos::prelude::*;

/// Tab definition: (value, label)
pub type TabDef = (String, String);

/// Context for active tab state - provided by Tabs, used by TabPanel
#[derive(Clone)]
pub struct TabsContext {
    pub active: Signal<String>,
}

/// Tabs container component
#[component]
pub fn Tabs(
    /// Currently active tab value
    #[prop(into)]
    active: Signal<String>,
    /// Callback when tab changes
    #[prop(into)]
    on_change: Callback<String>,
    /// Tab definitions: Vec<(value, label)>
    #[prop(into)]
    tabs: Vec<TabDef>,
    /// Tab panel content
    children: Children,
) -> impl IntoView {
    // Provide context for TabPanel children
    provide_context(TabsContext { active });

    // Eagerly render children once (after context is provided)
    let panels_content = children();

    view! {
        <div class="ui-tabs">
            <div class="ui-tabs__list" role="tablist">
                {tabs.into_iter().map(|(value, label)| {
                    let value_for_class = value.clone();
                    let value_for_click = value.clone();

                    view! {
                        <button
                            class=move || {
                                if active.get() == value_for_class {
                                    "ui-tabs__tab ui-tabs__tab--active"
                                } else {
                                    "ui-tabs__tab"
                                }
                            }
                            on:click=move |_| on_change.run(value_for_click.clone())
                            role="tab"
                            aria-selected=move || active.get() == value
                        >
                            {label}
                        </button>
                    }
                }).collect_view()}
            </div>

            <div class="ui-tabs__panels">
                {panels_content}
            </div>
        </div>
    }
}

/// Individual tab panel - visibility controlled via context from parent Tabs
#[component]
pub fn TabPanel(
    /// Value that matches the tab to show this panel
    #[prop(into)]
    value: String,
    /// Panel content
    children: Children,
) -> impl IntoView {
    let ctx = expect_context::<TabsContext>();
    let content = children();

    // Control visibility via style based on context
    let panel_style = move || {
        if ctx.active.get() == value {
            "display: block;"
        } else {
            "display: none;"
        }
    };

    view! {
        <div
            class="ui-tabs__panel"
            style=panel_style
            role="tabpanel"
        >
            {content}
        </div>
    }
}

/// Alias for TabPanel (for backwards compatibility with explicit naming)
pub use TabPanel as TabPanelControlled;
