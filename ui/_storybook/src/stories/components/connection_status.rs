//! Connection Status component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{ConnectionState, ConnectionStatus};

#[component]
pub fn ConnectionStatusStory() -> impl IntoView {
    let (status, set_status) = signal(ConnectionState::Disconnected);
    let (reconnect_count, set_reconnect_count) = signal(0u32);

    view! {
        <div>
            <div class="story-header">
                <h2>"Connection Status"</h2>
                <p>"A component for displaying WebSocket/realtime connection state with click-to-reconnect support."</p>
            </div>

            // States section
            <div class="story-section">
                <h3>"Connection States"</h3>
                <div class="story-canvas">
                    <div class="story-inline">
                        <ConnectionStatus status=ConnectionState::Connected />
                        <ConnectionStatus status=ConnectionState::Connecting />
                        <ConnectionStatus status=ConnectionState::Disconnected />
                        <ConnectionStatus status=ConnectionState::Error />
                    </div>
                </div>
            </div>

            // Interactive demo section
            <div class="story-section">
                <h3>"Interactive Demo"</h3>
                <div class="story-canvas">
                    <div class="story-inline">
                        <button
                            class="demo-btn demo-btn--success"
                            on:click=move |_| set_status.set(ConnectionState::Connected)
                        >
                            "Set Connected"
                        </button>
                        <button
                            class="demo-btn demo-btn--warning"
                            on:click=move |_| set_status.set(ConnectionState::Connecting)
                        >
                            "Set Connecting"
                        </button>
                        <button
                            class="demo-btn"
                            on:click=move |_| set_status.set(ConnectionState::Disconnected)
                        >
                            "Set Disconnected"
                        </button>
                        <button
                            class="demo-btn demo-btn--danger"
                            on:click=move |_| set_status.set(ConnectionState::Error)
                        >
                            "Set Error"
                        </button>
                    </div>
                    <div class="story-inline" style="margin-top: 1rem;">
                        <ConnectionStatus
                            status=status
                            on_reconnect=move || set_reconnect_count.update(|c| *c += 1)
                        />
                        <span class="status-indicator status-indicator--connected">
                            {move || format!("Reconnect clicked: {} times", reconnect_count.get())}
                        </span>
                    </div>
                </div>
            </div>

            // Attributes section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="status"
                            values="ConnectionState"
                            description="Connection state to display: Connected, Connecting, Disconnected, Error"
                        />
                        <AttributeCard
                            name="show_text"
                            values="bool"
                            description="Whether to show status text (default: true)"
                        />
                        <AttributeCard
                            name="on_reconnect"
                            values="Callback<()>"
                            description="Called when user clicks to reconnect (only on Disconnected/Error)"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r#"use ui_components::{ConnectionStatus, ConnectionState};

// Static status display
view! {
    <ConnectionStatus status=ConnectionState::Connected />
    <ConnectionStatus status=ConnectionState::Disconnected />
}

// Reactive status with reconnect handler
let (status, set_status) = create_signal(ConnectionState::Disconnected);
view! {
    <ConnectionStatus
        status=status
        on_reconnect=move || {
            set_status.set(ConnectionState::Connecting);
            // Attempt reconnection...
        }
    />
}

// Without text label
view! {
    <ConnectionStatus
        status=ConnectionState::Connected
        show_text=false
    />
}"#}</pre>
            </div>
        </div>
    }
}
