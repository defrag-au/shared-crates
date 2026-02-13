//! UserAvatar Leptos Component
//!
//! A circular avatar image with fallback to a default user icon.
//!
//! ## Props
//!
//! - `src` - Image URL (optional)
//! - `alt` - Alt text for the image
//! - `size` - Size variant (Sm, Md, Lg)
//! - `fallback` - Custom fallback content (optional)
//!
//! ## Usage
//!
//! ```ignore
//! // With image
//! <UserAvatar src="https://example.com/avatar.jpg" alt="John Doe" />
//!
//! // Without image (shows fallback icon)
//! <UserAvatar alt="Unknown User" />
//!
//! // Different sizes
//! <UserAvatar src=url size=AvatarSize::Sm />
//! <UserAvatar src=url size=AvatarSize::Md />
//! <UserAvatar src=url size=AvatarSize::Lg />
//!
//! // Custom fallback
//! <UserAvatar fallback="JD" />
//! ```

use leptos::prelude::*;

/// Circular avatar component
#[component]
pub fn UserAvatar(
    /// Image URL (optional)
    #[prop(into, optional)]
    src: Option<String>,
    /// Alt text for the image
    #[prop(into, optional)]
    alt: String,
    /// Size variant
    #[prop(into, optional)]
    size: AvatarSize,
    /// Custom fallback content (initials or emoji)
    #[prop(into, optional)]
    fallback: Option<String>,
) -> impl IntoView {
    let size_class = match size {
        AvatarSize::Sm => "ui-avatar ui-avatar--sm",
        AvatarSize::Md => "ui-avatar ui-avatar--md",
        AvatarSize::Lg => "ui-avatar ui-avatar--lg",
        AvatarSize::Xl => "ui-avatar ui-avatar--xl",
    };

    let fallback_content = fallback.unwrap_or_else(|| "👤".to_string());

    view! {
        <div class=size_class>
            {if let Some(url) = src {
                view! {
                    <img class="ui-avatar__image" src=url alt=alt />
                }.into_any()
            } else {
                view! {
                    <span class="ui-avatar__fallback">{fallback_content}</span>
                }.into_any()
            }}
        </div>
    }
}

/// Avatar size variants
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AvatarSize {
    /// 24px
    Sm,
    /// 40px (default)
    #[default]
    Md,
    /// 56px
    Lg,
    /// 80px
    Xl,
}
