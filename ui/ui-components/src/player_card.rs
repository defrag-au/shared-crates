//! PlayerCard Leptos Component
//!
//! A card component for displaying player/user information with avatar,
//! optional hero image, and action buttons.
//!
//! ## Props
//!
//! - `name` - Player display name
//! - `subtitle` - Optional subtitle (entity name, role, etc.)
//! - `badge` - Optional badge text (tier, rank, etc.)
//! - `avatar_url` - Optional avatar image URL
//! - `hero_url` - Optional hero/banner image URL
//! - `meta` - Optional metadata items
//! - `on_click` - Optional click handler for the card
//! - `actions` - Optional action buttons slot
//!
//! ## Usage
//!
//! ```ignore
//! <PlayerCard
//!     name="Captain Jack"
//!     subtitle="The Black Pearl"
//!     badge="Admiral"
//!     avatar_url="https://..."
//!     hero_url="https://..."
//!     meta=vec!["Position: (10, 5)", "3 intents"]
//! >
//!     <Button on_click=details>"Details"</Button>
//!     <Button on_click=render variant=ButtonVariant::Secondary>"Render"</Button>
//! </PlayerCard>
//! ```

use leptos::prelude::*;

/// Player/user card component
#[component]
pub fn PlayerCard(
    /// Player display name
    #[prop(into)]
    name: String,
    /// Optional subtitle (entity name, role, etc.)
    #[prop(into, optional)]
    subtitle: Option<String>,
    /// Optional badge text (tier, rank, etc.)
    #[prop(into, optional)]
    badge: Option<String>,
    /// Optional avatar image URL
    #[prop(into, optional)]
    avatar_url: Option<String>,
    /// Optional hero/banner image URL
    #[prop(into, optional)]
    hero_url: Option<String>,
    /// Optional metadata items
    #[prop(into, optional)]
    meta: Option<Vec<String>>,
    /// Optional click handler
    #[prop(into, optional)]
    on_click: Option<Callback<()>>,
    /// Action buttons slot
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    let card_class = if on_click.is_some() {
        "ui-player-card ui-player-card--clickable"
    } else {
        "ui-player-card"
    };

    let handle_click = move |_| {
        if let Some(cb) = on_click {
            cb.run(());
        }
    };

    view! {
        <div class=card_class on:click=handle_click>
            // Hero/banner image section
            <div class="ui-player-card__image">
                {hero_url.clone().map(|url| view! {
                    <img class="ui-player-card__hero" src=url alt="" />
                })}

                // Avatar overlay
                <div class="ui-player-card__avatar">
                    {if let Some(url) = avatar_url {
                        view! {
                            <img src=url alt="" />
                        }.into_any()
                    } else {
                        view! {
                            <span class="ui-player-card__avatar-fallback">"👤"</span>
                        }.into_any()
                    }}
                </div>

                // Name overlay on image
                <div class="ui-player-card__name-overlay">
                    <h4 class="ui-player-card__name">{name}</h4>
                </div>
            </div>

            // Content section
            <div class="ui-player-card__content">
                // Header: subtitle + badge
                {(subtitle.is_some() || badge.is_some()).then(|| view! {
                    <div class="ui-player-card__header">
                        {subtitle.map(|s| view! {
                            <p class="ui-player-card__subtitle">{s}</p>
                        })}
                        {badge.map(|b| view! {
                            <span class="ui-player-card__badge">{b}</span>
                        })}
                    </div>
                })}

                // Meta information
                {meta.filter(|m| !m.is_empty()).map(|items| view! {
                    <div class="ui-player-card__meta">
                        {items.into_iter().enumerate().map(|(i, item)| view! {
                            {(i > 0).then(|| view! { <span class="ui-player-card__meta-sep">"•"</span> })}
                            <span>{item}</span>
                        }).collect_view()}
                    </div>
                })}

                // Actions slot
                {children.map(|c| view! {
                    <div class="ui-player-card__actions">
                        {c()}
                    </div>
                })}
            </div>
        </div>
    }
}
