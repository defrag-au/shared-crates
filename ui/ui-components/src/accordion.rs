//! Accordion Leptos Component
//!
//! A collapsible accordion container with expandable items.
//!
//! ## Props (Accordion)
//!
//! - `class` - Additional CSS classes
//! - `single` - If true, only one item can be expanded at a time
//! - `children` - AccordionItem components
//!
//! ## Props (AccordionItem)
//!
//! - `title` - Header title text
//! - `icon` - Optional emoji or text icon
//! - `icon_url` - Optional URL for icon image
//! - `badge` - Optional badge text
//! - `expanded` - Signal controlling expanded state
//! - `on_toggle` - Callback when item is toggled
//! - `children` - Content to show when expanded
//!
//! ## Usage
//!
//! ```ignore
//! // Uncontrolled accordion (each item manages own state)
//! <Accordion>
//!     <AccordionItem title="Section 1" icon="🚢">
//!         <p>"Content for section 1"</p>
//!     </AccordionItem>
//!     <AccordionItem title="Section 2" icon="⚔️">
//!         <p>"Content for section 2"</p>
//!     </AccordionItem>
//! </Accordion>
//!
//! // Controlled single-expand accordion
//! let (expanded, set_expanded) = signal(Some(0usize));
//!
//! <Accordion single=true>
//!     <AccordionItem
//!         title="Section 1"
//!         expanded=Signal::derive(move || expanded.get() == Some(0))
//!         on_toggle=move |open| set_expanded.set(if open { Some(0) } else { None })
//!     >
//!         <p>"Content"</p>
//!     </AccordionItem>
//! </Accordion>
//! ```

use leptos::prelude::*;

/// Accordion container component
#[component]
pub fn Accordion(
    /// Additional CSS classes
    #[prop(into, optional)]
    class: String,
    /// Accordion items
    children: Children,
) -> impl IntoView {
    let accordion_class = if class.is_empty() {
        "ui-accordion".to_string()
    } else {
        format!("ui-accordion {class}")
    };

    view! {
        <div class=accordion_class>
            {children()}
        </div>
    }
}

/// Individual accordion item component
#[component]
pub fn AccordionItem(
    /// Header title text
    #[prop(into)]
    title: String,
    /// Optional emoji or text icon
    #[prop(into, optional)]
    icon: Option<String>,
    /// Optional URL for icon image
    #[prop(into, optional)]
    icon_url: Option<String>,
    /// Optional badge text
    #[prop(into, optional)]
    badge: Option<String>,
    /// Controlled expanded state (optional - uses internal state if not provided)
    #[prop(into, optional)]
    expanded: Option<Signal<bool>>,
    /// Callback when item is toggled
    #[prop(into, optional)]
    on_toggle: Option<Callback<bool>>,
    /// Content to show when expanded
    children: Children,
) -> impl IntoView {
    // Use internal state if no controlled state provided
    let (internal_expanded, set_internal_expanded) = signal(false);

    let is_expanded = move || {
        expanded
            .map(|s| s.get())
            .unwrap_or_else(|| internal_expanded.get())
    };

    let toggle = move |_| {
        let new_state = !is_expanded();
        if let Some(cb) = on_toggle {
            cb.run(new_state);
        } else {
            set_internal_expanded.set(new_state);
        }
    };

    let item_class = move || {
        if is_expanded() {
            "ui-accordion__item ui-accordion__item--expanded"
        } else {
            "ui-accordion__item"
        }
    };

    let indicator = move || if is_expanded() { "▼" } else { "▶" };

    // Eagerly evaluate children since we need them in the view
    let content = children();

    view! {
        <div class=item_class>
            <button class="ui-accordion__header" on:click=toggle type="button">
                {if let Some(url) = icon_url {
                    view! {
                        <span class="ui-accordion__icon">
                            <img class="ui-accordion__icon-img" src=url alt="" />
                        </span>
                    }.into_any()
                } else if let Some(emoji) = icon {
                    view! {
                        <span class="ui-accordion__icon">{emoji}</span>
                    }.into_any()
                } else {
                    ().into_any()
                }}
                <span class="ui-accordion__title">{title}</span>
                {badge.map(|b| view! {
                    <span class="ui-accordion__badge">{b}</span>
                })}
                <span class="ui-accordion__indicator">{indicator}</span>
            </button>
            <div
                class="ui-accordion__content"
                style:display=move || if is_expanded() { "block" } else { "none" }
            >
                <div class="ui-accordion__content-inner">
                    {content}
                </div>
            </div>
        </div>
    }
}
