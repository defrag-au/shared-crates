//! Card component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::Card;

#[component]
pub fn CardStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Card"</h2>
                <p>"A generic card container with optional accent bar. Use for grouping related content with consistent styling."</p>
            </div>

            // Examples section
            <div class="story-section">
                <h3>"Examples"</h3>
                <div class="story-canvas">
                    <div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem;">
                        <Card>
                            <p>"Basic card with default styling"</p>
                        </Card>
                        <Card accent_color="#FFD700">
                            <p>"Card with gold accent bar"</p>
                        </Card>
                        <Card accent_color="#dc3545">
                            <p>"Card with danger accent"</p>
                        </Card>
                    </div>
                </div>
            </div>

            // Custom class section
            <div class="story-section">
                <h3>"Custom Classes"</h3>
                <div class="story-canvas">
                    <Card class="custom-card-class">
                        <p>"Card with custom CSS class applied"</p>
                    </Card>
                </div>
            </div>

            // Nested content
            <div class="story-section">
                <h3>"Rich Content"</h3>
                <div class="story-canvas">
                    <Card accent_color="#28a745">
                        <h4 style="margin: 0 0 0.5rem 0;">"Card Title"</h4>
                        <p style="margin: 0 0 1rem 0; color: #aaa;">"Cards can contain any content including headers, paragraphs, lists, and other components."</p>
                        <ul style="margin: 0; padding-left: 1.2rem;">
                            <li>"Item one"</li>
                            <li>"Item two"</li>
                            <li>"Item three"</li>
                        </ul>
                    </Card>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="class"
                            values="String (optional)"
                            description="Additional CSS classes to apply to the card"
                        />
                        <AttributeCard
                            name="accent_color"
                            values="String (CSS color, optional)"
                            description="Color for the top accent bar. If not provided, no accent is shown."
                        />
                        <AttributeCard
                            name="children"
                            values="Children"
                            description="Content to render inside the card"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::Card;

// Basic card
view! {
    <Card>
        <p>"Card content goes here"</p>
    </Card>
}

// Card with accent color
view! {
    <Card accent_color="#FFD700">
        <h3>"Featured Content"</h3>
        <p>"Important information highlighted with accent"</p>
    </Card>
}

// Card with custom class
view! {
    <Card class="my-special-card">
        <p>"Styled with custom CSS"</p>
    </Card>
}"##}</pre>
            </div>
        </div>
    }
}
