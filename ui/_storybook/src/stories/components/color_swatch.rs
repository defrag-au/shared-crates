//! ColorSwatch component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{ColorSwatch, SwatchSize};

#[component]
pub fn ColorSwatchStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"ColorSwatch"</h2>
                <p>"A small color sample display for showing tier colors, role colors, etc."</p>
            </div>

            // Basic examples
            <div class="story-section">
                <h3>"Basic Examples"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1.5rem; flex-wrap: wrap; align-items: center;">
                        <ColorSwatch color="#FFD700" />
                        <ColorSwatch color="#28a745" />
                        <ColorSwatch color="#dc3545" />
                        <ColorSwatch color="#17a2b8" />
                        <ColorSwatch color="#6c757d" />
                    </div>
                </div>
            </div>

            // With labels
            <div class="story-section">
                <h3>"With Labels"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1.5rem; flex-wrap: wrap; align-items: center;">
                        <ColorSwatch color="#FFD700" label="Gold" />
                        <ColorSwatch color="#28a745" label="Success" />
                        <ColorSwatch color="#dc3545" label="Danger" />
                        <ColorSwatch color="#17a2b8" label="Info" />
                    </div>
                </div>
            </div>

            // Sizes
            <div class="story-section">
                <h3>"Sizes"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 2rem; flex-wrap: wrap; align-items: center;">
                        <ColorSwatch color="#9b59b6" label="Small" size=SwatchSize::Sm />
                        <ColorSwatch color="#9b59b6" label="Medium" size=SwatchSize::Md />
                        <ColorSwatch color="#9b59b6" label="Large" size=SwatchSize::Lg />
                    </div>
                </div>
            </div>

            // Show hex value
            <div class="story-section">
                <h3>"With Hex Value"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1.5rem; flex-wrap: wrap; align-items: center;">
                        <ColorSwatch color="#FFD700" label="Gold" show_hex=true />
                        <ColorSwatch color="#e74c3c" show_hex=true />
                        <ColorSwatch color="#3498db" label="Blue" show_hex=true size=SwatchSize::Lg />
                    </div>
                </div>
            </div>

            // Use case: Tier colors
            <div class="story-section">
                <h3>"Use Case: Tier Colors"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 0.75rem;">
                        <ColorSwatch color="#6c757d" label="Tier 1 - Common" />
                        <ColorSwatch color="#17a2b8" label="Tier 2 - Uncommon" />
                        <ColorSwatch color="#28a745" label="Tier 3 - Rare" />
                        <ColorSwatch color="#ffc107" label="Tier 4 - Epic" />
                        <ColorSwatch color="#fd7e14" label="Tier 5 - Legendary" />
                        <ColorSwatch color="#dc3545" label="Tier 6 - Mythic" />
                    </div>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="color"
                            values="String"
                            description="CSS color value (hex, rgb, etc.)"
                        />
                        <AttributeCard
                            name="label"
                            values="String (optional)"
                            description="Text label next to the swatch"
                        />
                        <AttributeCard
                            name="size"
                            values="SwatchSize (Sm|Md|Lg)"
                            description="Swatch size - default is Md"
                        />
                        <AttributeCard
                            name="show_hex"
                            values="bool (default: false)"
                            description="Display the color value"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{ColorSwatch, SwatchSize};

// Simple swatch
view! { <ColorSwatch color="#FFD700" /> }

// With label
view! { <ColorSwatch color="#28a745" label="Success" /> }

// Large with hex value
view! {
    <ColorSwatch
        color="#dc3545"
        label="Danger"
        size=SwatchSize::Lg
        show_hex=true
    />
}"##}</pre>
            </div>
        </div>
    }
}
