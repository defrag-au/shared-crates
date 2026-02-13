//! Game Lobby Component
//!
//! Displays the pre-game lobby where players wait and anyone can configure settings.

use leptos::prelude::*;

/// Game mode options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    TurnTaking,
    Race,
}

impl GameMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::TurnTaking => "Turn Taking (Classic)",
            Self::Race => "Race (Simultaneous)",
        }
    }
}

/// Lobby component for pre-game setup
#[component]
pub fn Lobby(
    /// List of players in the lobby
    #[prop(into)]
    players: Signal<Vec<(String, String)>>, // (user_id, user_name)
    /// Current user's ID
    #[prop(into)]
    current_user_id: Signal<String>,
    /// Current game mode setting
    #[prop(into)]
    game_mode: Signal<GameMode>,
    /// Current grid size setting
    #[prop(into)]
    grid_size: Signal<(u8, u8)>,
    /// Callback to change game mode
    on_mode_change: impl Fn(GameMode) + 'static,
    /// Callback to change grid size
    on_grid_change: impl Fn((u8, u8)) + 'static,
    /// Callback to start the game
    on_start: impl Fn() + 'static,
) -> impl IntoView {
    use leptos::prelude::CollectView;
    let on_mode_change = std::rc::Rc::new(on_mode_change);
    let on_grid_change = std::rc::Rc::new(on_grid_change);
    let on_start = std::rc::Rc::new(on_start);

    view! {
        <div class="lobby">
            <h2>"Game Lobby"</h2>

            <div class="lobby-players">
                <h3>"Players" <span class="player-count">"(" {move || players.get().len()} ")"</span></h3>
                <ul>
                    {move || {
                        let players_vec = players.get();
                        let my_id = current_user_id.get();

                        players_vec.into_iter().map(|(user_id, user_name)| {
                            let is_me = user_id == my_id;

                            view! {
                                <li>
                                    {user_name}
                                    {if is_me { " (you)" } else { "" }}
                                </li>
                            }
                        }).collect_view()
                    }}
                </ul>
            </div>

            {
                let on_mode = on_mode_change.clone();
                let on_grid = on_grid_change.clone();
                let on_start_click = on_start.clone();

                view! {
                    <div class="lobby-settings">
                        <h3>"Game Settings"</h3>

                        <div class="setting-row">
                            <label>"Mode:"</label>
                            <select on:change=move |ev| {
                                let value = event_target_value(&ev);
                                let mode = if value == "race" { GameMode::Race } else { GameMode::TurnTaking };
                                on_mode(mode);
                            }>
                                <option value="turn_taking" selected=move || game_mode.get() == GameMode::TurnTaking>
                                    "Turn Taking (Classic)"
                                </option>
                                <option value="race" selected=move || game_mode.get() == GameMode::Race>
                                    "Race (Simultaneous)"
                                </option>
                            </select>
                        </div>

                        <div class="setting-row">
                            <label>"Grid Size:"</label>
                            <select on:change=move |ev| {
                                let value = event_target_value(&ev);
                                let size = match value.as_str() {
                                    "4x4" => (4, 4),
                                    "6x6" => (6, 6),
                                    "8x8" => (8, 8),
                                    _ => (8, 8),
                                };
                                on_grid(size);
                            }>
                                <option value="4x4" selected=move || grid_size.get() == (4, 4)>
                                    "4x4 (8 pairs) - Quick"
                                </option>
                                <option value="6x6" selected=move || grid_size.get() == (6, 6)>
                                    "6x6 (18 pairs) - Medium"
                                </option>
                                <option value="8x8" selected=move || grid_size.get() == (8, 8)>
                                    "8x8 (32 pairs) - Full"
                                </option>
                            </select>
                        </div>

                        <button class="start-button" on:click=move |_| on_start_click()>
                            "Start Game"
                        </button>
                    </div>
                }
            }
        </div>
    }
}
