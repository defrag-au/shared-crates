//! Game Results Component
//!
//! Displays the final rankings when a game ends.

use leptos::prelude::*;

/// Game results / end screen component
#[component]
pub fn GameResults(
    /// Winner user ID (None if tie)
    #[prop(into)]
    winner: Signal<Option<String>>,
    /// Final rankings: (user_id, name, score)
    #[prop(into)]
    rankings: Signal<Vec<(String, String, u32)>>,
    /// Current user's ID
    #[prop(into)]
    current_user_id: Signal<String>,
    /// Callback to request rematch
    on_rematch: impl Fn() + 'static,
) -> impl IntoView {
    use leptos::prelude::CollectView;
    let on_rematch = std::rc::Rc::new(on_rematch);

    view! {
        <div class="game-results">
            <h2>"Game Over!"</h2>

            {move || {
                let winner_id = winner.get();
                let my_id = current_user_id.get();
                let rankings_vec = rankings.get();

                let winner_name = winner_id.as_ref().and_then(|wid| {
                    rankings_vec.iter().find(|(id, _, _)| id == wid).map(|(_, name, _)| name.clone())
                });

                let i_won = winner_id.as_ref() == Some(&my_id);

                view! {
                    <div class="winner-announcement">
                        {if let Some(name) = winner_name {
                            if i_won {
                                view! { <p class="winner-text you-won">"🎉 You won! 🎉"</p> }.into_any()
                            } else {
                                view! { <p class="winner-text">{format!("🏆 {} wins!", name)}</p> }.into_any()
                            }
                        } else {
                            view! { <p class="winner-text">"It's a tie!"</p> }.into_any()
                        }}
                    </div>
                }
            }}

            <div class="rankings">
                <h3>"Final Standings"</h3>
                <ol>
                    {move || {
                        let rankings_vec = rankings.get();
                        let my_id = current_user_id.get();

                        rankings_vec.into_iter().enumerate().map(|(idx, (user_id, name, score))| {
                            let is_me = user_id == my_id;
                            let medal = match idx {
                                0 => "🥇",
                                1 => "🥈",
                                2 => "🥉",
                                _ => "",
                            };

                            view! {
                                <li class:is-me=is_me>
                                    <span class="medal">{medal}</span>
                                    <span class="rank-name">{name}</span>
                                    {if is_me { " (you)" } else { "" }}
                                    <span class="rank-score">{format!("{} pairs", score)}</span>
                                </li>
                            }
                        }).collect_view()
                    }}
                </ol>
            </div>

            <button class="rematch-button" on:click={
                let on_rematch = on_rematch.clone();
                move |_| on_rematch()
            }>
                "Play Again"
            </button>
        </div>
    }
}
