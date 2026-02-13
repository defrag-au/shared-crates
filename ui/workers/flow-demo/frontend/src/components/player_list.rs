//! Player List / Scoreboard Component
//!
//! Displays the list of players with their scores and turn indicator.

use leptos::prelude::*;

/// Player information for display
#[derive(Debug, Clone)]
pub struct PlayerInfo {
    pub user_id: String,
    pub user_name: String,
    pub score: u32,
    pub spectating: bool,
}

/// Player list / scoreboard component
#[component]
pub fn PlayerList(
    /// List of players
    #[prop(into)]
    players: Signal<Vec<PlayerInfo>>,
    /// Current player's turn (user_id) - None if race mode or not playing
    #[prop(into)]
    current_turn: Signal<Option<String>>,
    /// Current user's ID
    #[prop(into)]
    current_user_id: Signal<String>,
) -> impl IntoView {
    use leptos::prelude::CollectView;
    view! {
        <div class="player-list">
            <h3>"Players"</h3>
            <ul>
                {move || {
                    let players_vec = players.get();
                    let turn = current_turn.get();
                    let my_id = current_user_id.get();

                    players_vec.into_iter().map(|player| {
                        let is_current_turn = turn.as_ref() == Some(&player.user_id);
                        let is_me = player.user_id == my_id;

                        view! {
                            <li class:current-turn=is_current_turn class:spectator=player.spectating>
                                <span class="turn-indicator">
                                    {if is_current_turn { "▶ " } else { "" }}
                                </span>
                                <span class="player-name">
                                    {player.user_name.clone()}
                                    {if is_me { " (me)" } else { "" }}
                                </span>
                                {if player.spectating {
                                    view! { <span class="spectator-badge">" 👁"</span> }.into_any()
                                } else {
                                    view! { <span class="score">{format!(" - {} pts", player.score)}</span> }.into_any()
                                }}
                            </li>
                        }
                    }).collect_view()
                }}
            </ul>
        </div>
    }
}
