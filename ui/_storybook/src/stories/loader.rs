//! UI Loader stories - loading states and configuration

use super::helpers::{ConfigOptionCard, LoaderErrorCard, LoaderStepCard};
use leptos::prelude::*;

// ============================================================================
// Loading States Story
// ============================================================================

#[component]
pub fn LoadingStatesStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Loading States"</h2>
                <p>"The ui-loader crate provides a framework-agnostic loading orchestrator for WASM widgets."</p>
            </div>

            <div class="story-section">
                <h3>"Loading Flow"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <LoaderStepCard
                            step="1"
                            title="Show Loading"
                            description="Immediately display a loading screen with customizable message"
                        />
                        <LoaderStepCard
                            step="2"
                            title="Validate Auth"
                            description="Parse URL token and validate JWT authentication"
                        />
                        <LoaderStepCard
                            step="3"
                            title="Fetch Data"
                            description="Run async load function with progress updates"
                        />
                        <LoaderStepCard
                            step="4"
                            title="Hand Off"
                            description="Pass loaded data to your framework (Seed, Leptos, etc.)"
                        />
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Error States"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <LoaderErrorCard
                            error_type="AuthRequired"
                            description="Missing or invalid authentication token"
                        />
                        <LoaderErrorCard
                            error_type="TokenExpired"
                            description="JWT token has passed its expiration time"
                        />
                        <LoaderErrorCard
                            error_type="FetchFailed"
                            description="Network error or API returned an error status"
                        />
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r#"use ui_loader::{LoadingOrchestrator, LoaderConfig};

#[wasm_bindgen(start)]
pub async fn start() {
    let config = LoaderConfig::new()
        .auth_required(true)
        .initial_message("Loading...");

    let result = LoadingOrchestrator::run(config, |auth, loader| async move {
        loader.set_message("Fetching data...");
        let data = fetch_data(&auth).await?;
        Ok(data)
    }).await;

    match result {
        Ok(loaded) => {
            // Start your UI framework with loaded.data
        }
        Err(_) => {
            // Error screen already shown
        }
    }
}"#}</pre>
            </div>
        </div>
    }
}

// ============================================================================
// Loader Config Story
// ============================================================================

#[component]
pub fn LoaderConfigStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Loader Configuration"</h2>
                <p>"Configure the loading orchestrator behavior with LoaderConfig."</p>
            </div>

            <div class="story-section">
                <h3>"Configuration Options"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <ConfigOptionCard
                            name="auth_required"
                            type_name="bool"
                            default="true"
                            description="Whether authentication is required. If true, shows error for missing/invalid tokens."
                        />
                        <ConfigOptionCard
                            name="log_level"
                            type_name="Level"
                            default="DEBUG"
                            description="Tracing log level for the widget runtime (DEBUG, INFO, WARN, ERROR)."
                        />
                        <ConfigOptionCard
                            name="initial_message"
                            type_name="String"
                            default="\"Loading...\""
                            description="The message shown on the loading screen before fetch starts."
                        />
                        <ConfigOptionCard
                            name="on_before_load"
                            type_name="fn()"
                            default="None"
                            description="Optional hook called after auth validation, before the load function runs."
                        />
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"LoadResult Fields"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <ConfigOptionCard
                            name="auth"
                            type_name="AuthState"
                            default="-"
                            description="The validated authentication state (Authenticated, Anonymous, etc.)"
                        />
                        <ConfigOptionCard
                            name="data"
                            type_name="T"
                            default="-"
                            description="Your loaded data returned from the fetch function"
                        />
                        <ConfigOptionCard
                            name="world_id"
                            type_name="Option<String>"
                            default="-"
                            description="World ID extracted from URL ?world= parameter"
                        />
                        <ConfigOptionCard
                            name="discord_url"
                            type_name="Option<String>"
                            default="-"
                            description="Return URL from JWT claims for 'Return to Discord' button"
                        />
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Builder Pattern"</h3>
                <pre class="code-block">{r#"use ui_loader::{LoaderConfig, Level};

let config = LoaderConfig::new()
    .auth_required(true)
    .log_level(Level::INFO)
    .initial_message("Starting up...")
    .on_before_load(|| {
        // Register custom elements, etc.
    });"#}</pre>
            </div>
        </div>
    }
}
