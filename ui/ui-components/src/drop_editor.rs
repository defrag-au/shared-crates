//! Drop Editor Component
//!
//! A compact horizontal editor for configuring reward drops (tips and wallet sends).
//! Used for raffle prizes, achievement rewards, giveaways, etc.
//!
//! ## Features
//!
//! - Horizontal stack of AssetCards showing prizes
//! - Drag-and-drop reordering
//! - Modal for adding new drops (tips or CNFTs)
//! - Remove button on hover
//! - Skeleton loading for NFT images
//!
//! # Example
//!
//! ```ignore
//! use ui_components::DropEditor;
//! use asset_intents::Drop;
//!
//! let (drops, set_drops) = signal(vec![
//!     Drop::tip("ADA", 100.0),
//!     Drop::wallet_send_single(asset_id),
//! ]);
//!
//! view! {
//!     <DropEditor
//!         drops=drops
//!         on_change=move |new_drops| set_drops.set(new_drops)
//!     />
//! }
//! ```

use crate::{
    AssetCard, Button, ButtonVariant, CardSize, DraggableStack, ItemDragState, Modal, Reorder,
    Select, SelectOption, StackDirection,
};
use asset_intents::{format_number, AssetId, Drop};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Editor for a list of reward drops - horizontal compact layout
#[component]
pub fn DropEditor(
    /// The current list of drops
    #[prop(into)]
    drops: Signal<Vec<Drop>>,
    /// Called when drops are modified
    #[prop(into)]
    on_change: Callback<Vec<Drop>>,
    /// If true, disables editing (no add/remove/reorder)
    #[prop(into, optional)]
    readonly: Signal<bool>,
    /// Card size for drop items
    #[prop(optional, default = CardSize::Sm)]
    size: CardSize,
) -> impl IntoView {
    // Modal state
    let (show_add_modal, set_show_add_modal) = signal(false);

    // Handle reordering
    let on_reorder = Callback::new(move |reorder: Reorder| {
        let mut new_drops = drops.get();
        reorder.apply(&mut new_drops);
        on_change.run(new_drops);
    });

    // Remove a drop by its key (since indices change during drag)
    let remove_drop = move |drop_to_remove: Drop| {
        let new_drops: Vec<Drop> = drops
            .get()
            .into_iter()
            .filter(|d| drop_key(d) != drop_key(&drop_to_remove))
            .collect();
        on_change.run(new_drops);
    };

    // Add a new drop from modal
    let add_drop = move |drop: Drop| {
        let mut new_drops = drops.get();
        new_drops.push(drop);
        on_change.run(new_drops);
        set_show_add_modal.set(false);
    };

    let btn_size = size.pixels().unwrap_or(120);

    view! {
        <div class="drop-editor">
            <div class="drop-editor__items">
                <Show
                    when=move || !drops.get().is_empty()
                    fallback=|| ()
                >
                    <DraggableStack
                        items=drops
                        on_reorder=on_reorder
                        key_fn=|drop| drop_key(drop)
                        direction=StackDirection::Horizontal
                        gap="0.5rem"
                        disabled=readonly
                        render_item=move |drop: Drop, idx: usize, drag_state: ItemDragState| {
                            let drop_for_remove = drop.clone();
                            view! {
                                <DropItem
                                    index=idx
                                    drop=drop
                                    size=size
                                    readonly=readonly
                                    drag_state=drag_state
                                    on_remove=Callback::new(move |()| remove_drop(drop_for_remove.clone()))
                                />
                            }
                        }
                    />
                </Show>

                // Add button (hidden in readonly mode)
                <Show when=move || !readonly.get()>
                    <button
                        class="drop-editor__add-btn"
                        style=format!("width: {btn_size}px; height: {btn_size}px;")
                        on:click=move |_| set_show_add_modal.set(true)
                        title="Add prize"
                    >
                        <span class="drop-editor__add-icon">"+"</span>
                    </button>
                </Show>
            </div>

            // Add drop modal
            <AddDropModal
                open=show_add_modal
                on_close=move || set_show_add_modal.set(false)
                on_add=add_drop
            />
        </div>
    }
}

