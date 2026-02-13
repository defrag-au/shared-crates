//! Welcome story - landing page for the storybook

use leptos::prelude::*;

#[component]
pub fn WelcomeStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Welcome to Shared UI"</h2>
                <p>"A collection of reusable Rust/WASM UI components for Cardano applications, built with Leptos."</p>
            </div>

            <div class="story-section">
                <h3>"Included Crates"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <FeatureCard
                            title="ui-components"
                            description="Leptos components for NFT asset display: ImageCard, AssetCard, MemoryCard, ConnectionStatus."
                        />
                        <FeatureCard
                            title="wallet-core"
                            description="CIP-30 wallet integration for Cardano wallets (Nami, Eternl, Lace, etc.)."
                        />
                        <FeatureCard
                            title="ui-flow"
                            description="WebSocket connection management with state synchronization and optimistic updates."
                        />
                        <FeatureCard
                            title="ui-loader"
                            description="Loading state management and skeleton UI components."
                        />
                        <FeatureCard
                            title="ui-toast"
                            description="Toast notification system for user feedback."
                        />
                    </div>
                </div>
            </div>
        </div>
    }
}

#[component]
fn FeatureCard(title: &'static str, description: &'static str) -> impl IntoView {
    view! {
        <div class="feature-card">
            <h4>{title}</h4>
            <p>{description}</p>
        </div>
    }
}
