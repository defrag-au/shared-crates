//! InfoGrid component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{Badge, InfoGrid, InfoRow};

#[component]
pub fn InfoGridStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"InfoGrid"</h2>
                <p>"A key-value grid for displaying structured information."</p>
            </div>

            // Basic example
            <div class="story-section">
                <h3>"Basic Example"</h3>
                <div class="story-canvas">
                    <div style="max-width: 350px; background: #1a1a2e; padding: 1rem; border-radius: 8px;">
                        <InfoGrid>
                            <InfoRow label="Name" value="Captain Jack Sparrow" />
                            <InfoRow label="Rank" value="Admiral" />
                            <InfoRow label="Ship" value="Black Pearl" />
                            <InfoRow label="Crew Size" value="42" />
                        </InfoGrid>
                    </div>
                </div>
            </div>

            // With custom content
            <div class="story-section">
                <h3>"With Custom Content"</h3>
                <div class="story-canvas">
                    <div style="max-width: 350px; background: #1a1a2e; padding: 1rem; border-radius: 8px;">
                        <InfoGrid>
                            <InfoRow label="Status">
                                <Badge label="Active" color="#28a745" />
                            </InfoRow>
                            <InfoRow label="Roles">
                                <Badge label="Captain" color="#FFD700" />
                                <Badge label="Navigator" color="#17a2b8" />
                            </InfoRow>
                            <InfoRow label="Power" value="285" />
                            <InfoRow label="Location" value="Port Royal" muted=true />
                        </InfoGrid>
                    </div>
                </div>
            </div>

            // Entity details example
            <div class="story-section">
                <h3>"Entity Details Example"</h3>
                <div class="story-canvas">
                    <div style="max-width: 400px; background: #1a1a2e; padding: 1rem; border-radius: 8px;">
                        <h4 style="margin: 0 0 0.75rem; color: #e0e0e0;">"Ship Details"</h4>
                        <InfoGrid>
                            <InfoRow label="Class" value="Galleon" />
                            <InfoRow label="Speed" value="12 knots" />
                            <InfoRow label="Cargo Capacity" value="500 tons" />
                            <InfoRow label="Cannons" value="32" />
                            <InfoRow label="Condition">
                                <Badge label="Good" color="#28a745" />
                            </InfoRow>
                            <InfoRow label="Last Maintenance" value="2 weeks ago" muted=true />
                        </InfoGrid>
                    </div>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <h4 style="margin: 0 0 0.5rem; color: #e0e0e0;">"InfoGrid"</h4>
                    <div class="story-grid" style="margin-bottom: 1.5rem;">
                        <AttributeCard
                            name="children"
                            values="Children"
                            description="InfoRow components"
                        />
                        <AttributeCard
                            name="class"
                            values="String (optional)"
                            description="Additional CSS class"
                        />
                    </div>

                    <h4 style="margin: 0 0 0.5rem; color: #e0e0e0;">"InfoRow"</h4>
                    <div class="story-grid">
                        <AttributeCard
                            name="label"
                            values="String"
                            description="Row label (left side)"
                        />
                        <AttributeCard
                            name="value"
                            values="String (optional)"
                            description="Simple text value (right side)"
                        />
                        <AttributeCard
                            name="children"
                            values="Children (optional)"
                            description="Custom content (overrides value)"
                        />
                        <AttributeCard
                            name="muted"
                            values="bool (default: false)"
                            description="Show row in muted/secondary style"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{InfoGrid, InfoRow, Badge};

// Simple key-value pairs
view! {
    <InfoGrid>
        <InfoRow label="Name" value="Captain Jack" />
        <InfoRow label="Ship" value="Black Pearl" />
    </InfoGrid>
}

// With custom content
view! {
    <InfoGrid>
        <InfoRow label="Status">
            <Badge label="Active" color="#28a745" />
        </InfoRow>
        <InfoRow label="Notes" value="Optional info" muted=true />
    </InfoGrid>
}"##}</pre>
            </div>
        </div>
    }
}