/// Generate a unique key for a drop (for list rendering)
fn drop_key(drop: &Drop) -> String {
    match drop {
        Drop::Tip { token, amount } => format!("tip-{token}-{amount}"),
        Drop::WalletSend { asset_id, amount } => {
            format!("ws-{}-{amount}", asset_id.concatenated())
        }
    }
}

/// Format drop display name for AssetCard
fn drop_display_name(drop: &Drop) -> String {
    match drop {
        Drop::Tip { token, amount } => {
            format!("{} {}", format_number(*amount), token)
        }
        Drop::WalletSend { asset_id, amount } => {
            let name = asset_id.asset_name();
            if *amount > 1 {
                format!("{} x{}", name, format_number(*amount as f64))
            } else {
                name
            }
        }
    }
}

/// A single drop item in the horizontal list
#[component]
fn DropItem(
    index: usize,
    drop: Drop,
    size: CardSize,
    #[prop(into)] readonly: Signal<bool>,
    drag_state: ItemDragState,
    #[prop(into)] on_remove: Callback<()>,
) -> impl IntoView {
    let display_name = drop_display_name(&drop);

    // Get asset_id for NFTs
    let asset_id = match &drop {
        Drop::WalletSend { asset_id, .. } => Some(asset_id.concatenated()),
        Drop::Tip { .. } => None,
    };

    let item_class = move || {
        let mut classes = vec!["drop-item"];
        if readonly.get() {
            classes.push("drop-item--readonly");
        }
        if drag_state.is_source {
            classes.push("drop-item--dragging");
        }
        classes.join(" ")
    };

    view! {
        <div class=item_class>
            // Position indicator
            <div class="drop-item__position">{index + 1}</div>

            // Card display
            <div class="drop-item__card">
                {match asset_id {
                    Some(id) => view! {
                        <AssetCard
                            asset_id=id
                            name=display_name.clone()
                            size=size
                            show_name=true
                            is_static=true
                        />
                    }.into_any(),
                    None => view! {
                        // Tip display - text card
                        <div class="drop-item__tip-card" style=format!(
                            "width: {}px; height: {}px;",
                            size.pixels().unwrap_or(120),
                            size.pixels().unwrap_or(120)
                        )>
                            <div class="drop-item__tip-icon">"$"</div>
                            <div class="drop-item__tip-name">{display_name.clone()}</div>
                        </div>
                    }.into_any(),
                }}
            </div>

            // Remove button (shown on hover, hidden in readonly)
            <Show when=move || !readonly.get()>
                <button
                    class="drop-item__remove"
                    on:click=move |ev| {
                        ev.stop_propagation();
                        on_remove.run(());
                    }
                    title="Remove"
                >
                    {"\u{00D7}"} // ×
                </button>
            </Show>
        </div>
    }
}

/// Form state for the AddDropModal
#[derive(Clone)]
struct AddDropFormState {
    drop_type: String,
    tip_token: String,
    tip_amount: f64,
    cnft_input: String,
    cnft_error: Option<String>,
}

impl Default for AddDropFormState {
    fn default() -> Self {
        Self {
            drop_type: "tip".to_string(),
            tip_token: "ADA".to_string(),
            tip_amount: 100.0,
            cnft_input: String::new(),
            cnft_error: None,
        }
    }
}

