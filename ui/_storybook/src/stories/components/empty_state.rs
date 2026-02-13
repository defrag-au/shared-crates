//! EmptyState component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::EmptyState;

#[component]
pub fn EmptyStateStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Empty State"</h2>
                <p>"A placeholder for when there's no content to display. Supports icon and message."</p>
            </div>

            // Basic examples
            <div class="story-section">
                <h3>"Basic Examples"</h3>
                <div class="story-canvas">
                    <div style="display: grid; grid-template-columns: repeat(2, 1fr); gap: 1rem;">
                        <div style="background: #1a1a2e; padding: 1rem; border-radius: 8px;">
                            <EmptyState message="No items found" />
                        </div>
                        <div style="background: #1a1a2e; padding: 1rem; border-radius: 8px;">
                            <EmptyState
                                message="No ships in your fleet"
                                icon="🚢"
                            />
                        </div>
                    </div>
                </div>
            </div>

            // Various icons
            <div class="story-section">
                <h3>"With Icons"</h3>
                <div class="story-canvas">
                    <div style="display: grid; grid-template-columns: repeat(2, 1fr); gap: 1rem;">
                        <div style="background: #1a1a2e; padding: 1rem; border-radius: 8px;">
                            <EmptyState
                                message="No crew members assigned"
                                icon="👥"
                            />
                        </div>
                        <div style="background: #1a1a2e; padding: 1rem; border-radius: 8px;">
                            <EmptyState
                                message="Your inventory is empty"
                                icon="📦"
                            />
                        </div>
                        <div style="background: #1a1a2e; padding: 1rem; border-radius: 8px;">
                            <EmptyState
                                message="No search results"
                                icon="🔍"
                            />
                        </div>
                        <div style="background: #1a1a2e; padding: 1rem; border-radius: 8px;">
                            <EmptyState
                                message="Wallet not connected"
                                icon="🔗"
                            />
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
                            name="message"
                            values="String"
                            description="The main message to display"
                        />
                        <AttributeCard
                            name="icon"
                            values="String (optional)"
                            description="Emoji or text icon shown above the message"
                        />
                        <AttributeCard
                            name="action"
                            values="Children (optional)"
                            description="Action button or additional content"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::EmptyState;

// Simple empty state
view! {
    <EmptyState message="No items found" />
}

// With icon
view! {
    <EmptyState
        message="No results"
        icon="🔍"
    />
}

// With action button (using action prop)
view! {
    <EmptyState
        message="Your cart is empty"
        icon="🛒"
        action=|| view! {
            <button class="btn btn--primary">
                "Start Shopping"
            </button>
        }
    />
}"##}</pre>
            </div>
        </div>
    }
}
