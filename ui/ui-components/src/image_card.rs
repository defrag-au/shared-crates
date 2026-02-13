//! Image Card Leptos Component
//!
//! A basic card displaying an image with optional name overlay and accent color.
//! Shows a skeleton loading state while the image is being fetched.
//!
//! ## Props
//!
//! - `image_url` - URL for the image
//! - `name` - Display name (shown in overlay and as title tooltip)
//! - `size` - Card size: Xs (80px), Sm (120px), Md (240px), Lg (400px), Xl (800px)
//! - `accent_color` - Optional accent/tier color for top bar
//! - `is_static` - If true, card is non-interactive (no hover effect)
//! - `show_name` - If true, show name overlay
//! - `show_skeleton` - If true, shows skeleton while image loads (default: true)
//! - `on_click` - Callback when card is clicked (if not static)
//! - `on_load` - Callback when image has loaded
//!
//! ## Usage
//!
//! ```ignore
//! <ImageCard
//!     image_url="https://example.com/image.png"
//!     name="My Image"
//!     size=CardSize::Md
//!     show_name=true
//!     on_click=move |_| { log!("clicked"); }
//! />
//! ```

use leptos::prelude::*;
use phf::phf_map;

/// Card size variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardSize {
    /// Auto - fills container width, maintains square aspect ratio
    Auto,
    /// 80px - Extra small, for dense grids
    Xs,
    /// 120px - Small, default size
    #[default]
    Sm,
    /// 240px - Medium, good for featured items
    Md,
    /// 400px - Large, detailed view
    Lg,
    /// 800px - Extra large, hero/spotlight
    Xl,
}

impl CardSize {
    /// Get the pixel width for this size (None for Auto)
    pub fn pixels(&self) -> Option<u16> {
        match self {
            CardSize::Auto => None,
            CardSize::Xs => Some(80),
            CardSize::Sm => Some(120),
            CardSize::Md => Some(240),
            CardSize::Lg => Some(400),
            CardSize::Xl => Some(800),
        }
    }

    /// Get the CSS class suffix for this size
    pub fn class_suffix(&self) -> &'static str {
        match self {
            CardSize::Auto => "auto",
            CardSize::Xs => "xs",
            CardSize::Sm => "sm",
            CardSize::Md => "md",
            CardSize::Lg => "lg",
            CardSize::Xl => "xl",
        }
    }
}

/// Map from attribute string to CardSize variant
static CARD_SIZE_MAP: phf::Map<&'static str, CardSize> = phf_map! {
    "auto" => CardSize::Auto,
    "xs" => CardSize::Xs,
    "extra-small" => CardSize::Xs,
    "sm" => CardSize::Sm,
    "small" => CardSize::Sm,
    "md" => CardSize::Md,
    "medium" => CardSize::Md,
    "lg" => CardSize::Lg,
    "large" => CardSize::Lg,
    "xl" => CardSize::Xl,
    "extra-large" => CardSize::Xl,
};

/// Parse size string to CardSize
pub fn parse_card_size(s: &str) -> CardSize {
    CARD_SIZE_MAP
        .get(s.to_lowercase().as_str())
        .copied()
        .unwrap_or_default()
}

/// Image card component
#[component]
pub fn ImageCard(
    /// URL for the image
    #[prop(into, optional)]
    image_url: Option<Signal<String>>,
    /// Display name (shown in overlay and as title tooltip)
    #[prop(into, optional)]
    name: Option<Signal<String>>,
    /// Card size
    #[prop(optional, default = CardSize::Sm)]
    size: CardSize,
    /// Accent color for top bar
    #[prop(into, optional)]
    accent_color: Option<Signal<String>>,
    /// If true, card is non-interactive
    #[prop(optional)]
    is_static: bool,
    /// If true, show name overlay
    #[prop(optional)]
    show_name: bool,
    /// If true, shows skeleton while image loads (default: true)
    #[prop(optional, default = true)]
    show_skeleton: bool,
    /// Click callback
    #[prop(into, optional)]
    on_click: Option<Callback<()>>,
    /// Image loaded callback
    #[prop(into, optional)]
    on_load: Option<Callback<()>>,
) -> impl IntoView {
    let size_class = format!("image-card--{}", size.class_suffix());

    let card_class = move || {
        let mut classes = vec!["image-card", &size_class];
        if is_static {
            classes.push("image-card--static");
        }
        classes.join(" ")
    };

    let handle_click = move |_| {
        if !is_static {
            if let Some(cb) = on_click {
                cb.run(());
            }
        }
    };

    // Convert name to a signal for reactive access
    let name_signal: Memo<String> =
        Memo::new(move |_| name.as_ref().map(|n| n.get()).unwrap_or_default());

    // Convert image_url to a signal
    let url_signal: Memo<String> =
        Memo::new(move |_| image_url.as_ref().map(|u| u.get()).unwrap_or_default());

    // Convert accent_color to a signal
    let accent_signal: Memo<Option<String>> =
        Memo::new(move |_| accent_color.as_ref().map(|c| c.get()));

    // Store callback in StoredValue to safely handle async image load events
    // even if the reactive scope is disposed
    let stored_on_load = StoredValue::new(on_load);

    // Track image loading state
    let (is_loaded, set_is_loaded) = signal(false);

    // Reset loaded state when URL changes
    let url_for_effect = url_signal;
    Effect::new(move |prev_url: Option<String>| {
        let current_url = url_for_effect.get();
        if prev_url.is_some() && prev_url.as_ref() != Some(&current_url) {
            set_is_loaded.set(false);
        }
        current_url
    });

    view! {
        <div
            class=card_class
            title=move || name_signal.get()
            on:click=handle_click
        >
            {move || accent_signal.get().map(|color| view! {
                <div class="image-card__accent" style=format!("background-color: {color}")></div>
            })}

            <div class="image-card__image-wrapper">
                {move || {
                    let url = url_signal.get();
                    if !url.is_empty() {
                        // Check cache for preloaded blob URL
                        let resolved_url = crate::image_cache::get_cached_url(&url)
                            .unwrap_or(url);
                        view! {
                            // Show skeleton while loading (if enabled)
                            <Show when=move || show_skeleton && !is_loaded.get()>
                                <div class="image-card__skeleton">
                                    <div class="ui-skeleton ui-skeleton--rect"></div>
                                </div>
                            </Show>
                            <img
                                class="image-card__image"
                                class:image-card__image--loading=move || show_skeleton && !is_loaded.get()
                                src=resolved_url
                                alt=move || name_signal.get()
                                loading="lazy"
                                on:load=move |_| {
                                    set_is_loaded.set(true);
                                    // Use try_get_value to safely handle disposed scope
                                    if let Some(Some(cb)) = stored_on_load.try_get_value() {
                                        cb.run(());
                                    }
                                }
                                on:error=move |_| {
                                    // Hide skeleton on error too
                                    set_is_loaded.set(true);
                                }
                            />
                        }.into_any()
                    } else {
                        view! {
                            <div class="image-card__placeholder"></div>
                        }.into_any()
                    }
                }}

                {move || {
                    if show_name {
                        let n = name_signal.get();
                        if !n.is_empty() {
                            Some(view! {
                                <div class="image-card__name">{n}</div>
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }}
            </div>
        </div>
    }
}
