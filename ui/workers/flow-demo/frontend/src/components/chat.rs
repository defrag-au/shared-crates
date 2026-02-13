//! Chat component

use crate::ChatMessage;
use leptos::prelude::*;

/// Chat messages list with input
#[component]
pub fn Chat<F>(
    /// List of chat messages
    messages: Signal<Vec<ChatMessage>>,
    /// Current user's ID to identify own messages
    current_user_id: ReadSignal<String>,
    /// Called when a message is sent
    on_send: F,
    /// Whether input is disabled
    disabled: Signal<bool>,
) -> impl IntoView
where
    F: Fn(String) + 'static,
{
    let (input_value, set_input_value) = signal(String::new());

    // Wrap on_send in Rc so we can clone it for multiple handlers
    let on_send = std::rc::Rc::new(on_send);
    let on_send_keypress = on_send.clone();
    let on_send_click = on_send.clone();

    view! {
        <div class="card chat">
            <h2>"Chat"</h2>
            <div class="messages">
                <For
                    each=move || messages.get()
                    key=|msg| msg.id
                    children=move |msg| {
                        let is_me = msg.user_id == current_user_id.get();
                        let author_display = if is_me {
                            format!("{} (me)", msg.user_name)
                        } else {
                            msg.user_name.clone()
                        };
                        let message_class = if is_me { "message message--mine" } else { "message" };
                        view! {
                            <div class=message_class>
                                <div class="meta">
                                    <span class="author">{author_display}</span>
                                    <span class="time">{format_time(msg.timestamp)}</span>
                                </div>
                                <div class="content">{msg.text.clone()}</div>
                            </div>
                        }
                    }
                />
            </div>
            <div class="input-row">
                <input
                    type="text"
                    placeholder="Type a message..."
                    prop:value=move || input_value.get()
                    on:input=move |ev| set_input_value.set(event_target_value(&ev))
                    on:keypress=move |ev| {
                        if ev.key() == "Enter" {
                            let text = input_value.get();
                            if !text.trim().is_empty() {
                                on_send_keypress(text);
                                set_input_value.set(String::new());
                            }
                        }
                    }
                    disabled=move || disabled.get()
                />
                <button
                    class="primary"
                    on:click=move |_| {
                        let text = input_value.get();
                        if !text.trim().is_empty() {
                            on_send_click(text);
                            set_input_value.set(String::new());
                        }
                    }
                    disabled=move || disabled.get() || input_value.get().trim().is_empty()
                >
                    "Send"
                </button>
            </div>
        </div>
    }
}

/// Format timestamp to human readable time
fn format_time(timestamp: u64) -> String {
    let date = js_sys::Date::new(&(timestamp as f64).into());
    let hours = date.get_hours();
    let minutes = date.get_minutes();
    format!("{:02}:{:02}", hours, minutes)
}
