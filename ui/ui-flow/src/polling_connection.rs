//! Polling-based WebSocket connection with Flow protocol
//!
//! This module provides a polling-based connection that works with both web-sys
//! and quad-net transports. Unlike the callback-based FlowConnection, this
//! connection requires calling `poll()` regularly to process incoming messages.
//!
//! This is ideal for game loops where you're already polling each frame.

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::status::{CloseInfo, ConnectionStatus};
use crate::transport::{DefaultTransport, WebSocketEvent, WebSocketTransport};
use ui_flow_protocol::{decode, encode, ClientMessage, OpId, PresenceInfo, ServerMessage};

/// Configuration for reconnection behavior
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Base delay in ms (doubled each attempt)
    pub base_delay_ms: u32,
    /// Maximum delay between attempts
    pub max_delay_ms: u32,
    /// Maximum number of attempts (None = infinite)
    pub max_attempts: Option<u32>,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            base_delay_ms: 1000,
            max_delay_ms: 30000,
            max_attempts: None,
        }
    }
}

/// Events emitted by the polling connection
#[derive(Debug)]
pub enum FlowEvent<State, Delta, Event> {
    /// Connection established (contains connection_id)
    Connected(String),
    /// Full state snapshot received
    Snapshot { state: State, seq: u64 },
    /// Delta update received
    Delta { delta: Delta, seq: u64 },
    /// Batch of deltas received
    Deltas { deltas: Vec<Delta>, seq: u64 },
    /// Presence update received
    Presence(Vec<PresenceInfo>),
    /// Notification event received
    Notify {
        domain: String,
        event: Event,
        correlation_id: Option<OpId>,
    },
    /// Action progress update
    Progress {
        op_id: OpId,
        percent: Option<u8>,
        message: Option<String>,
    },
    /// Action completed successfully
    ActionOk(OpId),
    /// Action failed
    ActionErr {
        op_id: OpId,
        code: Option<String>,
        message: String,
    },
    /// Connection status changed
    StatusChanged(ConnectionStatus),
    /// Error occurred
    Error { message: String, fatal: bool },
    /// Pong received (for latency measurement)
    Pong { client_ts: u64, server_ts: u64 },
}

/// Polling-based Flow connection
///
/// Unlike the callback-based `FlowConnection`, this connection must be polled
/// regularly to process incoming messages. This is ideal for game loops.
///
/// # Example
///
/// ```ignore
/// let mut connection = PollingFlowConnection::<GameState, GameDelta, GameEvent, GameAction>::connect(url)?;
///
/// // In your game loop:
/// loop {
///     while let Some(event) = connection.poll() {
///         match event {
///             FlowEvent::Snapshot { state, seq } => { /* handle */ }
///             FlowEvent::Delta { delta, seq } => { /* handle */ }
///             _ => {}
///         }
///     }
///
///     // ... rest of game loop
/// }
/// ```
pub struct PollingFlowConnection<State, Delta, Event, Action> {
    transport: DefaultTransport,
    url: String,
    status: ConnectionStatus,
    reconnect_config: ReconnectConfig,
    reconnect_attempt: u32,
    current_seq: u64,
    // Time tracking for reconnection delays
    reconnect_delay_until: Option<f64>,
    _phantom: std::marker::PhantomData<(State, Delta, Event, Action)>,
}