/// Modal for adding a new drop
#[component]
fn AddDropModal(
    #[prop(into)] open: Signal<bool>,
    on_close: impl Fn() + Send + Sync + 'static + Copy,
    on_add: impl Fn(Drop) + Send + Sync + 'static + Copy,
) -> impl IntoView {
    // Form state as single struct
    let (form, set_form) = signal(AddDropFormState::default());

    // Reset form when modal opens
    Effect::new(move |_| {
        if open.get() {
            set_form.set(AddDropFormState::default());
        }
    });

    let handle_add = move || {
        let state = form.get();
        if state.drop_type == "tip" {
            let drop = Drop::tip(state.tip_token, state.tip_amount);
            on_add(drop);
        } else {
            // CNFT - parse asset ID
            match AssetId::parse_smart(state.cnft_input.trim()) {
                Ok(asset_id) => {
                    let drop = Drop::wallet_send_single(asset_id);
                    on_add(drop);
                }
                Err(_) => {
                    set_form.update(|f| f.cnft_error = Some("Invalid asset ID format".to_string()));
                }
            }
        }
    };

    let type_options = Signal::derive(|| {
        vec![
            SelectOption::new("tip", "Tip (Fungible Token)"),
            SelectOption::new("cnft", "CNFT (NFT Transfer)"),
        ]
    });

    let can_add = move || {
        let state = form.get();
        if state.drop_type == "tip" {
            state.tip_amount > 0.0 && !state.tip_token.is_empty()
        } else {
            !state.cnft_input.trim().is_empty()
        }
    };

    // Derived signals for Select component (needs Signal<String>)
    let drop_type_signal = Signal::derive(move || form.get().drop_type);

    view! {
        <Modal
            open=open
            title="Add Prize"
            on_close=on_close
        >
            <div class="add-drop-modal">
                <div class="add-drop-modal__field">
                    <label>"Prize Type"</label>
                    <Select
                        value=drop_type_signal
                        options=type_options
                        on_change=Callback::new(move |v| set_form.update(|f| f.drop_type = v))
                    />
                </div>

                // Tip fields
                <Show when=move || form.get().drop_type == "tip">
                    <div class="add-drop-modal__row">
                        <div class="add-drop-modal__field add-drop-modal__field--amount">
                            <label>"Amount"</label>
                            <input
                                type="number"
                                class="add-drop-modal__input"
                                prop:value=move || form.get().tip_amount.to_string()
                                on:input=move |ev| {
                                    let target = ev.target().unwrap();
                                    let input: web_sys::HtmlInputElement = target.unchecked_into();
                                    if let Ok(val) = input.value().parse::<f64>() {
                                        set_form.update(|f| f.tip_amount = val);
                                    }
                                }
                                min="0"
                                step="0.01"
                            />
                        </div>
                        <div class="add-drop-modal__field add-drop-modal__field--token">
                            <label>"Token"</label>
                            <input
                                type="text"
                                class="add-drop-modal__input"
                                prop:value=move || form.get().tip_token.clone()
                                on:input=move |ev| {
                                    let target = ev.target().unwrap();
                                    let input: web_sys::HtmlInputElement = target.unchecked_into();
                                    set_form.update(|f| f.tip_token = input.value());
                                }
                                placeholder="ADA"
                            />
                        </div>
                    </div>
                </Show>

                // CNFT fields
                <Show when=move || form.get().drop_type == "cnft">
                    <div class="add-drop-modal__field">
                        <label>"Asset ID"</label>
                        <input
                            type="text"
                            class="add-drop-modal__input"
                            prop:value=move || form.get().cnft_input.clone()
                            on:input=move |ev| {
                                let target = ev.target().unwrap();
                                let input: web_sys::HtmlInputElement = target.unchecked_into();
                                set_form.update(|f| {
                                    f.cnft_input = input.value();
                                    f.cnft_error = None;
                                });
                            }
                            placeholder="Paste asset ID (policy + asset name hex)"
                        />
                        <Show when=move || form.get().cnft_error.is_some()>
                            <span class="add-drop-modal__error">{move || form.get().cnft_error.clone()}</span>
                        </Show>
                        <span class="add-drop-modal__hint">
                            "Formats: policy_id + asset_name_hex, or policy_id.asset_name_hex"
                        </span>
                    </div>
                </Show>

                <div class="add-drop-modal__actions">
                    <Button
                        variant=ButtonVariant::Secondary
                        on_click=move || on_close()
                    >
                        "Cancel"
                    </Button>
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=handle_add
                        disabled=Signal::derive(move || !can_add())
                    >
                        "Add Prize"
                    </Button>
                </div>
            </div>
        </Modal>
    }
}
