//! Loading Overlay component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{Button, ButtonVariant, LoadingOverlay, Spinner, SpinnerSize};

#[component]
pub fn LoadingOverlayStory() -> impl IntoView {
    let (show_overlay, set_show_overlay) = signal(false);

    // Auto-hide overlay after 2 seconds when shown
    Effect::new(move || {
        if show_overlay.get() {
            set_timeout(
                move || set_show_overlay.set(false),
                std::time::Duration::from_secs(2),
            );
        }
    });

    view! {
        <div>
            <div class="story-header">
                <h2>"Loading Overlay"</h2>
                <p>"A full-screen loading overlay with spinner and optional message. Also includes an inline Spinner component."</p>
            </div>

            // Full overlay demo
            <div class="story-section">
                <h3>"Full-Screen Overlay"</h3>
                <p class="story-description">"Click the button to show a loading overlay (auto-hides after 2 seconds)."</p>
                <div class="story-canvas">
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=move || set_show_overlay.set(true)
                    >
                        "Show Loading Overlay"
                    </Button>
                    <LoadingOverlay
                        loading=show_overlay
                        message="Loading your data..."
                    />
                </div>
            </div>

            // Inline spinners
            <div class="story-section">
                <h3>"Inline Spinners"</h3>
                <p class="story-description">"Use the Spinner component inline within content."</p>
                <div class="story-canvas">
                    <div style="display: flex; align-items: center; gap: 2rem;">
                        <div style="display: flex; flex-direction: column; align-items: center; gap: 0.5rem;">
                            <Spinner size=SpinnerSize::Sm />
                            <span style="font-size: 0.75rem; color: #888;">"Small"</span>
                        </div>
                        <div style="display: flex; flex-direction: column; align-items: center; gap: 0.5rem;">
                            <Spinner size=SpinnerSize::Md />
                            <span style="font-size: 0.75rem; color: #888;">"Medium"</span>
                        </div>
                        <div style="display: flex; flex-direction: column; align-items: center; gap: 0.5rem;">
                            <Spinner size=SpinnerSize::Lg />
                            <span style="font-size: 0.75rem; color: #888;">"Large"</span>
                        </div>
                    </div>
                </div>
            </div>

            // Spinner with text
            <div class="story-section">
                <h3>"Spinner with Text"</h3>
                <p class="story-description">"Combine spinner with loading text for inline loading states."</p>
                <div class="story-canvas">
                    <div style="display: flex; align-items: center; gap: 0.75rem; padding: 1rem; background: rgba(255,255,255,0.05); border-radius: 6px; width: fit-content;">
                        <Spinner size=SpinnerSize::Sm />
                        <span>"Saving changes..."</span>
                    </div>
                </div>
            </div>

            // Button with spinner
            <div class="story-section">
                <h3>"Loading Button Pattern"</h3>
                <p class="story-description">"Common pattern: button with inline spinner during async operations."</p>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1rem;">
                        <button
                            style="display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem 1rem; background: #2563eb; border: none; border-radius: 6px; color: white; cursor: pointer;"
                            disabled
                        >
                            <Spinner size=SpinnerSize::Sm />
                            "Submitting..."
                        </button>
                        <button
                            style="padding: 0.5rem 1rem; background: #2563eb; border: none; border-radius: 6px; color: white; cursor: pointer;"
                        >
                            "Submit"
                        </button>
                    </div>
                </div>
            </div>

            // Props section - LoadingOverlay
            <div class="story-section">
                <h3>"LoadingOverlay Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="loading"
                            values="Signal<bool>"
                            description="Signal controlling whether the overlay is visible"
                        />
                        <AttributeCard
                            name="message"
                            values="String (optional)"
                            description="Loading message to display. Defaults to 'Loading...'"
                        />
                        <AttributeCard
                            name="children"
                            values="Children (optional)"
                            description="Content behind the overlay"
                        />
                    </div>
                </div>
            </div>

            // Props section - Spinner
            <div class="story-section">
                <h3>"Spinner Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="size"
                            values="SpinnerSize (Sm, Md, Lg)"
                            description="Size variant for the spinner. Defaults to Md."
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{LoadingOverlay, Spinner, SpinnerSize};

// Full-screen loading overlay
let (is_loading, set_loading) = signal(false);

view! {
    <LoadingOverlay
        loading=is_loading
        message="Loading data..."
    />
}

// Inline spinner
view! {
    <div style="display: flex; align-items: center; gap: 0.5rem;">
        <Spinner size=SpinnerSize::Sm />
        <span>"Processing..."</span>
    </div>
}

// Spinner sizes
view! {
    <Spinner size=SpinnerSize::Sm />  // 1rem
    <Spinner size=SpinnerSize::Md />  // 1.5rem (default)
    <Spinner size=SpinnerSize::Lg />  // 2.5rem
}"##}</pre>
            </div>
        </div>
    }
}
