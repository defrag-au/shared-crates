//! Rating component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{Rating, RatingSize};

#[component]
pub fn RatingStory() -> impl IntoView {
    let (interactive_value, set_interactive_value) = signal(3u32);

    view! {
        <div>
            <div class="story-header">
                <h2>"Rating"</h2>
                <p>"A visual rating display using repeated icons/emojis. Great for star ratings, difficulty indicators, etc."</p>
            </div>

            // Basic examples
            <div class="story-section">
                <h3>"Basic Examples"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem;">
                        <div style="display: flex; align-items: center; gap: 1rem;">
                            <span style="width: 100px; color: #888;">"1 of 5:"</span>
                            <Rating value=Signal::derive(|| 1u32) max=5 />
                        </div>
                        <div style="display: flex; align-items: center; gap: 1rem;">
                            <span style="width: 100px; color: #888;">"3 of 5:"</span>
                            <Rating value=Signal::derive(|| 3u32) max=5 />
                        </div>
                        <div style="display: flex; align-items: center; gap: 1rem;">
                            <span style="width: 100px; color: #888;">"5 of 5:"</span>
                            <Rating value=Signal::derive(|| 5u32) max=5 />
                        </div>
                    </div>
                </div>
            </div>

            // With empty icons
            <div class="story-section">
                <h3>"With Empty Icons"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem;">
                        <div style="display: flex; align-items: center; gap: 1rem;">
                            <span style="width: 80px; color: #888;">"Stars:"</span>
                            <Rating value=Signal::derive(|| 3u32) max=5 icon="⭐" empty_icon="☆" />
                        </div>
                        <div style="display: flex; align-items: center; gap: 1rem;">
                            <span style="width: 80px; color: #888;">"Hearts:"</span>
                            <Rating value=Signal::derive(|| 4u32) max=5 icon="❤️" empty_icon="🤍" />
                        </div>
                        <div style="display: flex; align-items: center; gap: 1rem;">
                            <span style="width: 80px; color: #888;">"Circles:"</span>
                            <Rating value=Signal::derive(|| 2u32) max=5 icon="●" empty_icon="○" color="#FFD700" />
                        </div>
                    </div>
                </div>
            </div>

            // Difficulty indicators (no empty icons)
            <div class="story-section">
                <h3>"Difficulty Indicators"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 0.75rem;">
                        <div style="display: flex; align-items: center; gap: 1rem;">
                            <span style="width: 100px; color: #888;">"Easy:"</span>
                            <Rating value=Signal::derive(|| 1u32) max=5 icon="💀" />
                        </div>
                        <div style="display: flex; align-items: center; gap: 1rem;">
                            <span style="width: 100px; color: #888;">"Medium:"</span>
                            <Rating value=Signal::derive(|| 3u32) max=5 icon="💀" />
                        </div>
                        <div style="display: flex; align-items: center; gap: 1rem;">
                            <span style="width: 100px; color: #888;">"Hard:"</span>
                            <Rating value=Signal::derive(|| 5u32) max=5 icon="💀" />
                        </div>
                    </div>
                </div>
            </div>

            // Sizes
            <div class="story-section">
                <h3>"Sizes"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem;">
                        <div style="display: flex; align-items: center; gap: 1rem;">
                            <span style="width: 80px; color: #888;">"Small:"</span>
                            <Rating value=Signal::derive(|| 3u32) max=5 icon="⭐" empty_icon="☆" size=RatingSize::Sm />
                        </div>
                        <div style="display: flex; align-items: center; gap: 1rem;">
                            <span style="width: 80px; color: #888;">"Medium:"</span>
                            <Rating value=Signal::derive(|| 3u32) max=5 icon="⭐" empty_icon="☆" size=RatingSize::Md />
                        </div>
                        <div style="display: flex; align-items: center; gap: 1rem;">
                            <span style="width: 80px; color: #888;">"Large:"</span>
                            <Rating value=Signal::derive(|| 3u32) max=5 icon="⭐" empty_icon="☆" size=RatingSize::Lg />
                        </div>
                    </div>
                </div>
            </div>

            // Custom colors
            <div class="story-section">
                <h3>"Custom Colors"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem;">
                        <Rating value=Signal::derive(|| 4u32) max=5 icon="●" empty_icon="○" color="#FFD700" />
                        <Rating value=Signal::derive(|| 3u32) max=5 icon="●" empty_icon="○" color="#28a745" />
                        <Rating value=Signal::derive(|| 2u32) max=5 icon="●" empty_icon="○" color="#dc3545" />
                    </div>
                </div>
            </div>

            // Interactive example
            <div class="story-section">
                <h3>"Reactive Value"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem;">
                        <Rating value=interactive_value max=5 icon="⭐" empty_icon="☆" size=RatingSize::Lg />
                        <div style="display: flex; gap: 0.5rem;">
                            {(1..=5).map(|n| view! {
                                <button
                                    style="padding: 0.25rem 0.75rem; background: #2a2a4e; border: 1px solid #3a3a5e; border-radius: 4px; color: #e0e0e0; cursor: pointer;"
                                    on:click=move |_| set_interactive_value.set(n)
                                >
                                    {n}
                                </button>
                            }).collect::<Vec<_>>()}
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
                            values="Signal<u32>"
                            description="Current rating value"
                        />
                        <AttributeCard
                            name="max"
                            values="u32 (default: 5)"
                            description="Maximum rating (total icons)"
                        />
                        <AttributeCard
                            name="icon"
                            values="String (default: ★)"
                            description="Filled icon/emoji"
                        />
                        <AttributeCard
                            name="empty_icon"
                            values="String (optional)"
                            description="Empty icon (if not set, only filled shown)"
                        />
                        <AttributeCard
                            name="size"
                            values="RatingSize (Sm|Md|Lg)"
                            description="Display size - default is Md"
                        />
                        <AttributeCard
                            name="color"
                            values="String (optional)"
                            description="Icon color (CSS color value)"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{Rating, RatingSize};

// Star rating (3 out of 5)
view! {
    <Rating
        value=Signal::derive(|| 3u32)
        max=5
        icon="⭐"
        empty_icon="☆"
    />
}

// Difficulty skulls (no empty icons)
view! {
    <Rating
        value=difficulty
        max=5
        icon="💀"
    />
}

// Custom colored dots
view! {
    <Rating
        value=score
        max=5
        icon="●"
        empty_icon="○"
        color="#FFD700"
        size=RatingSize::Lg
    />
}"##}</pre>
            </div>
        </div>
    }
}
