//! Badge component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{Badge, BadgeVariant};

#[component]
pub fn BadgeStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Badge"</h2>
                <p>"A small label for categorization, status, or tagging. Supports solid, outline, and subtle variants."</p>
            </div>

            // Basic examples
            <div class="story-section">
                <h3>"Basic Examples"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 0.5rem; flex-wrap: wrap; align-items: center;">
                        <Badge label="Default" />
                        <Badge label="New" />
                        <Badge label="Featured" />
                        <Badge label="Sale" />
                    </div>
                </div>
            </div>

            // Variants
            <div class="story-section">
                <h3>"Variants"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem;">
                        <div>
                            <p style="margin: 0 0 0.5rem; color: #888; font-size: 0.875rem;">"Solid (default)"</p>
                            <div style="display: flex; gap: 0.5rem; flex-wrap: wrap;">
                                <Badge label="Solid" variant=BadgeVariant::Solid />
                                <Badge label="Solid" variant=BadgeVariant::Solid color="#28a745" />
                                <Badge label="Solid" variant=BadgeVariant::Solid color="#dc3545" />
                                <Badge label="Solid" variant=BadgeVariant::Solid color="#FFD700" />
                            </div>
                        </div>
                        <div>
                            <p style="margin: 0 0 0.5rem; color: #888; font-size: 0.875rem;">"Outline"</p>
                            <div style="display: flex; gap: 0.5rem; flex-wrap: wrap;">
                                <Badge label="Outline" variant=BadgeVariant::Outline />
                                <Badge label="Outline" variant=BadgeVariant::Outline color="#28a745" />
                                <Badge label="Outline" variant=BadgeVariant::Outline color="#dc3545" />
                                <Badge label="Outline" variant=BadgeVariant::Outline color="#FFD700" />
                            </div>
                        </div>
                        <div>
                            <p style="margin: 0 0 0.5rem; color: #888; font-size: 0.875rem;">"Subtle"</p>
                            <div style="display: flex; gap: 0.5rem; flex-wrap: wrap;">
                                <Badge label="Subtle" variant=BadgeVariant::Subtle />
                                <Badge label="Subtle" variant=BadgeVariant::Subtle color="#28a745" />
                                <Badge label="Subtle" variant=BadgeVariant::Subtle color="#dc3545" />
                                <Badge label="Subtle" variant=BadgeVariant::Subtle color="#FFD700" />
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            // Use cases
            <div class="story-section">
                <h3>"Use Cases"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem;">
                        <div style="display: flex; align-items: center; gap: 0.5rem;">
                            <span>"Roles:"</span>
                            <Badge label="Captain" color="#FFD700" />
                            <Badge label="Navigator" color="#17a2b8" />
                            <Badge label="Gunner" color="#dc3545" />
                        </div>
                        <div style="display: flex; align-items: center; gap: 0.5rem;">
                            <span>"Status:"</span>
                            <Badge label="Active" color="#28a745" />
                            <Badge label="Pending" color="#ffc107" variant=BadgeVariant::Outline />
                            <Badge label="Inactive" color="#6c757d" variant=BadgeVariant::Subtle />
                        </div>
                        <div style="display: flex; align-items: center; gap: 0.5rem;">
                            <span>"Tags:"</span>
                            <Badge label="Rare" color="#9b59b6" />
                            <Badge label="Epic" color="#e74c3c" />
                            <Badge label="Legendary" color="#f39c12" />
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
                            name="label"
                            values="String"
                            description="Text to display in the badge"
                        />
                        <AttributeCard
                            name="color"
                            values="String (optional)"
                            description="CSS color for the badge"
                        />
                        <AttributeCard
                            name="variant"
                            values="BadgeVariant (Solid|Outline|Subtle)"
                            description="Visual style - default is Solid"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{Badge, BadgeVariant};

// Basic badge
view! { <Badge label="New" /> }

// Colored badge
view! { <Badge label="Sale" color="#dc3545" /> }

// Outline variant
view! {
    <Badge
        label="Pending"
        color="#ffc107"
        variant=BadgeVariant::Outline
    />
}

// Subtle variant
view! {
    <Badge
        label="Archived"
        variant=BadgeVariant::Subtle
    />
}"##}</pre>
            </div>
        </div>
    }
}
