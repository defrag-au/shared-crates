//! InfoGrid Leptos Components
//!
//! A key-value grid for displaying structured information.
//!
//! ## Components
//!
//! - `InfoGrid` - Container for info rows
//! - `InfoRow` - Single key-value row
//!
//! ## Usage
//!
//! ```ignore
//! <InfoGrid>
//!     <InfoRow label="Name" value="Captain Jack" />
//!     <InfoRow label="Rank" value="Admiral" />
//!     <InfoRow label="Ship" value="Black Pearl" />
//! </InfoGrid>
//!
//! // With custom value content
//! <InfoGrid>
//!     <InfoRow label="Status">
//!         <Badge label="Active" color="#28a745" />
//!     </InfoRow>
//! </InfoGrid>
//! ```

use leptos::prelude::*;

/// Info grid container
#[component]
pub fn InfoGrid(
    /// Additional class
    #[prop(into, optional)]
    class: Option<String>,
    /// Grid rows
    children: Children,
) -> impl IntoView {
    let grid_class = if let Some(c) = class {
        format!("ui-info-grid {c}")
    } else {
        "ui-info-grid".to_string()
    };

    view! {
        <div class=grid_class>
            {children()}
        </div>
    }
}

/// Single info row with label and value
#[component]
pub fn InfoRow(
    /// Row label
    #[prop(into)]
    label: String,
    /// Simple string value (use children for complex content)
    #[prop(into, optional)]
    value: Option<String>,
    /// Whether to show as muted/secondary
    #[prop(optional, default = false)]
    muted: bool,
    /// Additional class
    #[prop(into, optional)]
    class: Option<String>,
    /// Custom value content (overrides value prop)
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    let row_class = {
        let mut classes = vec!["ui-info-row"];
        if muted {
            classes.push("ui-info-row--muted");
        }
        if let Some(ref c) = class {
            classes.push(c);
        }
        classes.join(" ")
    };

    view! {
        <div class=row_class>
            <span class="ui-info-row__label">{label}</span>
            <span class="ui-info-row__value">
                {if let Some(children) = children {
                    children().into_any()
                } else {
                    value.unwrap_or_default().into_any()
                }}
            </span>
        </div>
    }
}
