//! ProgressBar component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::ProgressBar;

#[component]
pub fn ProgressBarStory() -> impl IntoView {
    let (progress, set_progress) = signal(0.65f32);

    view! {
        <div>
            <div class="story-header">
                <h2>"Progress Bar"</h2>
                <p>"A horizontal bar showing progress from 0% to 100%. Supports custom colors and optional label."</p>
            </div>

            // Basic examples
            <div class="story-section">
                <h3>"Basic Examples"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem;">
                        <ProgressBar value=Signal::derive(|| 0.0) />
                        <ProgressBar value=Signal::derive(|| 0.25) />
                        <ProgressBar value=Signal::derive(|| 0.5) />
                        <ProgressBar value=Signal::derive(|| 0.75) />
                        <ProgressBar value=Signal::derive(|| 1.0) />
                    </div>
                </div>
            </div>

            // With labels
            <div class="story-section">
                <h3>"With Labels"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem;">
                        <ProgressBar value=Signal::derive(|| 0.3) label="Loading..." />
                        <ProgressBar value=Signal::derive(|| 0.7) label="70% Complete" />
                        <ProgressBar value=Signal::derive(|| 1.0) label="Done!" />
                    </div>
                </div>
            </div>

            // With percentage
            <div class="story-section">
                <h3>"Show Percentage"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem;">
                        <ProgressBar value=Signal::derive(|| 0.45) show_percentage=true />
                        <ProgressBar value=Signal::derive(|| 0.8) label="Progress" show_percentage=true />
                    </div>
                </div>
            </div>

            // Custom colors
            <div class="story-section">
                <h3>"Custom Colors"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem;">
                        <ProgressBar value=Signal::derive(|| 0.6) label="Default" />
                        <ProgressBar value=Signal::derive(|| 0.6) label="Success" color="#28a745" />
                        <ProgressBar value=Signal::derive(|| 0.6) label="Warning" color="#ffc107" />
                        <ProgressBar value=Signal::derive(|| 0.6) label="Danger" color="#dc3545" />
                        <ProgressBar value=Signal::derive(|| 0.6) label="Info" color="#17a2b8" />
                        <ProgressBar value=Signal::derive(|| 0.6) label="Gold" color="#FFD700" />
                    </div>
                </div>
            </div>

            // Interactive demo
            <div class="story-section">
                <h3>"Interactive Demo"</h3>
                <div class="story-canvas">
                    <ProgressBar
                        value=progress
                        label="Adjustable Progress"
                        show_percentage=true
                    />
                    <div style="display: flex; gap: 0.5rem; margin-top: 1rem;">
                        <button class="btn btn--secondary btn--sm" on:click=move |_| set_progress.set(0.0)>"0%"</button>
                        <button class="btn btn--secondary btn--sm" on:click=move |_| set_progress.update(|p| *p = (*p - 0.1).max(0.0))>"-10%"</button>
                        <button class="btn btn--secondary btn--sm" on:click=move |_| set_progress.update(|p| *p = (*p + 0.1).min(1.0))>"+10%"</button>
                        <button class="btn btn--secondary btn--sm" on:click=move |_| set_progress.set(1.0)>"100%"</button>
                    </div>
                </div>
            </div>

            // Use cases
            <div class="story-section">
                <h3>"Use Cases"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1.5rem;">
                        <div>
                            <p style="margin: 0 0 0.5rem; color: #ccc;">"Health"</p>
                            <ProgressBar value=Signal::derive(|| 0.8) label="80 / 100 HP" color="#28a745" />
                        </div>
                        <div>
                            <p style="margin: 0 0 0.5rem; color: #ccc;">"Experience"</p>
                            <ProgressBar value=Signal::derive(|| 0.45) label="4,500 / 10,000 XP" color="#9b59b6" />
                        </div>
                        <div>
                            <p style="margin: 0 0 0.5rem; color: #ccc;">"Cargo Capacity"</p>
                            <ProgressBar value=Signal::derive(|| 0.92) label="92% Full" color="#ffc107" />
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
                            name="value"
                            values="Signal<f32>"
                            description="Progress value from 0.0 to 1.0"
                        />
                        <AttributeCard
                            name="label"
                            values="String (optional)"
                            description="Text label shown above the bar"
                        />
                        <AttributeCard
                            name="color"
                            values="String (optional)"
                            description="CSS color for the filled portion"
                        />
                        <AttributeCard
                            name="show_percentage"
                            values="bool"
                            description="Whether to show percentage text"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::ProgressBar;

// Basic progress bar
view! { <ProgressBar value=Signal::derive(|| 0.5) /> }

// With label
view! {
    <ProgressBar
        value=Signal::derive(|| 0.75)
        label="75% Complete"
    />
}

// Custom color with percentage
view! {
    <ProgressBar
        value=progress_signal
        label="Health"
        color="#28a745"
        show_percentage=true
    />
}"##}</pre>
            </div>
        </div>
    }
}
