//! Badge Leptos Component
//!
//! A small label/badge for categorization or status.
//!
//! ## Props
//!
//! - `label` - Badge text
//! - `color` - Optional CSS color
//! - `variant` - Style variant (Solid, Outline, Subtle)
//!
//! ## Usage
//!
//! ```ignore
//! <Badge label="New" />
//! <Badge label="Captain" color="#ffc107" variant=BadgeVariant::Outline />
//! <Badge label="Active" variant=BadgeVariant::Subtle />
//! ```

use leptos::prelude::*;

/// Badge style variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BadgeVariant {
    /// Solid background color
    #[default]
    Solid,
    /// Outlined with border
    Outline,
    /// Subtle/muted background
    Subtle,
}

impl BadgeVariant {
    fn class_suffix(&self) -> &'static str {
        match self {
            BadgeVariant::Solid => "solid",
            BadgeVariant::Outline => "outline",
            BadgeVariant::Subtle => "subtle",
        }
    }
}

/// Badge size variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BadgeSize {
    /// Extra small - for overlays on small cards
    Xs,
    /// Small
    Sm,
    /// Medium (default)
    #[default]
    Md,
}

impl BadgeSize {
    fn class_suffix(&self) -> &'static str {
        match self {
            BadgeSize::Xs => "xs",
            BadgeSize::Sm => "sm",
            BadgeSize::Md => "md",
        }
    }
}

/// Badge component
#[component]
pub fn Badge(
    /// Badge text
    #[prop(into)]
    label: String,
    /// Optional CSS color
    #[prop(into, optional)]
    color: Option<String>,
    /// Style variant
    #[prop(optional, default = BadgeVariant::Solid)]
    variant: BadgeVariant,
    /// Size variant
    #[prop(optional, default = BadgeSize::Md)]
    size: BadgeSize,
) -> impl IntoView {
    let variant_class = format!("ui-badge--{}", variant.class_suffix());
    let size_class = format!("ui-badge--{}", size.class_suffix());
    let badge_class = format!("ui-badge {variant_class} {size_class}");

    let style = color.map(|c| format!("--badge-color: {c}"));

    view! {
        <span class=badge_class style=style>
            {label}
        </span>
    }
}
