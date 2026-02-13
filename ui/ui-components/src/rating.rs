//! Rating Leptos Component
//!
//! A visual rating display using repeated icons/emojis.
//! Useful for star ratings, difficulty indicators, etc.
//!
//! ## Props
//!
//! - `value` - Current rating value
//! - `max` - Maximum rating (number of icons)
//! - `icon` - Icon/emoji to display (filled)
//! - `empty_icon` - Optional empty icon (unfilled)
//! - `size` - Display size
//! - `color` - Icon color
//!
//! ## Usage
//!
//! ```ignore
//! // Star rating (3 out of 5)
//! <Rating value=3 max=5 icon="⭐" empty_icon="☆" />
//!
//! // Difficulty skulls (just shows filled count)
//! <Rating value=3 max=5 icon="💀" />
//!
//! // Heart rating
//! <Rating value=4 max=5 icon="❤️" empty_icon="🤍" />
//!
//! // Custom color
//! <Rating value=3 max=5 icon="●" empty_icon="○" color="#FFD700" />
//! ```

use leptos::prelude::*;

/// Rating size variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RatingSize {
    Sm,
    #[default]
    Md,
    Lg,
}

impl RatingSize {
    fn class_suffix(&self) -> &'static str {
        match self {
            RatingSize::Sm => "sm",
            RatingSize::Md => "md",
            RatingSize::Lg => "lg",
        }
    }
}

/// Rating component
#[component]
pub fn Rating(
    /// Current rating value
    #[prop(into)]
    value: Signal<u32>,
    /// Maximum rating (total icons)
    #[prop(optional, default = 5)]
    max: u32,
    /// Filled icon/emoji
    #[prop(into, optional, default = "★".into())]
    icon: String,
    /// Empty icon/emoji (if not provided, only filled icons shown)
    #[prop(into, optional)]
    empty_icon: Option<String>,
    /// Display size
    #[prop(optional, default = RatingSize::Md)]
    size: RatingSize,
    /// Icon color
    #[prop(into, optional)]
    color: Option<String>,
    /// Additional class
    #[prop(into, optional)]
    class: Option<String>,
) -> impl IntoView {
    let size_class = format!("ui-rating--{}", size.class_suffix());
    let rating_class = {
        let mut classes = vec!["ui-rating", &size_class];
        if let Some(ref c) = class {
            classes.push(c);
        }
        classes.join(" ")
    };

    let color_style = color.map(|c| format!("color: {c};"));

    view! {
        <div class=rating_class style=color_style>
            {move || {
                let current = value.get().min(max);
                let mut icons = Vec::new();

                // Filled icons
                for _ in 0..current {
                    icons.push(view! {
                        <span class="ui-rating__icon ui-rating__icon--filled">
                            {icon.clone()}
                        </span>
                    });
                }

                // Empty icons (if empty_icon provided)
                if let Some(ref empty) = empty_icon {
                    for _ in current..max {
                        icons.push(view! {
                            <span class="ui-rating__icon ui-rating__icon--empty">
                                {empty.clone()}
                            </span>
                        });
                    }
                }

                icons
            }}
        </div>
    }
}
