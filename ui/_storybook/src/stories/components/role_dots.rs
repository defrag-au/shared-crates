//! Role Dots component story

use crate::stories::helpers::{resolve_iiif_image, AttributeCard};
use cardano_assets::AssetId;
use leptos::prelude::*;
use ui_components::{children_fn, AssetCard, CardSize, RoleDot, RoleDots};

// Black Flag Pirates policy ID
const BFP_POLICY: &str = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

// Sample pirates for demo
fn pirate_asset(asset_name_hex: &str) -> AssetId {
    AssetId {
        policy_id: BFP_POLICY.to_string(),
        asset_name_hex: asset_name_hex.to_string(),
    }
}

#[component]
pub fn RoleDotsStory() -> impl IntoView {
    // Asset images for the "In Context" section
    let pirate_1_url = resolve_iiif_image(&pirate_asset("506972617465313839"), 48);
    let pirate_2_url = resolve_iiif_image(&pirate_asset("506972617465323030"), 48);
    let pirate_3_url = resolve_iiif_image(&pirate_asset("506972617465333333"), 48);

    // Example roles for demos
    let single_role = vec![RoleDot::new("doctor")
        .with_color("#4ade80")
        .with_label("Doctor")];

    let two_roles = vec![
        RoleDot::new("doctor")
            .with_color("#4ade80")
            .with_label("Doctor"),
        RoleDot::new("navigator")
            .with_color("#60a5fa")
            .with_label("Navigator"),
    ];

    let many_roles = vec![
        RoleDot::new("doctor")
            .with_color("#4ade80")
            .with_label("Doctor"),
        RoleDot::new("gunner")
            .with_color("#f87171")
            .with_label("Gunner"),
        RoleDot::new("navigator")
            .with_color("#60a5fa")
            .with_label("Navigator"),
        RoleDot::new("cook")
            .with_color("#fbbf24")
            .with_label("Cook"),
    ];

    let pirate_roles = vec![
        RoleDot::new("quartermaster")
            .with_color("#c084fc")
            .with_label("Quartermaster"),
        RoleDot::new("bosun")
            .with_color("#f97316")
            .with_label("Bosun"),
        RoleDot::new("carpenter")
            .with_color("#a78bfa")
            .with_label("Carpenter"),
    ];

    // Clone for multiple uses in view
    let single_role_2 = single_role.clone();
    let two_roles_2 = two_roles.clone();
    let many_roles_2 = many_roles.clone();
    let many_roles_3 = many_roles.clone();
    let pirate_roles_2 = pirate_roles.clone();

    // For AssetCard demo
    let card_roles_1 = two_roles.clone();
    let card_roles_2 = many_roles.clone();
    let card_roles_3 = pirate_roles.clone();

    view! {
        <div>
            <div class="story-header">
                <h2>"Role Dots"</h2>
                <p>"Compact colored dots showing role assignments. Hover to see tooltip with role names. Perfect for small layouts where full badges are too large."</p>
            </div>

            // Basic examples
            <div class="story-section">
                <h3>"Basic Usage"</h3>
                <p class="story-description">"Hover over the dots to see role names in the tooltip."</p>
                <div class="story-canvas">
                    <div style="display: flex; gap: 2rem; align-items: center;">
                        <div style="text-align: center;">
                            <RoleDots roles=Signal::derive(move || single_role.clone()) />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"1 role"</p>
                        </div>
                        <div style="text-align: center;">
                            <RoleDots roles=Signal::derive(move || two_roles.clone()) />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"2 roles"</p>
                        </div>
                        <div style="text-align: center;">
                            <RoleDots roles=Signal::derive(move || many_roles_2.clone()) />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"4 roles"</p>
                        </div>
                        <div style="text-align: center;">
                            <RoleDots roles=Signal::derive(std::vec::Vec::new) />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"Empty"</p>
                        </div>
                    </div>
                </div>
            </div>

            // Without tooltip
            <div class="story-section">
                <h3>"Without Tooltip"</h3>
                <p class="story-description">"Disable tooltip when it's not needed."</p>
                <div class="story-canvas">
                    <RoleDots
                        roles=Signal::derive(move || pirate_roles.clone())
                        show_tooltip=false
                    />
                </div>
            </div>

            // In context
            <div class="story-section">
                <h3>"In Context"</h3>
                <p class="story-description">"Role dots work well in compact card layouts."</p>
                <div class="story-canvas">
                    <div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem; max-width: 600px;">
                        <div style="display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem; background: rgba(255,255,255,0.02); border-radius: 6px;">
                            <img
                                src=pirate_1_url.clone()
                                style="width: 32px; height: 32px; border-radius: 4px;"
                            />
                            <div style="flex: 1; min-width: 0;">
                                <div style="font-size: 0.875rem; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">"Captain Jack"</div>
                            </div>
                            <RoleDots roles=Signal::derive(move || two_roles_2.clone()) />
                        </div>
                        <div style="display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem; background: rgba(255,255,255,0.02); border-radius: 6px;">
                            <img
                                src=pirate_2_url.clone()
                                style="width: 32px; height: 32px; border-radius: 4px;"
                            />
                            <div style="flex: 1; min-width: 0;">
                                <div style="font-size: 0.875rem; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">"Anne Bonny"</div>
                            </div>
                            <RoleDots roles=Signal::derive(move || single_role_2.clone()) />
                        </div>
                        <div style="display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem; background: rgba(255,255,255,0.02); border-radius: 6px;">
                            <img
                                src=pirate_3_url.clone()
                                style="width: 32px; height: 32px; border-radius: 4px;"
                            />
                            <div style="flex: 1; min-width: 0;">
                                <div style="font-size: 0.875rem; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">"Blackbeard"</div>
                            </div>
                            <RoleDots roles=Signal::derive(move || pirate_roles_2.clone()) />
                        </div>
                    </div>
                </div>
            </div>

            // In AssetCard
            <div class="story-section">
                <h3>"In AssetCard"</h3>
                <p class="story-description">"Role dots in the bottom-left slot of an AssetCard. Tooltip is disabled to avoid clipping issues."</p>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1rem; flex-wrap: wrap;">
                        <AssetCard
                            asset_id=pirate_asset("506972617465313839").concatenated()
                            name="Captain Jack"
                            show_name=true
                            size=CardSize::Sm
                            bottom_left=children_fn({
                                let roles = card_roles_1.clone();
                                move || view! { <RoleDots roles=Signal::derive({ let r = roles.clone(); move || r.clone() }) show_tooltip=false /> }
                            })
                        />
                        <AssetCard
                            asset_id=pirate_asset("506972617465323030").concatenated()
                            name="Anne Bonny"
                            show_name=true
                            size=CardSize::Sm
                            bottom_left=children_fn({
                                let roles = card_roles_2.clone();
                                move || view! { <RoleDots roles=Signal::derive({ let r = roles.clone(); move || r.clone() }) show_tooltip=false /> }
                            })
                        />
                        <AssetCard
                            asset_id=pirate_asset("506972617465333333").concatenated()
                            name="Blackbeard"
                            show_name=true
                            size=CardSize::Sm
                            bottom_left=children_fn({
                                let roles = card_roles_3.clone();
                                move || view! { <RoleDots roles=Signal::derive({ let r = roles.clone(); move || r.clone() }) show_tooltip=false /> }
                            })
                        />
                    </div>
                </div>
            </div>

            // Color variations
            <div class="story-section">
                <h3>"Color Variations"</h3>
                <p class="story-description">"Roles without colors use a default gray."</p>
                <div class="story-canvas">
                    <div style="display: flex; gap: 2rem;">
                        <div>
                            <p style="font-size: 0.75rem; color: #888; margin-bottom: 0.5rem;">"With colors"</p>
                            <RoleDots roles=Signal::derive(move || many_roles_3.clone()) />
                        </div>
                        <div>
                            <p style="font-size: 0.75rem; color: #888; margin-bottom: 0.5rem;">"Without colors"</p>
                            <RoleDots roles=Signal::derive(|| vec![
                                RoleDot::new("role1"),
                                RoleDot::new("role2"),
                                RoleDot::new("role3"),
                            ]) />
                        </div>
                    </div>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="roles"
                            values="Signal<Vec<RoleDot>>"
                            description="List of roles to display as dots"
                        />
                        <AttributeCard
                            name="show_tooltip"
                            values="bool (optional)"
                            description="Whether to show tooltip on hover. Defaults to true."
                        />
                    </div>
                </div>
            </div>

            // RoleDot struct
            <div class="story-section">
                <h3>"RoleDot Struct"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="id"
                            values="String"
                            description="Role identifier"
                        />
                        <AttributeCard
                            name="color"
                            values="Option<String>"
                            description="CSS color for the dot. Defaults to gray if not provided."
                        />
                        <AttributeCard
                            name="label"
                            values="Option<String>"
                            description="Display label for tooltip. Falls back to capitalized id."
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{RoleDot, RoleDots};

// Define roles
let roles = vec![
    RoleDot::new("doctor")
        .with_color("#4ade80")
        .with_label("Doctor"),
    RoleDot::new("gunner")
        .with_color("#f87171")
        .with_label("Gunner"),
];

// Basic usage with tooltip
view! {
    <RoleDots roles=Signal::derive(move || roles.clone()) />
}

// Without tooltip
view! {
    <RoleDots
        roles=Signal::derive(move || roles.clone())
        show_tooltip=false
    />
}

// Roles without custom colors (uses default gray)
let simple_roles = vec![
    RoleDot::new("role1"),
    RoleDot::new("role2"),
];
view! {
    <RoleDots roles=Signal::derive(move || simple_roles.clone()) />
}"##}</pre>
            </div>
        </div>
    }
}
