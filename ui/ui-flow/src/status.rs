//! Connection status types

use serde::{Deserialize, Serialize};

/// Current status of the WebSocket connection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub enum ConnectionStatus {
    /// Not connected, no reconnection in progress
    #[default]
    Disconnected,
    /// Attempting initial connection
    Connecting,
    /// Connected and receiving updates
    Connected,
    /// Disconnected, attempting to reconnect
    Reconnecting {
        /// Current reconnection attempt number (1-based)
        attempt: u32,
    },
    /// Authentication failed, will not auto-reconnect
    AuthFailed,
}

impl ConnectionStatus {
    /// Check if currently connected
    pub fn is_connected(&self) -> bool {
        matches!(self, ConnectionStatus::Connected)
    }

    /// Check if a connection attempt is in progress
    pub fn is_connecting(&self) -> bool {
        matches!(
            self,
            ConnectionStatus::Connecting | ConnectionStatus::Reconnecting { .. }
        )
    }

    /// Check if disconnected (not connecting)
    pub fn is_disconnected(&self) -> bool {
        matches!(
            self,
            ConnectionStatus::Disconnected | ConnectionStatus::AuthFailed
        )
    }

    /// Human-readable status description
    pub fn description(&self) -> &'static str {
        match self {
            ConnectionStatus::Disconnected => "Disconnected",
            ConnectionStatus::Connecting => "Connecting...",
            ConnectionStatus::Connected => "Connected",
            ConnectionStatus::Reconnecting { attempt } if *attempt <= 3 => "Reconnecting...",
            ConnectionStatus::Reconnecting { .. } => "Connection unstable",
            ConnectionStatus::AuthFailed => "Authentication failed",
        }
    }
}


/// Information about a WebSocket close event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseInfo {
    /// WebSocket close code
    pub code: u16,
    /// Close reason string
    pub reason: String,
}

impl CloseInfo {
    /// Check if this close indicates an authentication failure
    ///
    /// Close codes 4001-4009 are typically used for auth-related closures
    pub fn is_auth_failure(&self) -> bool {
        (4001..=4009).contains(&self.code)
    }

    /// Check if this was a normal closure
    pub fn is_normal(&self) -> bool {
        self.code == 1000 || self.code == 1001
    }
}

impl Default for CloseInfo {
    fn default() -> Self {
        Self {
            code: 1000,
            reason: String::new(),
        }
    }
}
