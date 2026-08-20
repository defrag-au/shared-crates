//! Shared helper components for rendering story cards

use cardano_assets::AssetId;
use leptos::prelude::*;

// ============================================================================
// IIIF Image URL Helper
// ============================================================================

/// Generate an IIIF image URL for an asset
///
/// # Arguments
/// * `asset_id` - The asset identifier (policy_id + asset_name_hex)
/// * `size` - Image width in pixels (height scales proportionally)
///
/// Stories ask for small icon widths (32, 48) which the service does **not**
/// keep warm, so this goes through [`image_core::IiifUrl::custom_px`] — cold on
/// every request, which is acceptable for a dev-only storybook and would not be
/// in a shipping surface. Real surfaces want [`image_core::ImageSize`].
///
/// # Example
/// ```ignore
/// let url = resolve_iiif_image(&asset_id, 256);
/// // Returns: https://iiif.hodlcroft.com/iiif/3/{policy_id}:{asset_name_hex}/full/256,/0/default.jpg
/// ```
pub fn resolve_iiif_image(asset_id: &AssetId, size: u16) -> String {
    image_core::IiifUrl::new(&asset_id.policy_id, &asset_id.asset_name_hex)
        .custom_px(u32::from(size))
        .build()
}

/// Render an attribute documentation card
#[component]
pub fn AttributeCard(
    name: &'static str,
    values: &'static str,
    description: &'static str,
) -> impl IntoView {
    view! {
        <div class="wallet-card">
            <div class="wallet-card__header">
                <span class="wallet-card__name">{name}</span>
            </div>
            <div class="wallet-card__body">
                <div class="wallet-card__row">
                    <span class="wallet-card__label">"Values"</span>
                    <span class="wallet-card__value">{values}</span>
                </div>
                <p style="margin-top: 0.5rem; font-size: 0.9em; color: #8b8fa3;">
                    {description}
                </p>
            </div>
        </div>
    }
}

/// Render a loader step card (numbered)
#[component]
pub fn LoaderStepCard(
    step: &'static str,
    title: &'static str,
    description: &'static str,
) -> impl IntoView {
    view! {
        <div class="wallet-card">
            <div class="wallet-card__header">
                <div class="wallet-card__icon">{step}</div>
                <span class="wallet-card__name">{title}</span>
            </div>
            <div class="wallet-card__body">
                <p>{description}</p>
            </div>
        </div>
    }
}

/// Render a loader error card
#[component]
pub fn LoaderErrorCard(error_type: &'static str, description: &'static str) -> impl IntoView {
    view! {
        <div class="wallet-card">
            <div class="wallet-card__header">
                <span class="status-indicator status-indicator--disconnected">{error_type}</span>
            </div>
            <div class="wallet-card__body">
                <p>{description}</p>
            </div>
        </div>
    }
}

/// Render a config option card
#[component]
pub fn ConfigOptionCard(
    name: &'static str,
    type_name: &'static str,
    default: &'static str,
    description: &'static str,
) -> impl IntoView {
    view! {
        <div class="wallet-card">
            <div class="wallet-card__header">
                <span class="wallet-card__name">{name}</span>
            </div>
            <div class="wallet-card__body">
                <div class="wallet-card__row">
                    <span class="wallet-card__label">"Type"</span>
                    <span class="wallet-card__value">{type_name}</span>
                </div>
                <div class="wallet-card__row">
                    <span class="wallet-card__label">"Default"</span>
                    <span class="wallet-card__value">{default}</span>
                </div>
                <p style="margin-top: 0.5rem; font-size: 0.9em; color: #8b8fa3;">
                    {description}
                </p>
            </div>
        </div>
    }
}

/// Render a trait method card
#[component]
pub fn TraitMethodCard(
    signature: &'static str,
    returns: &'static str,
    description: &'static str,
) -> impl IntoView {
    view! {
        <div class="wallet-card">
            <div class="wallet-card__header">
                <span class="wallet-card__name">{signature}</span>
            </div>
            <div class="wallet-card__body">
                <div class="wallet-card__row">
                    <span class="wallet-card__label">"Returns"</span>
                    <span class="wallet-card__value">{returns}</span>
                </div>
                <p style="margin-top: 0.5rem; font-size: 0.9em; color: #8b8fa3;">
                    {description}
                </p>
            </div>
        </div>
    }
}

/// Render a toast kind card
#[component]
pub fn ToastKindCard(
    name: &'static str,
    css_class: &'static str,
    icon: &'static str,
    example: &'static str,
) -> impl IntoView {
    view! {
        <div class="wallet-card">
            <div class="wallet-card__header">
                <div class="wallet-card__icon">{icon}</div>
                <span class="wallet-card__name">{name}</span>
            </div>
            <div class="wallet-card__body">
                <div class="wallet-card__row">
                    <span class="wallet-card__label">"CSS Class"</span>
                    <span class="wallet-card__value">{css_class}</span>
                </div>
                <div class="wallet-card__row">
                    <span class="wallet-card__label">"Example"</span>
                    <span class="wallet-card__value">{example}</span>
                </div>
            </div>
        </div>
    }
}

/// Render a toast function card
#[component]
pub fn ToastFnCard(signature: &'static str, description: &'static str) -> impl IntoView {
    view! {
        <div class="wallet-card">
            <div class="wallet-card__header">
                <span class="wallet-card__name">{signature}</span>
            </div>
            <div class="wallet-card__body">
                <p>{description}</p>
            </div>
        </div>
    }
}

/// Render a flow concept card
#[component]
pub fn FlowConceptCard(title: &'static str, description: &'static str) -> impl IntoView {
    view! {
        <div class="wallet-card">
            <div class="wallet-card__header">
                <span class="wallet-card__name">{title}</span>
            </div>
            <div class="wallet-card__body">
                <p>{description}</p>
            </div>
        </div>
    }
}
