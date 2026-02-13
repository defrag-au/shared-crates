//! ColorSwatch Leptos Component
//!
//! A small color sample display, useful for showing tier colors, role colors, etc.
//!
//! ## Props
//!
//! - `color` - CSS color value
//! - `label` - Optional label text
//! - `size` - Swatch size (Sm, Md, Lg)
//! - `show_hex` - Whether to show the hex value
//!
//! ## Usage
//!
//! ```ignore
//! // Simple swatch
//! <ColorSwatch color="#FFD700" />
//!
//! // With label
//! <ColorSwatch color="#28a745" label="Success" />
//!
//! // Show hex code
//! <ColorSwatch color="#dc3545" show_hex=true />
//!
//! // Different sizes
//! <ColorSwatch color="#17a2b8" size=SwatchSize::Lg />
//! ```

use leptos::prelude::*;

/// Swatch size variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SwatchSize {
    Sm,
    #[default]
    Md,
    Lg,
}

impl SwatchSize {
    fn class_suffix(&self) -> &'static str {
        match self {
            SwatchSize::Sm => "sm",
            SwatchSize::Md => "md",
            SwatchSize::Lg => "lg",
        }
    }
}

/// Color swatch component
#[component]
pub fn ColorSwatch(
    /// CSS color value
    #[prop(into)]
    color: String,
    /// Optional label
    #[prop(into, optional)]
    label: Option<String>,
    /// Swatch size
    #[prop(optional, default = SwatchSize::Md)]
    size: SwatchSize,
    /// Whether to show hex value
    #[prop(optional, default = false)]
    show_hex: bool,
    /// Additional class
    #[prop(into, optional)]
    class: Option<String>,
) -> impl IntoView {
    let size_class = format!("ui-color-swatch--{}", size.class_suffix());
    let swatch_class = {
        let mut classes = vec!["ui-color-swatch", &size_class];
        if let Some(ref c) = class {
            classes.push(c);
        }
        classes.join(" ")
    };

    let color_style = format!("background-color: {color};");

    view! {
        <div class=swatch_class>
            <span class="ui-color-swatch__color" style=color_style></span>
            {label.map(|l| view! {
                <span class="ui-color-swatch__label">{l}</span>
            })}
            {if show_hex {
                Some(view! {
                    <span class="ui-color-swatch__hex">{color.clone()}</span>
                })
            } else {
                None
            }}
        </div>
    }
}
