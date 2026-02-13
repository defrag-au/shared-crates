//! RoleDots Leptos Component
//!
//! Compact colored dots showing role assignments with tooltip.
//! Useful for smaller layouts where text badges are too large.
//!
//! ## Props
//!
//! - `roles` - Signal with list of roles (id and color)
//! - `show_tooltip` - Whether to show tooltip on hover (default: true)
//!
//! ## Usage
//!
//! ```ignore
//! let roles = vec![
//!     RoleDot { id: "doctor".to_string(), color: Some("#4ade80".to_string()), label: Some("Doctor".to_string()) },
//!     RoleDot { id: "gunner".to_string(), color: Some("#f87171".to_string()), label: Some("Gunner".to_string()) },
//! ];
//!
//! <RoleDots roles=Signal::derive(move || roles.clone()) />
//! ```

use leptos::prelude::*;

/// A single role represented as a dot
#[derive(Debug, Clone, PartialEq)]
pub struct RoleDot {
    /// Role identifier
    pub id: String,
    /// Color for the dot (CSS color value)
    pub color: Option<String>,
    /// Display label for tooltip (uses id if not provided)
    pub label: Option<String>,
}

impl RoleDot {
    /// Create a new role dot
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            color: None,
            label: None,
        }
    }

    /// Set the color
    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Set the label
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Get the display label (falls back to capitalized id)
    pub fn display_label(&self) -> String {
        self.label.clone().unwrap_or_else(|| {
            let mut chars = self.id.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().chain(chars).collect(),
            }
        })
    }
}

/// Compact role dots with optional tooltip
#[component]
pub fn RoleDots(
    /// List of roles to display
    #[prop(into)]
    roles: Signal<Vec<RoleDot>>,
    /// Whether to show tooltip on hover
    #[prop(optional)]
    show_tooltip: Option<bool>,
) -> impl IntoView {
    let show_tooltip = show_tooltip.unwrap_or(true);

    view! {
        {move || {
            let current_roles = roles.get();
            if current_roles.is_empty() {
                return view! { <span></span> }.into_any();
            }

            let tooltip_text = current_roles
                .iter()
                .map(|r| r.display_label())
                .collect::<Vec<_>>()
                .join(", ");

            view! {
                <div class="ui-role-dots">
                    {show_tooltip.then(|| view! {
                        <div class="ui-role-dots__tooltip">{tooltip_text}</div>
                    })}
                    {current_roles.into_iter().map(|role| {
                        let color = role.color.unwrap_or_else(|| "#6c757d".to_string());
                        view! {
                            <span
                                class="ui-role-dots__dot"
                                style:background-color=color
                            ></span>
                        }
                    }).collect_view()}
                </div>
            }.into_any()
        }}
    }
}
