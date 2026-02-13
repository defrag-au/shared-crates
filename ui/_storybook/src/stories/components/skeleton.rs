//! Skeleton component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{Skeleton, SkeletonVariant};

#[component]
pub fn SkeletonStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Skeleton"</h2>
                <p>"Loading placeholder shapes with shimmer animation. Use to indicate content that is loading."</p>
            </div>

            // Text skeleton
            <div class="story-section">
                <h3>"Text Skeleton"</h3>
                <p class="story-description">"Placeholder for text content. Supports multiple lines."</p>
                <div class="story-canvas">
                    <div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 2rem;">
                        <div>
                            <p style="font-size: 0.75rem; color: #888; margin-bottom: 0.5rem;">"Single line"</p>
                            <Skeleton variant=SkeletonVariant::Text />
                        </div>
                        <div>
                            <p style="font-size: 0.75rem; color: #888; margin-bottom: 0.5rem;">"Two lines"</p>
                            <Skeleton variant=SkeletonVariant::Text lines=2 />
                        </div>
                        <div>
                            <p style="font-size: 0.75rem; color: #888; margin-bottom: 0.5rem;">"Three lines"</p>
                            <Skeleton variant=SkeletonVariant::Text lines=3 />
                        </div>
                    </div>
                </div>
            </div>

            // Circle skeleton
            <div class="story-section">
                <h3>"Circle Skeleton"</h3>
                <p class="story-description">"Placeholder for avatars and circular images."</p>
                <div class="story-canvas">
                    <div style="display: flex; gap: 2rem; align-items: center;">
                        <div style="text-align: center;">
                            <Skeleton variant=SkeletonVariant::Circle width="32px" />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"32px"</p>
                        </div>
                        <div style="text-align: center;">
                            <Skeleton variant=SkeletonVariant::Circle width="48px" />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"48px"</p>
                        </div>
                        <div style="text-align: center;">
                            <Skeleton variant=SkeletonVariant::Circle width="64px" />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"64px"</p>
                        </div>
                        <div style="text-align: center;">
                            <Skeleton variant=SkeletonVariant::Circle width="96px" />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"96px"</p>
                        </div>
                    </div>
                </div>
            </div>

            // Rectangle skeleton
            <div class="story-section">
                <h3>"Rectangle Skeleton"</h3>
                <p class="story-description">"Placeholder for images and rectangular content."</p>
                <div class="story-canvas">
                    <div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem;">
                        <Skeleton variant=SkeletonVariant::Rect height="80px" />
                        <Skeleton variant=SkeletonVariant::Rect height="120px" />
                        <Skeleton variant=SkeletonVariant::Rect height="160px" />
                    </div>
                </div>
            </div>

            // Card skeleton
            <div class="story-section">
                <h3>"Card Skeleton"</h3>
                <p class="story-description">"Complete card placeholder with image and text areas."</p>
                <div class="story-canvas">
                    <div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem; max-width: 600px;">
                        <Skeleton variant=SkeletonVariant::Card />
                        <Skeleton variant=SkeletonVariant::Card />
                        <Skeleton variant=SkeletonVariant::Card />
                    </div>
                </div>
            </div>

            // Composite example
            <div class="story-section">
                <h3>"Composite Layout"</h3>
                <p class="story-description">"Combine skeletons to create realistic loading states."</p>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1rem; padding: 1rem; background: rgba(255,255,255,0.02); border-radius: 8px; max-width: 400px;">
                        <Skeleton variant=SkeletonVariant::Circle width="48px" />
                        <div style="flex: 1;">
                            <Skeleton variant=SkeletonVariant::Text lines=1 />
                            <div style="margin-top: 0.25rem;">
                                <Skeleton variant=SkeletonVariant::Text width="60%" />
                            </div>
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
                            name="variant"
                            values="SkeletonVariant (Text, Circle, Rect, Card)"
                            description="Shape variant. Defaults to Text."
                        />
                        <AttributeCard
                            name="width"
                            values="String (CSS value, optional)"
                            description="Custom width. For Circle, this sets both width and height."
                        />
                        <AttributeCard
                            name="height"
                            values="String (CSS value, optional)"
                            description="Custom height. Used mainly for Rect variant."
                        />
                        <AttributeCard
                            name="lines"
                            values="usize (optional)"
                            description="Number of text lines for Text variant. Last line is shorter."
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{Skeleton, SkeletonVariant};

// Text placeholder (multiple lines)
view! {
    <Skeleton variant=SkeletonVariant::Text lines=3 />
}

// Avatar placeholder
view! {
    <Skeleton variant=SkeletonVariant::Circle width="48px" />
}

// Image placeholder
view! {
    <Skeleton variant=SkeletonVariant::Rect width="200px" height="120px" />
}

// Card placeholder
view! {
    <Skeleton variant=SkeletonVariant::Card />
}

// Composite loading state
view! {
    <div style="display: flex; gap: 1rem;">
        <Skeleton variant=SkeletonVariant::Circle width="48px" />
        <div>
            <Skeleton variant=SkeletonVariant::Text lines=2 />
        </div>
    </div>
}"##}</pre>
            </div>
        </div>
    }
}