impl<State, Delta, Event, Action> PollingFlowConnection<State, Delta, Event, Action>
where
    State: DeserializeOwned,
    Delta: DeserializeOwned,
    Event: DeserializeOwned,
    Action: Serialize,
{
    /// Connect to a WebSocket server
    pub fn connect(url: &str) -> Result<Self, FlowError> {
        Self::connect_with_config(url, ReconnectConfig::default())
    }

    /// Connect with custom reconnection configuration
    pub fn connect_with_config(url: &str, config: ReconnectConfig) -> Result<Self, FlowError> {
        let transport =
            DefaultTransport::connect(url).map_err(|e| FlowError::Connection(format!("{e}")))?;

        Ok(Self {
            transport,
            url: url.to_string(),
            status: ConnectionStatus::Connecting,
            reconnect_config: config,
            reconnect_attempt: 0,
            current_seq: 0,
            reconnect_delay_until: None,
            _phantom: std::marker::PhantomData,
        })
    }

    /// Poll for the next event
    ///
    /// Call this regularly (e.g., each frame) to process incoming messages.
    /// Returns `None` when no events are available.
    pub fn poll(&mut self) -> Option<FlowEvent<State, Delta, Event>> {
        // Handle reconnection delay
        if let Some(delay_until) = self.reconnect_delay_until {
            let now = current_time_ms();
            if now < delay_until {
                return None;
            }
            // Delay elapsed, attempt reconnection
            self.reconnect_delay_until = None;
            if let Err(e) = self.attempt_reconnect() {
                return Some(FlowEvent::Error {
                    message: format!("Reconnection failed: {e}"),
                    fatal: false,
                });
            }
        }

        // Poll the transport for events
        let event = self.transport.poll()?;

        match event {
            WebSocketEvent::Open => {
                self.status = ConnectionStatus::Connected;
                self.reconnect_attempt = 0;
                Some(FlowEvent::StatusChanged(ConnectionStatus::Connected))
            }
            WebSocketEvent::Message(bytes) => self.handle_message(&bytes),
            WebSocketEvent::Close { code, reason } => {
                let close_info = CloseInfo { code, reason };
                self.handle_close(close_info)
            }
            WebSocketEvent::Error(msg) => Some(FlowEvent::Error {
                message: msg,
                fatal: false,
            }),
        }
    }

    fn handle_message(&mut self, bytes: &[u8]) -> Option<FlowEvent<State, Delta, Event>> {
        match decode::<ServerMessage<State, Delta, Event>>(bytes) {
            Ok(msg) => self.handle_server_message(msg),
            Err(e) => Some(FlowEvent::Error {
                message: format!("Decode error: {e}"),
                fatal: false,
            }),
        }
    }

    fn handle_server_message(
        &mut self,
        msg: ServerMessage<State, Delta, Event>,
    ) -> Option<FlowEvent<State, Delta, Event>> {
        match msg {
            ServerMessage::Connected { connection_id, .. } => {
                Some(FlowEvent::Connected(connection_id))
            }
            ServerMessage::Pong {
                client_ts,
                server_ts,
            } => Some(FlowEvent::Pong {
                client_ts,
                server_ts,
            }),
            ServerMessage::Error { message, fatal, .. } => {
                Some(FlowEvent::Error { message, fatal })
            }
            ServerMessage::Snapshot { state, seq, .. } => {
                self.current_seq = seq;
                Some(FlowEvent::Snapshot { state, seq })
            }
            ServerMessage::Delta { delta, seq, .. } => {
                self.current_seq = seq;
                Some(FlowEvent::Delta { delta, seq })
            }
            ServerMessage::Deltas { deltas, seq, .. } => {
                self.current_seq = seq;
                Some(FlowEvent::Deltas { deltas, seq })
            }
            ServerMessage::Presence { users } => Some(FlowEvent::Presence(users)),
            ServerMessage::Signal { .. } => {
                // WebRTC signalling - not implemented
                None
            }
            ServerMessage::Notify {
                domain,
                event,
                correlation_id,
            } => Some(FlowEvent::Notify {
                domain,
                event,
                correlation_id,
            }),
            ServerMessage::Progress {
                op_id,
                percent,
                message,
            } => Some(FlowEvent::Progress {
                op_id,
                percent,
                message,
            }),
            ServerMessage::ActionOk { op_id, .. } => Some(FlowEvent::ActionOk(op_id)),
            ServerMessage::ActionErr {
                op_id,
                code,
                message,
            } => Some(FlowEvent::ActionErr {
                op_id,
                code,
                message,
            }),
        }
    }

    fn handle_close(&mut self, close_info: CloseInfo) -> Option<FlowEvent<State, Delta, Event>> {
        if close_info.is_auth_failure() {
            self.status = ConnectionStatus::AuthFailed;
            return Some(FlowEvent::StatusChanged(ConnectionStatus::AuthFailed));
        }

        // Schedule reconnection
        self.reconnect_attempt += 1;
        let should_reconnect = self
            .reconnect_config
            .max_attempts
            .map(|m| self.reconnect_attempt <= m)
            .unwrap_or(true);

        if should_reconnect {
            let delay = calculate_backoff(self.reconnect_attempt, &self.reconnect_config);
            self.reconnect_delay_until = Some(current_time_ms() + delay as f64);
            self.status = ConnectionStatus::Reconnecting {
                attempt: self.reconnect_attempt,
            };
            Some(FlowEvent::StatusChanged(ConnectionStatus::Reconnecting {
                attempt: self.reconnect_attempt,
            }))
        } else {
            self.status = ConnectionStatus::Disconnected;
            Some(FlowEvent::StatusChanged(ConnectionStatus::Disconnected))
        }
    }

    fn attempt_reconnect(&mut self) -> Result<(), FlowError> {
        self.transport = DefaultTransport::connect(&self.url)
            .map_err(|e| FlowError::Connection(format!("{e}")))?;
        self.status = ConnectionStatus::Connecting;
        Ok(())
    }

    /// Get current connection status
    pub fn status(&self) -> ConnectionStatus {
        self.status
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.status.is_connected() && self.transport.is_connected()
    }

    /// Get current sequence number
    pub fn current_seq(&self) -> u64 {
        self.current_seq
    }

    /// Send an action to the server
    pub fn send_action(&self, op_id: OpId, action: Action) -> Result<(), FlowError> {
        let msg = ClientMessage::action(op_id, action);
        self.send_message(&msg)
    }

    /// Request state resynchronization
    pub fn resync(&self, last_seq: Option<u64>) -> Result<(), FlowError> {
        let msg: ClientMessage<Action> = ClientMessage::resync(last_seq);
        self.send_message(&msg)
    }

    /// Subscribe to notification domains
    pub fn subscribe(&self, domains: Vec<String>) -> Result<(), FlowError> {
        let msg: ClientMessage<Action> = ClientMessage::subscribe(domains);
        self.send_message(&msg)
    }

    /// Unsubscribe from notification domains
    pub fn unsubscribe(&self, domains: Vec<String>) -> Result<(), FlowError> {
        let msg: ClientMessage<Action> = ClientMessage::unsubscribe(domains);
        self.send_message(&msg)
    }

    /// Send a ping for latency measurement
    pub fn send_ping(&self, timestamp: u64) -> Result<(), FlowError> {
        let msg: ClientMessage<Action> = ClientMessage::ping(timestamp);
        self.send_message(&msg)
    }

    /// Disconnect and clean up
    pub fn disconnect(&mut self) {
        self.transport.close();
        self.status = ConnectionStatus::Disconnected;
        self.reconnect_attempt = 0;
        self.reconnect_delay_until = None;
    }

    fn send_message<M: Serialize>(&self, msg: &M) -> Result<(), FlowError> {
        if !self.transport.is_connected() {
            return Err(FlowError::NotConnected);
        }

        let bytes = encode(msg).map_err(|e| FlowError::Serialization(e.to_string()))?;
        self.transport
            .send(&bytes)
            .map_err(|e| FlowError::Send(format!("{e}")))
    }
}

fn calculate_backoff(attempt: u32, config: &ReconnectConfig) -> u32 {
    let multiplier = 2u32.saturating_pow(attempt.saturating_sub(1));
    let delay = config.base_delay_ms.saturating_mul(multiplier);
    delay.min(config.max_delay_ms)
}

/// Get current time in milliseconds
///
/// Uses different implementations for web-sys vs macroquad
#[cfg(all(feature = "web-sys-transport", not(feature = "macroquad")))]
fn current_time_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(feature = "macroquad")]
fn current_time_ms() -> f64 {
    // miniquad provides date::now() in seconds
    miniquad::date::now() * 1000.0
}

#[cfg(not(any(feature = "web-sys-transport", feature = "macroquad")))]
fn current_time_ms() -> f64 {
    0.0 // Fallback - reconnection timing won't work
}

/// Errors that can occur with Flow connections
#[derive(Debug, thiserror::Error)]
pub enum FlowError {
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Not connected")]
    NotConnected,
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Send error: {0}")]
    Send(String),
}
