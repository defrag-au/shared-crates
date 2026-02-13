//! Asset Card Leptos Component
//!
//! A card for displaying Cardano NFT assets. Wraps `ImageCard` and adds
//! automatic IIIF URL generation from asset IDs. Supports overlay slots
//! for additional content like stats, badges, and actions.
//!
//! ## Image URL Resolution
//!
//! The component generates IIIF URLs from the asset ID:
//! - Asset ID format: `{policy_id}{asset_name_hex}` (56+ chars)
//! - Generated URL: `https://iiif.hodlcroft.com/iiif/3/{policy_id}:{asset_name}/full/{size},/0/default.jpg`
//!
//! The IIIF image size is automatically selected based on card size:
//! - xs, sm, md, lg (≤400px): uses 400px IIIF image (cached, fast)
//! - xl (>400px): uses 1686px IIIF image (high resolution)
//!
//! ## Overlay Slots
//!
//! The card supports overlay content at four corners and a footer:
//! - `top_left` - Content positioned at top-left (e.g., status indicators)
//! - `top_right` - Content positioned at top-right (e.g., power pills)
//! - `bottom_left` - Content positioned at bottom-left (e.g., role badges)
//! - `bottom_right` - Content positioned at bottom-right
//! - `footer` - Content below the image (e.g., action buttons)
//!
//! ## Props
//!
//! - `asset_id` - Cardano asset ID (policy_id + asset_name hex) - generates IIIF URL
//! - `image_url` - Direct image URL (fallback when asset_id not available)
//! - `size` - Card size: Xs, Sm, Md, Lg, Xl
//! - `name` - Display name of the asset
//! - `accent_color` - Optional accent/tier color for top bar
//! - `is_static` - If true, card is non-interactive
//! - `show_name` - If true, show name overlay
//! - `on_click` - Callback when card is clicked, receives asset_id
//! - `on_load` - Callback when image has loaded
//! - `top_left`, `top_right`, `bottom_left`, `bottom_right` - Overlay slot content
//! - `footer` - Footer slot content
//!
//! ## Usage
//!
//! ```ignore
//! // Basic usage
//! <AssetCard
//!     asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465313839"
//!     name="Pirate #189"
//!     size=CardSize::Md
//!     show_name=true
//!     on_click=move |id| { log!("clicked: {}", id); }
//! />
//!
//! // With overlay slots
//! <AssetCard
//!     asset_id=asset_id
//!     name=name
//!     top_right=view! { <StatPill value="350" icon="⚡" /> }
//!     bottom_left=view! { <RoleBadges roles=roles /> }
//!     footer=view! { <Button on_click=hire>"Hire"</Button> }
//! />
//! ```

use crate::image_card::{CardSize, ImageCard};
use leptos::children::ChildrenFn;
use leptos::prelude::*;

/// IIIF base URL for image lookups
const IIIF_BASE_URL: &str = "https://iiif.hodlcroft.com/iiif/3";

/// IIIF image size variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IiifSize {
    /// 400px width - fast, cached thumbnails
    #[default]
    Thumb,
    /// 1686px width - high resolution
    Large,
}

impl IiifSize {
    /// Get the IIIF size parameter value
    pub fn pixels(&self) -> u16 {
        match self {
            IiifSize::Thumb => 400,
            IiifSize::Large => 1686,
        }
    }

    /// Select appropriate IIIF size for a given card size
    /// Uses thumb (400px) for cards up to 400px, large for bigger cards
    pub fn for_card_size(card_size: CardSize) -> Self {
        match card_size.pixels() {
            Some(px) if px > 400 => IiifSize::Large,
            _ => IiifSize::Thumb,
        }
    }
}

