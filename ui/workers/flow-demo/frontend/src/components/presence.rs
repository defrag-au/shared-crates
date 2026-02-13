//! Presence component

use leptos::prelude::*;
use ui_flow_protocol::PresenceInfo;

/// Online users list
#[component]
pub fn Presence(
    /// List of online users
    users: ReadSignal<Vec<PresenceInfo>>,
    /// Current user's ID to identify self
    current_user_id: ReadSignal<String>,
) -> impl IntoView {
    view! {
        <div class="card presence">
            <h2>"Online Users (" {move || users.get().len()} ")"</h2>
            <Show
                when=move || !users.get().is_empty()
                fallback=|| view! { <p class="empty">"No users online"</p> }
            >
                <ul class="user-list">
                    <For
                        each=move || users.get()
                        key=|user| user.user_id.clone()
                        children=move |user| {
                            let is_me = user.user_id == current_user_id.get();
                            let name = user.name.clone().unwrap_or_else(|| user.user_id.clone());
                            let display_name = if is_me {
                                format!("{} (me)", name)
                            } else {
                                name
                            };
                            let class = if is_me { "user--me" } else { "" };
                            view! {
                                <li class=class>
                                    <span class="status-dot"></span>
                                    <span class="name">{display_name}</span>
                                </li>
                            }
                        }
                    />
                </ul>
            </Show>
        </div>
    }
}
