//! Skeleton Leptos Component
//!
//! Loading placeholder shapes with shimmer animation.
//!
//! ## Props
//!
//! - `variant` - Shape variant (Text, Circle, Rect, Card)
//! - `width` - Custom width (CSS value)
//! - `height` - Custom height (CSS value)
//! - `lines` - Number of text lines (for Text variant)
//!
//! ## Usage
//!
//! ```ignore
//! // Text skeleton (multiple lines)
//! <Skeleton variant=SkeletonVariant::Text lines=3 />
//!
//! // Circle skeleton (avatar placeholder)
//! <Skeleton variant=SkeletonVariant::Circle width="48px" />
//!
//! // Rectangle skeleton
//! <Skeleton variant=SkeletonVariant::Rect width="200px" height="120px" />
//!
//! // Card skeleton
//! <Skeleton variant=SkeletonVariant::Card />
//! ```

use leptos::prelude::*;

/// Skeleton loading placeholder
#[component]
pub fn Skeleton(
    /// Shape variant
    #[prop(into, optional)]
    variant: SkeletonVariant,
    /// Custom width (CSS value)
    #[prop(into, optional)]
    width: Option<String>,
    /// Custom height (CSS value)
    #[prop(into, optional)]
    height: Option<String>,
    /// Number of text lines (for Text variant)
    #[prop(optional)]
    lines: Option<usize>,
) -> impl IntoView {
    match variant {
        SkeletonVariant::Text => {
            let line_count = lines.unwrap_or(1);
            let lines_vec: Vec<usize> = (0..line_count).collect();
            view! {
                <div class="ui-skeleton-text">
                    {lines_vec.into_iter().map(|i| {
                        // Last line is shorter for natural text appearance
                        let is_last = i == line_count - 1 && line_count > 1;
                        let line_width = if is_last { "75%" } else { "100%" };
                        view! {
                            <div
                                class="ui-skeleton ui-skeleton--text"
                                style:width=line_width
                            ></div>
                        }
                    }).collect_view()}
                </div>
            }
            .into_any()
        }
        SkeletonVariant::Circle => {
            let size = width.unwrap_or_else(|| "40px".to_string());
            view! {
                <div
                    class="ui-skeleton ui-skeleton--circle"
                    style:width=size.clone()
                    style:height=size
                ></div>
            }
            .into_any()
        }
        SkeletonVariant::Rect => {
            let w = width.unwrap_or_else(|| "100%".to_string());
            let h = height.unwrap_or_else(|| "100px".to_string());
            view! {
                <div
                    class="ui-skeleton ui-skeleton--rect"
                    style:width=w
                    style:height=h
                ></div>
            }
            .into_any()
        }
        SkeletonVariant::Card => view! {
            <div class="ui-skeleton-card">
                <div class="ui-skeleton ui-skeleton--rect ui-skeleton-card__image"></div>
                <div class="ui-skeleton-card__content">
                    <div class="ui-skeleton ui-skeleton--text" style:width="70%"></div>
                    <div class="ui-skeleton ui-skeleton--text" style:width="90%"></div>
                    <div class="ui-skeleton ui-skeleton--text" style:width="60%"></div>
                </div>
            </div>
        }
        .into_any(),
    }
}

/// Skeleton shape variants
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SkeletonVariant {
    /// Text line placeholder
    #[default]
    Text,
    /// Circular placeholder (for avatars)
    Circle,
    /// Rectangle placeholder
    Rect,
    /// Card placeholder with image and text
    Card,
}
