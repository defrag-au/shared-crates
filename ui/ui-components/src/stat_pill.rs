//! StatPill Leptos Component
//!
//! A colored pill for displaying stats with optional icon.
//!
//! ## Props
//!
//! - `value` - The stat value to display
//! - `icon` - Optional icon/emoji to show before value
//! - `color` - CSS color or preset (success, warning, danger, info, muted)
//! - `size` - Pill size (Sm, Md, Lg)
//!
//! ## Usage
//!
//! ```ignore
//! // With preset color
//! <StatPill value="42" color=StatPillColor::Success />
//!
//! // With custom color and icon
//! <StatPill value="350" icon="⚡" color="#ffc107" />
//!
//! // Reactive value
//! <StatPill value=move || score.get().to_string() />
//! ```

use leptos::prelude::*;

/// Preset color variants for StatPill
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatPillColor {
    #[default]
    Muted,
    Success,
    Warning,
    Danger,
    Info,
}

impl StatPillColor {
    fn css_color(&self) -> &'static str {
        match self {
            StatPillColor::Muted => "#6c757d",
            StatPillColor::Success => "#28a745",
            StatPillColor::Warning => "#ffc107",
            StatPillColor::Danger => "#dc3545",
            StatPillColor::Info => "#17a2b8",
        }
    }
}

/// Size variants for StatPill
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatPillSize {
    Sm,
    #[default]
    Md,
    Lg,
}

impl StatPillSize {
    fn class_suffix(&self) -> &'static str {
        match self {
            StatPillSize::Sm => "sm",
            StatPillSize::Md => "md",
            StatPillSize::Lg => "lg",
        }
    }
}

/// Color input that accepts either a preset or custom CSS color
#[derive(Debug, Clone)]
pub enum StatPillColorInput {
    Preset(StatPillColor),
    Custom(String),
}

impl Default for StatPillColorInput {
    fn default() -> Self {
        StatPillColorInput::Preset(StatPillColor::default())
    }
}

impl From<StatPillColor> for StatPillColorInput {
    fn from(color: StatPillColor) -> Self {
        StatPillColorInput::Preset(color)
    }
}

impl From<&str> for StatPillColorInput {
    fn from(s: &str) -> Self {
        StatPillColorInput::Custom(s.to_string())
    }
}

impl From<String> for StatPillColorInput {
    fn from(s: String) -> Self {
        StatPillColorInput::Custom(s)
    }
}

impl StatPillColorInput {
    fn to_css(&self) -> String {
        match self {
            StatPillColorInput::Preset(p) => p.css_color().to_string(),
            StatPillColorInput::Custom(c) => c.clone(),
        }
    }
}

/// Stat pill component
#[component]
pub fn StatPill(
    /// The value to display
    #[prop(into)]
    value: Signal<String>,
    /// Optional icon/emoji before value
    #[prop(into, optional)]
    icon: Option<String>,
    /// Color (preset or custom CSS color)
    #[prop(into, optional)]
    color: Option<StatPillColorInput>,
    /// Size variant
    #[prop(optional, default = StatPillSize::Md)]
    size: StatPillSize,
) -> impl IntoView {
    let size_class = format!("ui-stat-pill--{}", size.class_suffix());
    let css_color = color.unwrap_or_default().to_css();

    let pill_class = format!("ui-stat-pill {size_class}");

    view! {
        <span
            class=pill_class
            style=format!("--pill-color: {css_color}")
        >
            {icon.map(|i| view! { <span class="ui-stat-pill__icon">{i}</span> })}
            <span class="ui-stat-pill__value">{move || value.get()}</span>
        </span>
    }
}
