//! PageHeader component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::PageHeader;

#[component]
pub fn HeaderStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Page Header"</h2>
                <p>"A page header component with title and optional subtitle."</p>
            </div>

            // Basic examples
            <div class="story-section">
                <h3>"Examples"</h3>
                <div class="story-canvas" style="display: flex; flex-direction: column; gap: 1.5rem;">
                    <PageHeader title="Simple Title" />

                    <PageHeader
                        title="With Subtitle"
                        subtitle="A helpful description of this page"
                    />

                    <PageHeader
                        title="Dashboard"
                        subtitle="View your statistics and manage settings"
                    />
                </div>
            </div>

            // Various contexts
            <div class="story-section">
                <h3>"Different Contexts"</h3>
                <div class="story-canvas" style="display: flex; flex-direction: column; gap: 1.5rem;">
                    <PageHeader
                        title="Fleet Management"
                        subtitle="Configure and deploy your ships"
                    />

                    <PageHeader
                        title="Settings"
                        subtitle="Manage your account and preferences"
                    />

                    <PageHeader
                        title="NFT Collection"
                        subtitle="Browse and manage your digital assets"
                    />
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="title"
                            values="String (optional)"
                            description="Main heading text"
                        />
                        <AttributeCard
                            name="subtitle"
                            values="String (optional)"
                            description="Secondary descriptive text below the title"
                        />
                        <AttributeCard
                            name="actions"
                            values="Children (optional)"
                            description="Action buttons or other content aligned to the right"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::PageHeader;

// Simple header
view! {
    <PageHeader title="My Page" />
}

// With subtitle
view! {
    <PageHeader
        title="Settings"
        subtitle="Manage your preferences"
    />
}

// With action buttons (using actions prop)
view! {
    <PageHeader
        title="Users"
        subtitle="Manage team members"
        actions=|| view! {
            <button class="btn btn--primary">"Add User"</button>
        }
    />
}"##}</pre>
            </div>
        </div>
    }
}
