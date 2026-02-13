//! DropEditor component story

use crate::stories::helpers::AttributeCard;
use asset_intents::{AssetId, Drop};
use leptos::prelude::*;
use ui_components::{CardSize, DropEditor};

/// Sample asset IDs for the story
fn sample_asset_id() -> AssetId {
    AssetId::new_unchecked(
        "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string(),
        "50697261746531303836".to_string(), // Pirate1086
    )
}

fn sample_asset_id_2() -> AssetId {
    AssetId::new_unchecked(
        "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string(),
        "506972617465313839".to_string(), // Pirate189
    )
}

fn sample_asset_id_3() -> AssetId {
    AssetId::new_unchecked(
        "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string(),
        "5069726174653432".to_string(), // Pirate42
    )
}

#[component]
pub fn DropEditorStory() -> impl IntoView {
    // Basic interactive example
    let (drops, set_drops) = signal(vec![
        Drop::tip("ADA", 500.0),
        Drop::tip("ADA", 250.0),
        Drop::wallet_send_single(sample_asset_id()),
        Drop::wallet_send_single(sample_asset_id_2()),
    ]);

    // Readonly example
    let readonly_drops = Signal::derive(move || {
        vec![
            Drop::tip("SNEK", 1000.0),
            Drop::wallet_send_single(sample_asset_id_3()),
        ]
    });

    // Empty example
    let (empty_drops, set_empty_drops) = signal(Vec::<Drop>::new());

    // Small size example
    let (small_drops, set_small_drops) = signal(vec![
        Drop::tip("ADA", 100.0),
        Drop::tip("ADA", 50.0),
        Drop::tip("ADA", 25.0),
    ]);

    view! {
        <div>
            <div class="story-header">
                <h2>"DropEditor"</h2>
                <p>"A compact horizontal editor for configuring reward drops (tips and wallet sends). Features drag-and-drop reordering and a modal for adding new prizes."</p>
            </div>

            // Basic interactive example
            <div class="story-section">
                <h3>"Interactive Example"</h3>
                <p style="color: #888; margin-bottom: 1rem; font-size: 0.875rem;">
                    "Drag items to reorder. Hover to see remove button. Click + to add new prizes."
                </p>
                <div class="story-canvas">
                    <DropEditor
                        drops=drops
                        on_change=Callback::new(move |new_drops| {
                            web_sys::console::log_1(&format!("Drops changed: {:?}", new_drops).into());
                            set_drops.set(new_drops);
                        })
                    />
                </div>
                <div style="margin-top: 1rem; padding: 0.75rem; background: #1a1a2e; border-radius: 4px;">
                    <p style="font-size: 0.75rem; color: #888; margin-bottom: 0.5rem;">"Current state:"</p>
                    <pre style="font-size: 0.75rem; color: #888; margin: 0; white-space: pre-wrap;">
                        {move || format!("{:#?}", drops.get())}
                    </pre>
                </div>
            </div>

            // Readonly example
            <div class="story-section">
                <h3>"Readonly Mode"</h3>
                <p style="color: #888; margin-bottom: 1rem; font-size: 0.875rem;">
                    "When readonly is true, add/remove/reorder controls are hidden."
                </p>
                <div class="story-canvas">
                    <DropEditor
                        drops=readonly_drops
                        on_change=Callback::new(move |_| {})
                        readonly=true
                    />
                </div>
            </div>

            // Empty state
            <div class="story-section">
                <h3>"Empty State"</h3>
                <p style="color: #888; margin-bottom: 1rem; font-size: 0.875rem;">
                    "Shows an add button and placeholder when no drops are configured."
                </p>
                <div class="story-canvas">
                    <DropEditor
                        drops=empty_drops
                        on_change=Callback::new(move |new_drops| set_empty_drops.set(new_drops))
                    />
                </div>
            </div>

            // Small size example
            <div class="story-section">
                <h3>"Card Sizes"</h3>
                <p style="color: #888; margin-bottom: 1rem; font-size: 0.875rem;">
                    "The size prop controls the card dimensions. Default is Sm (120px)."
                </p>
                <div class="story-canvas" style="display: flex; flex-direction: column; gap: 1rem;">
                    <div>
                        <p style="margin: 0 0 0.5rem; font-size: 0.75rem; color: #888;">"Xs (80px)"</p>
                        <DropEditor
                            drops=small_drops
                            on_change=Callback::new(move |new_drops| set_small_drops.set(new_drops))
                            size=CardSize::Xs
                        />
                    </div>
                    <div>
                        <p style="margin: 0 0 0.5rem; font-size: 0.75rem; color: #888;">"Sm (120px) - default"</p>
                        <DropEditor
                            drops=drops
                            on_change=Callback::new(move |new_drops| set_drops.set(new_drops))
                            size=CardSize::Sm
                        />
                    </div>
                </div>
            </div>

            // Features
            <div class="story-section">
                <h3>"Features"</h3>
                <div class="story-canvas">
                    <ul style="margin: 0; padding-left: 1.5rem; color: #888; font-size: 0.875rem;">
                        <li style="margin-bottom: 0.5rem;">
                            <strong style="color: #fff;">"Horizontal layout"</strong>
                            " - Compact display as a row of cards"
                        </li>
                        <li style="margin-bottom: 0.5rem;">
                            <strong style="color: #fff;">"Drag and drop"</strong>
                            " - Reorder prizes by dragging"
                        </li>
                        <li style="margin-bottom: 0.5rem;">
                            <strong style="color: #fff;">"Position numbers"</strong>
                            " - Each item shows its position (1st, 2nd, 3rd...)"
                        </li>
                        <li style="margin-bottom: 0.5rem;">
                            <strong style="color: #fff;">"Skeleton loading"</strong>
                            " - NFT images show skeleton while loading from IIIF"
                        </li>
                        <li style="margin-bottom: 0.5rem;">
                            <strong style="color: #fff;">"Modal for adding"</strong>
                            " - Click + button to add tips or CNFTs via modal"
                        </li>
                        <li>
                            <strong style="color: #fff;">"Hover to remove"</strong>
                            " - Remove button appears on hover"
                        </li>
                    </ul>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="drops"
                            values="Signal<Vec<Drop>>"
                            description="The current list of drops to display/edit"
                        />
                        <AttributeCard
                            name="on_change"
                            values="Callback<Vec<Drop>>"
                            description="Called when drops are modified (add, remove, reorder)"
                        />
                        <AttributeCard
                            name="readonly"
                            values="Signal<bool> (optional)"
                            description="If true, disables all editing controls"
                        />
                        <AttributeCard
                            name="size"
                            values="CardSize (Xs|Sm|Md|Lg)"
                            description="Card size for drop items. Default: Sm (120px)"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{DropEditor, CardSize};
use asset_intents::{Drop, AssetId};

let (drops, set_drops) = signal(vec![
    Drop::tip("ADA", 500.0),
    Drop::tip("ADA", 250.0),
    Drop::wallet_send_single(my_asset_id),
]);

view! {
    <DropEditor
        drops=drops
        on_change=move |new_drops| set_drops.set(new_drops)
    />
}

// Smaller cards
view! {
    <DropEditor
        drops=drops
        on_change=move |new_drops| set_drops.set(new_drops)
        size=CardSize::Xs
    />
}

// Readonly mode
view! {
    <DropEditor
        drops=drops
        on_change=move |_| {}
        readonly=true
    />
}"##}</pre>
            </div>

            // Drop types
            <div class="story-section">
                <h3>"Drop Types"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem;">
                        <div>
                            <h4 style="margin: 0 0 0.5rem; font-size: 0.875rem; color: #fff;">"Tip"</h4>
                            <p style="margin: 0 0 0.5rem; font-size: 0.75rem; color: #888;">
                                "Fungible token tips via tipping services (e.g., FarmBot). Displays as a green card with token amount."
                            </p>
                            <pre class="code-block" style="margin: 0; font-size: 0.75rem;">
                                {"Drop::tip(\"ADA\", 100.0)"}
                            </pre>
                        </div>
                        <div>
                            <h4 style="margin: 0 0 0.5rem; font-size: 0.875rem; color: #fff;">"WalletSend"</h4>
                            <p style="margin: 0 0 0.5rem; font-size: 0.75rem; color: #888;">
                                "NFT transfers via wallet services. Displays as an AssetCard with the NFT image."
                            </p>
                            <pre class="code-block" style="margin: 0; font-size: 0.75rem;">
                                {"Drop::wallet_send(asset_id, 1)\nDrop::wallet_send_single(asset_id)"}
                            </pre>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
