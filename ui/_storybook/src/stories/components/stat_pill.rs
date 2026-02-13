//! StatPill component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{StatPill, StatPillColor, StatPillSize};

#[component]
pub fn StatPillStory() -> impl IntoView {
    let (dynamic_value, set_dynamic_value) = signal(150u32);

    view! {
        <div>
            <div class="story-header">
                <h2>"Stat Pill"</h2>
                <p>"A compact pill for displaying stats with optional icon and color. Use for power levels, counts, scores, or any numeric value."</p>
            </div>

            // Basic examples
            <div class="story-section">
                <h3>"Basic Examples"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1rem; flex-wrap: wrap; align-items: center;">
                        <StatPill value="42" />
                        <StatPill value="100" icon="⚡" />
                        <StatPill value="5/10" icon="❤️" />
                        <StatPill value="Level 3" />
                    </div>
                </div>
            </div>

            // Color presets
            <div class="story-section">
                <h3>"Color Presets"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1rem; flex-wrap: wrap; align-items: center;">
                        <StatPill value="Default" />
                        <StatPill value="Success" color=StatPillColor::Success />
                        <StatPill value="Warning" color=StatPillColor::Warning />
                        <StatPill value="Danger" color=StatPillColor::Danger />
                        <StatPill value="Info" color=StatPillColor::Info />
                        <StatPill value="Muted" color=StatPillColor::Muted />
                    </div>
                </div>
            </div>

            // Custom colors
            <div class="story-section">
                <h3>"Custom Colors"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1rem; flex-wrap: wrap; align-items: center;">
                        <StatPill value="Gold" color="#FFD700" />
                        <StatPill value="Purple" color="#9b59b6" />
                        <StatPill value="Cyan" color="#17a2b8" />
                        <StatPill value="Orange" color="#fd7e14" />
                    </div>
                </div>
            </div>

            // Size variants
            <div class="story-section">
                <h3>"Size Variants"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1rem; flex-wrap: wrap; align-items: center;">
                        <StatPill value="Small" size=StatPillSize::Sm icon="⚡" />
                        <StatPill value="Medium" size=StatPillSize::Md icon="⚡" />
                        <StatPill value="Large" size=StatPillSize::Lg icon="⚡" />
                    </div>
                </div>
            </div>

            // Interactive demo
            <div class="story-section">
                <h3>"Interactive Demo"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1rem; align-items: center;">
                        <StatPill
                            value=move || dynamic_value.get().to_string()
                            icon="⚡"
                            color=StatPillColor::Info
                        />
                        <button class="btn btn--secondary btn--sm" on:click=move |_| set_dynamic_value.update(|v| *v = v.saturating_sub(50))>"-50"</button>
                        <button class="btn btn--secondary btn--sm" on:click=move |_| set_dynamic_value.update(|v| *v += 50)>"+50"</button>
                        <span style="color: #888;">"Value updates reactively!"</span>
                    </div>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="value"
                            values="impl Into<Signal<String>>"
                            description="The text/number to display"
                        />
                        <AttributeCard
                            name="icon"
                            values="String (optional)"
                            description="Emoji or text icon shown before the value"
                        />
                        <AttributeCard
                            name="color"
                            values="StatPillColor | String"
                            description="Preset color or custom CSS color"
                        />
                        <AttributeCard
                            name="size"
                            values="StatPillSize (Sm|Md|Lg)"
                            description="Pill size - default is Md"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{StatPill, StatPillColor, StatPillSize};

// Basic pill
view! { <StatPill value="42" /> }

// With icon and preset color
view! {
    <StatPill
        value="250"
        icon="⚡"
        color=StatPillColor::Success
    />
}

// Custom color and size
view! {
    <StatPill
        value="MAX"
        icon="🔥"
        color="#FFD700"
        size=StatPillSize::Lg
    />
}

// Reactive value
let (power, _) = signal(100);
view! {
    <StatPill
        value=move || power.get().to_string()
        icon="⚡"
    />
}"##}</pre>
            </div>
        </div>
    }
}