/// Generate IIIF URL from asset ID and size
///
/// Asset ID format: `{policy_id_hex}{asset_name_hex}` (total 56+ chars)
/// Policy ID is always 56 hex chars, asset name is the remainder.
///
/// Returns None if asset_id is too short to contain a valid policy ID.
pub fn generate_iiif_url(asset_id: &str, size: IiifSize) -> Option<String> {
    // Policy ID is 56 hex characters (28 bytes)
    if asset_id.len() < 56 {
        return None;
    }

    let policy_id = &asset_id[..56];
    let asset_name = &asset_id[56..];

    if asset_name.is_empty() {
        return None;
    }

    // IIIF format: {policy_id}:{asset_name_hex}
    Some(format!(
        "{IIIF_BASE_URL}/{policy_id}:{asset_name}/full/{},/0/default.jpg",
        size.pixels()
    ))
}

/// Asset card component - wraps ImageCard with IIIF URL generation and overlay slots
#[component]
pub fn AssetCard(
    /// Cardano asset ID (policy_id + asset_name hex)
    #[prop(into, optional)]
    asset_id: Option<Signal<String>>,
    /// Direct image URL (fallback when asset_id not available)
    #[prop(into, optional)]
    image_url: Option<Signal<String>>,
    /// Card size
    #[prop(optional, default = CardSize::Sm)]
    size: CardSize,
    /// Display name
    #[prop(into, optional)]
    name: Option<Signal<String>>,
    /// Accent color for top bar
    #[prop(into, optional)]
    accent_color: Option<Signal<String>>,
    /// If true, card is non-interactive
    #[prop(optional)]
    is_static: bool,
    /// If true, show name overlay
    #[prop(optional)]
    show_name: bool,
    /// Click callback - receives asset_id
    #[prop(into, optional)]
    on_click: Option<Callback<String>>,
    /// Image loaded callback
    #[prop(into, optional)]
    on_load: Option<Callback<()>>,
    /// Top-left overlay slot (e.g., status indicators)
    #[prop(optional)]
    top_left: Option<ChildrenFn>,
    /// Top-right overlay slot (e.g., power pills)
    #[prop(optional)]
    top_right: Option<ChildrenFn>,
    /// Bottom-left overlay slot (e.g., role badges)
    #[prop(optional)]
    bottom_left: Option<ChildrenFn>,
    /// Bottom-right overlay slot
    #[prop(optional)]
    bottom_right: Option<ChildrenFn>,
    /// Footer slot (e.g., action buttons)
    #[prop(optional)]
    footer: Option<ChildrenFn>,
) -> impl IntoView {
    // Clone for use in closures
    let asset_id_for_url = asset_id;
    let asset_id_for_click = asset_id;

    // Eagerly render overlay content
    let top_left_content = top_left.map(|c| c());
    let top_right_content = top_right.map(|c| c());
    let bottom_left_content = bottom_left.map(|c| c());
    let bottom_right_content = bottom_right.map(|c| c());
    let footer_content = footer.map(|c| c());

    let has_overlays = top_left_content.is_some()
        || top_right_content.is_some()
        || bottom_left_content.is_some()
        || bottom_right_content.is_some();
    let has_footer = footer_content.is_some();

    // Resolve image URL - prefer direct URL, fall back to IIIF generation
    let resolved_url: Memo<String> = Memo::new(move |_| {
        // Direct image URL takes precedence
        if let Some(ref url_signal) = image_url {
            let url = url_signal.get();
            if !url.is_empty() {
                return url;
            }
        }

        // Fall back to IIIF URL generation from asset-id
        if let Some(ref id_signal) = asset_id_for_url {
            let id = id_signal.get();
            if !id.is_empty() {
                let iiif_size = IiifSize::for_card_size(size);
                if let Some(url) = generate_iiif_url(&id, iiif_size) {
                    return url;
                }
            }
        }

        String::new()
    });

    let handle_click = move || {
        if let Some(cb) = on_click {
            let id = asset_id_for_click
                .as_ref()
                .map(|s| s.get())
                .unwrap_or_default();
            cb.run(id);
        }
    };

    // Convert optional signals to memos for ImageCard
    let name_memo: Memo<String> =
        Memo::new(move |_| name.as_ref().map(|n| n.get()).unwrap_or_default());

    let accent_memo: Memo<String> =
        Memo::new(move |_| accent_color.as_ref().map(|c| c.get()).unwrap_or_default());

    // Wrap the on_load callback to forward through
    let handle_load = move || {
        if let Some(cb) = on_load {
            cb.run(());
        }
    };

    let size_class = format!("asset-card--{}", size.class_suffix());
    let wrapper_class = if has_overlays || has_footer {
        format!("asset-card {size_class}")
    } else {
        // No wrapper needed if no overlays
        String::new()
    };

    // If we have overlays or footer, wrap the ImageCard in our container
    if has_overlays || has_footer {
        view! {
            <div class=wrapper_class>
                <div class="asset-card__image-container">
                    <ImageCard
                        image_url=Signal::derive(move || resolved_url.get())
                        name=Signal::derive(move || name_memo.get())
                        size=size
                        accent_color=Signal::derive(move || accent_memo.get())
                        is_static=is_static
                        show_name=show_name
                        on_click=handle_click
                        on_load=handle_load
                    />

                    {has_overlays.then(|| view! {
                        <div class="asset-card__overlays">
                            {top_left_content.map(|c| view! {
                                <div class="asset-card__overlay asset-card__overlay--top-left">{c}</div>
                            })}
                            {top_right_content.map(|c| view! {
                                <div class="asset-card__overlay asset-card__overlay--top-right">{c}</div>
                            })}
                            {bottom_left_content.map(|c| view! {
                                <div class="asset-card__overlay asset-card__overlay--bottom-left">{c}</div>
                            })}
                            {bottom_right_content.map(|c| view! {
                                <div class="asset-card__overlay asset-card__overlay--bottom-right">{c}</div>
                            })}
                        </div>
                    })}
                </div>

                {footer_content.map(|c| view! {
                    <div class="asset-card__footer">{c}</div>
                })}
            </div>
        }
        .into_any()
    } else {
        // No overlays - render ImageCard directly
        view! {
            <ImageCard
                image_url=Signal::derive(move || resolved_url.get())
                name=Signal::derive(move || name_memo.get())
                size=size
                accent_color=Signal::derive(move || accent_memo.get())
                is_static=is_static
                show_name=show_name
                on_click=handle_click
                on_load=handle_load
            />
        }
        .into_any()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_iiif_url() {
        let asset_id = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465313839";

        let thumb_url = generate_iiif_url(asset_id, IiifSize::Thumb).unwrap();
        assert!(thumb_url.contains("/400,/"));
        assert!(thumb_url.contains(
            "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6:506972617465313839"
        ));

        let large_url = generate_iiif_url(asset_id, IiifSize::Large).unwrap();
        assert!(large_url.contains("/1686,/"));
    }

    #[test]
    fn test_generate_iiif_url_short_id() {
        let short_id = "abc123";
        assert!(generate_iiif_url(short_id, IiifSize::Thumb).is_none());
    }

    #[test]
    fn test_generate_iiif_url_policy_only() {
        // 56 chars but no asset name
        let policy_only = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";
        assert!(generate_iiif_url(policy_only, IiifSize::Thumb).is_none());
    }

    #[test]
    fn test_iiif_size_for_card_size() {
        assert_eq!(IiifSize::for_card_size(CardSize::Xs), IiifSize::Thumb);
        assert_eq!(IiifSize::for_card_size(CardSize::Sm), IiifSize::Thumb);
        assert_eq!(IiifSize::for_card_size(CardSize::Md), IiifSize::Thumb);
        assert_eq!(IiifSize::for_card_size(CardSize::Lg), IiifSize::Thumb);
        assert_eq!(IiifSize::for_card_size(CardSize::Xl), IiifSize::Large);
    }
}
