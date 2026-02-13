//! Connection Status Leptos Component
//!
//! A status indicator showing WebSocket/realtime connection state.
//! Supports click-to-reconnect behavior.
//!
//! ## Props
//!
//! - `status` - Connection state: Connected, Connecting, Disconnected, Error
//! - `show_text` - Whether to show status text (default: true)
//! - `on_reconnect` - Callback when user clicks to reconnect (only when disconnected/error)
//!
//! ## Usage
//!
//! ```ignore
//! <ConnectionStatus
//!     status=connection_status
//!     on_reconnect=move |_| { reconnect(); }
//! />
//! ```

use leptos::prelude::*;

/// Connection state values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Error,
}

impl ConnectionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Connecting => "connecting",
            Self::Disconnected => "disconnected",
            Self::Error => "error",
        }
    }

    pub fn display_text(&self) -> &'static str {
        match self {
            Self::Connected => "Connected",
            Self::Connecting => "Connecting...",
            Self::Disconnected => "Disconnected",
            Self::Error => "Connection Error",
        }
    }

    pub fn is_reconnectable(&self) -> bool {
        matches!(self, Self::Disconnected | Self::Error)
    }
}

/// Connection status component
#[component]
pub fn ConnectionStatus(
    /// Connection state
    #[prop(into)]
    status: Signal<ConnectionState>,
    /// Whether to show status text
    #[prop(optional, default = true)]
    show_text: bool,
    /// Reconnect callback (called when clicking in disconnected/error state)
    #[prop(into, optional)]
    on_reconnect: Option<Callback<()>>,
) -> impl IntoView {
    let status_class = move || {
        let s = status.get();
        let clickable = s.is_reconnectable() && on_reconnect.is_some();
        let mut classes = vec![
            "connection-status",
            match s {
                ConnectionState::Connected => "connection-status--connected",
                ConnectionState::Connecting => "connection-status--connecting",
                ConnectionState::Disconnected => "connection-status--disconnected",
                ConnectionState::Error => "connection-status--error",
            },
        ];
        if clickable {
            classes.push("connection-status--clickable");
        }
        classes.join(" ")
    };

    let handle_click = move |_| {
        if status.get().is_reconnectable() {
            if let Some(cb) = on_reconnect {
                cb.run(());
            }
        }
    };

    view! {
        <div class=status_class on:click=handle_click>
            <span class="connection-status__indicator"></span>
            {move || show_text.then(|| {
                let s = status.get();
                let clickable = s.is_reconnectable() && on_reconnect.is_some();
                let text_class = if clickable {
                    "connection-status__text connection-status__text--clickable"
                } else {
                    "connection-status__text"
                };
                view! {
                    <span class=text_class>{s.display_text()}</span>
                }
            })}
        </div>
    }
}
