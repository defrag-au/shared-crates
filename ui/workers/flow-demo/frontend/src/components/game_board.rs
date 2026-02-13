//! Memory Game Board Component
//!
//! Displays the grid of cards and handles card flip interactions.

use crate::memory_app::CardId;
use leptos::prelude::*;
use ui_components::MemoryCard;

/// Card view data for the frontend
#[derive(Debug, Clone)]
pub struct CardView {
    /// Stable card ID (used as DOM key for animations)
    pub card_id: CardId,
    /// Whether the card face is visible
    pub visible: bool,
    /// Asset ID (policy_id + asset_name_hex) for IIIF URL generation
    pub asset_id: Option<String>,
    /// Asset name (if visible)
    pub name: Option<String>,
    /// Whether this card has been matched
    pub matched: bool,
    /// Who matched this card
    pub matched_by: Option<String>,
}

/// Memory game board displaying a grid of cards
#[component]
pub fn GameBoard(
    /// Grid dimensions (cols, rows)
    #[prop(into)]
    grid_size: Signal<(u8, u8)>,
    /// Card data - keyed by CardId for stable DOM elements
    #[prop(into)]
    cards: Signal<Vec<CardView>>,
    /// Currently flipped card IDs
    #[prop(into)]
    flipped_card_ids: Signal<Vec<CardId>>,
    /// Whether it's this player's turn
    #[prop(into)]
    is_my_turn: Signal<bool>,
    /// Callback when a card is clicked (receives CardId)
    on_flip: impl Fn(CardId) + 'static + Clone + Send + Sync,
    /// Callback when a card's image has loaded (receives CardId)
    on_card_loaded: impl Fn(CardId) + 'static + Clone + Send + Sync,
    /// Whether input is disabled
    #[prop(into)]
    disabled: Signal<bool>,
) -> impl IntoView {
    view! {
        <div class="game-board-container">
            <div
                class="game-board"
                style=move || {
                    let (cols, _rows) = grid_size.get();
                    format!(
                        "display: grid; grid-template-columns: repeat({}, 1fr); gap: 8px;",
                        cols
                    )
                }
            >
                // Use For with CardId as the stable key
                // This ensures DOM elements persist across state updates and CSS transitions animate
                <For
                    each=move || cards.get()
                    key=|card| card.card_id.0.clone()
                    children={
                        let on_flip = on_flip.clone();
                        let on_card_loaded = on_card_loaded.clone();
                        move |card| {
                        let card_id = card.card_id.clone();
                        let card_id_for_flip = card_id.clone();
                        let card_id_for_load = card_id.clone();
                        let on_flip = on_flip.clone();
                        let on_card_loaded = on_card_loaded.clone();

                        // Derive state reactively for this specific card
                        let asset_id = Signal::derive({
                            let card_id = card_id.clone();
                            move || {
                                cards.get()
                                    .iter()
                                    .find(|c| c.card_id == card_id)
                                    .and_then(|c| c.asset_id.clone())
                                    .unwrap_or_default()
                            }
                        });

                        let name = Signal::derive({
                            let card_id = card_id.clone();
                            move || {
                                cards.get()
                                    .iter()
                                    .find(|c| c.card_id == card_id)
                                    .and_then(|c| c.name.clone())
                                    .unwrap_or_default()
                            }
                        });

                        let is_flipped = Signal::derive({
                            let card_id = card_id.clone();
                            move || {
                                let flipped = flipped_card_ids.get();
                                let card_visible = cards.get()
                                    .iter()
                                    .find(|c| c.card_id == card_id)
                                    .map(|c| c.visible)
                                    .unwrap_or(false);
                                flipped.contains(&card_id) || card_visible
                            }
                        });

                        let is_matched = Signal::derive({
                            let card_id = card_id.clone();
                            move || {
                                cards.get()
                                    .iter()
                                    .find(|c| c.card_id == card_id)
                                    .map(|c| c.matched)
                                    .unwrap_or(false)
                            }
                        });

                        let matched_by_signal = Signal::derive({
                            let card_id = card_id.clone();
                            move || {
                                cards.get()
                                    .iter()
                                    .find(|c| c.card_id == card_id)
                                    .and_then(|c| c.matched_by.clone())
                                    .unwrap_or_default()
                            }
                        });

                        let can_click = Signal::derive(move || {
                            let my_turn = is_my_turn.get();
                            let is_disabled_val = disabled.get();
                            let matched = is_matched.get();
                            let flipped = is_flipped.get();
                            my_turn && !is_disabled_val && !matched && !flipped
                        });

                        view! {
                            <MemoryCard
                                asset_id=asset_id
                                name=name
                                flipped=is_flipped
                                matched=is_matched
                                matched_by=matched_by_signal
                                disabled=Signal::derive(move || !can_click.get())
                                on_click=move || {
                                    if can_click.get_untracked() {
                                        on_flip(card_id_for_flip.clone());
                                    }
                                }
                                on_load=move || {
                                    on_card_loaded(card_id_for_load.clone());
                                }
                            />
                        }
                    }}
                />
            </div>
        </div>
    }
}
