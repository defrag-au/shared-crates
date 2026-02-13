//! # ui-flow-protocol
//!
//! Wire protocol types for unified realtime communication using MessagePack.
//!
//! This crate provides the shared message types used by both client (ui-flow)
//! and server (Cloudflare Workers) for real-time state synchronization.
//!
//! ## Protocol Overview
//!
//! Messages use u16 integer tags for efficient dispatch and extensibility:
//!
//! | Range | Category | Notes |
//! |-------|----------|-------|
//! | 0-999 | Core protocol | Connection lifecycle, errors |
//! | 1000-1999 | State sync | Snapshots, deltas |
//! | 2000-2999 | Presence | User presence tracking |
//! | 3000-3999 | Signalling | WebRTC connection setup |
//! | 4000-4999 | Notifications | Application events |
//! | 5000-5999 | Action feedback | Optimistic UI support |
//!
//! ## Example
//!
//! ```rust,ignore
//! use ui_flow_protocol::{ServerMessage, ClientMessage, encode, decode};
//!
//! // Define your application types
//! #[derive(Serialize, Deserialize)]
//! struct MyState { counter: u64 }
//!
//! #[derive(Serialize, Deserialize)]
//! enum MyDelta { CounterChanged(u64) }
//!
//! #[derive(Serialize, Deserialize)]
//! enum MyEvent { Announcement(String) }
//!
//! #[derive(Serialize, Deserialize)]
//! enum MyAction { Increment, Decrement }
//!
//! // Encode a server message
//! let msg = ServerMessage::<MyState, MyDelta, MyEvent>::snapshot(
//!     MyState { counter: 42 },
//!     1,
//!     1234567890,
//! );
//! let bytes = encode(&msg)?;
//!
//! // Decode a client message
//! let client_msg: ClientMessage<MyAction> = decode(&bytes)?;
//! ```

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

// ─────────────────────────────────────────────────────────────────────────────
// Error Types
// ─────────────────────────────────────────────────────────────────────────────

/// Protocol errors
#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("Failed to encode message: {0}")]
    Encode(#[from] rmp_serde::encode::Error),

    #[error("Failed to decode message: {0}")]
    Decode(#[from] rmp_serde::decode::Error),

    #[error("Unknown message tag: {0}")]
    UnknownTag(u16),
}

// ─────────────────────────────────────────────────────────────────────────────
// Codec Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Encode a message to MessagePack bytes
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, ProtocolError> {
    rmp_serde::to_vec_named(msg).map_err(ProtocolError::from)
}

/// Decode a message from MessagePack bytes
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProtocolError> {
    rmp_serde::from_slice(bytes).map_err(ProtocolError::from)
}

// ─────────────────────────────────────────────────────────────────────────────
// Operation ID
// ─────────────────────────────────────────────────────────────────────────────

/// Unique operation identifier for tracking actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OpId(pub u64);

impl OpId {
    /// Generate a new unique operation ID
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Create an OpId from a raw value
    pub fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Get the raw value
    pub fn as_raw(&self) -> u64 {
        self.0
    }
}

impl Default for OpId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for OpId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "op:{}", self.0)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Server Message Tags
// ─────────────────────────────────────────────────────────────────────────────

/// Message type tag for server→client messages
#[derive(Serialize_repr, Deserialize_repr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ServerTag {
    // Core protocol (0-999)
    Connected = 0,
    Pong = 1,
    Error = 2,

    // State sync (1000-1999)
    Snapshot = 1000,
    Delta = 1001,
    Deltas = 1002,

    // Presence (2000-2999)
    Presence = 2000,

    // WebRTC signalling (3000-3999)
    Signal = 3000,

    // Notifications (4000-4999)
    Notify = 4000,

    // Action feedback (5000-5999)
    Progress = 5000,
    ActionOk = 5001,
    ActionErr = 5002,
}

// ─────────────────────────────────────────────────────────────────────────────
// Client Message Tags
// ─────────────────────────────────────────────────────────────────────────────

/// Message type tag for client→server messages
#[derive(Serialize_repr, Deserialize_repr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ClientTag {
    // Core protocol (0-999)
    Ping = 0,
    Resync = 1,

    // Actions (1000-1999)
    Action = 1000,

    // Subscriptions (2000-2999)
    Subscribe = 2000,
    Unsubscribe = 2001,

    // WebRTC signalling (3000-3999)
    Signal = 3000,
}

// ─────────────────────────────────────────────────────────────────────────────
// Presence Types
// ─────────────────────────────────────────────────────────────────────────────

/// Information about a connected user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceInfo {
    /// User identifier
    pub user_id: String,
    /// Display name if available
    pub name: Option<String>,
    /// Current status
    pub status: PresenceStatus,
    /// When they connected (unix ms)
    pub connected_at: u64,
}

/// User presence status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresenceStatus {
    Active,
    Idle,
    Away,
}

// ─────────────────────────────────────────────────────────────────────────────
// WebRTC Signalling Types
// ─────────────────────────────────────────────────────────────────────────────

/// WebRTC signalling payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SignalPayload {
    /// SDP offer
    Offer { sdp: String },
    /// SDP answer
    Answer { sdp: String },
    /// ICE candidate
    IceCandidate {
        candidate: String,
        sdp_mid: Option<String>,
        sdp_m_line_index: Option<u16>,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Server Messages
// ─────────────────────────────────────────────────────────────────────────────

/// Server-to-client message
///
/// Generic over:
/// - `State`: Full state type for snapshots
/// - `Delta`: Incremental state change type
/// - `Event`: Application-specific event type (flows through Notify)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum ServerMessage<State, Delta, Event> {
    // ─────────────────────────────────────────────────────────────
    // Connection Lifecycle (0-999)
    // ─────────────────────────────────────────────────────────────
    /// Connection established, server ready
    #[serde(rename = "0")]
    Connected {
        /// Protocol version for compatibility checking
        protocol_version: u8,
        /// Server-assigned connection ID (for debugging/logging)
        connection_id: String,
    },

    /// Response to client ping
    #[serde(rename = "1")]
    Pong {
        /// Echo back the client's timestamp for latency measurement
        client_ts: u64,
        /// Server timestamp
        server_ts: u64,
    },

    /// Server-initiated error (not tied to specific action)
    #[serde(rename = "2")]
    Error {
        /// Error code
        code: Option<String>,
        /// Error message
        message: String,
        /// Whether client should disconnect
        fatal: bool,
    },

    // ─────────────────────────────────────────────────────────────
    // State Synchronization (1000-1999)
    // ─────────────────────────────────────────────────────────────
    /// Full state snapshot - sent on connect and on resync requests
    #[serde(rename = "1000")]
    Snapshot {
        /// Complete state
        state: State,
        /// Sequence number for this state
        seq: u64,
        /// Server timestamp when snapshot was generated
        timestamp: u64,
    },

    /// Incremental state update
    #[serde(rename = "1001")]
    Delta {
        /// Changes to apply
        delta: Delta,
        /// New sequence number after applying this delta
        seq: u64,
        /// Server timestamp
        timestamp: u64,
    },

    /// Batched deltas for catch-up scenarios
    #[serde(rename = "1002")]
    Deltas {
        /// Ordered list of deltas to apply
        deltas: Vec<Delta>,
        /// Final sequence number after all deltas applied
        seq: u64,
        /// Server timestamp
        timestamp: u64,
    },

    // ─────────────────────────────────────────────────────────────
    // Presence (2000-2999)
    // ─────────────────────────────────────────────────────────────
    /// Presence update - who's connected
    #[serde(rename = "2000")]
    Presence {
        /// Users currently connected to this resource
        users: Vec<PresenceInfo>,
    },

    // ─────────────────────────────────────────────────────────────
    // WebRTC Signalling (3000-3999)
    // ─────────────────────────────────────────────────────────────
    /// WebRTC signalling message from another peer
    #[serde(rename = "3000")]
    Signal {
        /// Source user ID
        from_user_id: String,
        /// Signalling payload
        signal: SignalPayload,
    },

    // ─────────────────────────────────────────────────────────────
    // Notifications (4000-4999)
    // ─────────────────────────────────────────────────────────────
    /// Application event notification
    ///
    /// The `Event` type parameter allows applications to define their own
    /// strongly-typed event enums. All application-specific events flow
    /// through this single variant.
    #[serde(rename = "4000")]
    Notify {
        /// Domain identifier (e.g., "world", "widget_bridge", "rewards")
        domain: String,
        /// Event payload - application-defined type
        event: Event,
        /// Optional correlation ID (links to triggering action)
        correlation_id: Option<OpId>,
    },

    // ─────────────────────────────────────────────────────────────
    // Action Feedback (5000-5999)
    // ─────────────────────────────────────────────────────────────
    /// Progress update for an in-flight action
    #[serde(rename = "5000")]
    Progress {
        op_id: OpId,
        /// Progress percentage (0-100), if determinable
        percent: Option<u8>,
        /// Human-readable status message
        message: Option<String>,
    },

    /// Action completed successfully
    #[serde(rename = "5001")]
    ActionOk {
        op_id: OpId,
        /// Optional result data (using Vec<u8> instead of serde_json::Value for no-std compat)
        result: Option<Vec<u8>>,
    },

    /// Action failed
    #[serde(rename = "5002")]
    ActionErr {
        op_id: OpId,
        /// Error code for programmatic handling
        code: Option<String>,
        /// Human-readable error message
        message: String,
    },
}

impl<State, Delta, Event> ServerMessage<State, Delta, Event> {
    /// Create a Connected message
    pub fn connected(protocol_version: u8, connection_id: String) -> Self {
        Self::Connected {
            protocol_version,
            connection_id,
        }
    }

    /// Create a Pong message
    pub fn pong(client_ts: u64, server_ts: u64) -> Self {
        Self::Pong {
            client_ts,
            server_ts,
        }
    }

    /// Create an Error message
    pub fn error(message: impl Into<String>, fatal: bool) -> Self {
        Self::Error {
            code: None,
            message: message.into(),
            fatal,
        }
    }

    /// Create an Error message with a code
    pub fn error_with_code(
        code: impl Into<String>,
        message: impl Into<String>,
        fatal: bool,
    ) -> Self {
        Self::Error {
            code: Some(code.into()),
            message: message.into(),
            fatal,
        }
    }

    /// Create a Snapshot message
    pub fn snapshot(state: State, seq: u64, timestamp: u64) -> Self {
        Self::Snapshot {
            state,
            seq,
            timestamp,
        }
    }

    /// Create a Delta message
    pub fn delta(delta: Delta, seq: u64, timestamp: u64) -> Self {
        Self::Delta {
            delta,
            seq,
            timestamp,
        }
    }

    /// Create a Deltas (batch) message
    pub fn deltas(deltas: Vec<Delta>, seq: u64, timestamp: u64) -> Self {
        Self::Deltas {
            deltas,
            seq,
            timestamp,
        }
    }

    /// Create a Presence message
    pub fn presence(users: Vec<PresenceInfo>) -> Self {
        Self::Presence { users }
    }

    /// Create a Notify message
    pub fn notify(domain: impl Into<String>, event: Event, correlation_id: Option<OpId>) -> Self {
        Self::Notify {
            domain: domain.into(),
            event,
            correlation_id,
        }
    }

    /// Create a Progress message
    pub fn progress(op_id: OpId, percent: Option<u8>, message: Option<String>) -> Self {
        Self::Progress {
            op_id,
            percent,
            message,
        }
    }

    /// Create an ActionOk message
    pub fn action_ok(op_id: OpId, result: Option<Vec<u8>>) -> Self {
        Self::ActionOk { op_id, result }
    }

    /// Create an ActionErr message
    pub fn action_err(op_id: OpId, message: impl Into<String>) -> Self {
        Self::ActionErr {
            op_id,
            code: None,
            message: message.into(),
        }
    }

    /// Create an ActionErr message with a code
    pub fn action_err_with_code(
        op_id: OpId,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::ActionErr {
            op_id,
            code: Some(code.into()),
            message: message.into(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Client Messages
// ─────────────────────────────────────────────────────────────────────────────

/// Client-to-server message
///
/// Generic over:
/// - `Action`: Application-specific action type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum ClientMessage<Action> {
    // ─────────────────────────────────────────────────────────────
    // Connection Lifecycle (0-999)
    // ─────────────────────────────────────────────────────────────
    /// Keepalive ping
    #[serde(rename = "0")]
    Ping {
        /// Client timestamp for latency measurement
        ts: u64,
    },

    /// Request state resynchronization
    #[serde(rename = "1")]
    Resync {
        /// Last known sequence number (server may send delta if close enough)
        last_seq: Option<u64>,
    },

    // ─────────────────────────────────────────────────────────────
    // Actions (1000-1999)
    // ─────────────────────────────────────────────────────────────
    /// Client action with operation tracking
    #[serde(rename = "1000")]
    Action {
        /// Operation ID for correlation
        op_id: OpId,
        /// The action payload - application-defined type
        action: Action,
    },

    // ─────────────────────────────────────────────────────────────
    // Subscriptions (2000-2999)
    // ─────────────────────────────────────────────────────────────
    /// Subscribe to notification domain(s)
    #[serde(rename = "2000")]
    Subscribe {
        /// Domains to subscribe to
        domains: Vec<String>,
    },

    /// Unsubscribe from notification domain(s)
    #[serde(rename = "2001")]
    Unsubscribe {
        /// Domains to unsubscribe from
        domains: Vec<String>,
    },

    // ─────────────────────────────────────────────────────────────
    // WebRTC Signalling (3000-3999)
    // ─────────────────────────────────────────────────────────────
    /// Send WebRTC signalling message to a peer
    #[serde(rename = "3000")]
    Signal {
        /// Target user ID
        target_user_id: String,
        /// Signalling payload (offer, answer, ICE candidate)
        signal: SignalPayload,
    },
}

impl<Action> ClientMessage<Action> {
    /// Create a Ping message
    pub fn ping(ts: u64) -> Self {
        Self::Ping { ts }
    }

    /// Create a Resync message
    pub fn resync(last_seq: Option<u64>) -> Self {
        Self::Resync { last_seq }
    }

    /// Create an Action message
    pub fn action(op_id: OpId, action: Action) -> Self {
        Self::Action { op_id, action }
    }

    /// Create a Subscribe message
    pub fn subscribe(domains: Vec<String>) -> Self {
        Self::Subscribe { domains }
    }

    /// Create an Unsubscribe message
    pub fn unsubscribe(domains: Vec<String>) -> Self {
        Self::Unsubscribe { domains }
    }

    /// Create a Signal message
    pub fn signal(target_user_id: String, signal: SignalPayload) -> Self {
        Self::Signal {
            target_user_id,
            signal,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestState {
        counter: u64,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum TestDelta {
        CounterChanged(u64),
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum TestEvent {
        Announcement(String),
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum TestAction {
        Increment,
        Decrement,
    }

    #[test]
    fn test_op_id_generation() {
        let id1 = OpId::new();
        let id2 = OpId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_server_message_roundtrip() {
        type Msg = ServerMessage<TestState, TestDelta, TestEvent>;

        // Test Connected
        let msg: Msg = ServerMessage::connected(1, "conn-123".to_string());
        let bytes = encode(&msg).unwrap();
        let decoded: Msg = decode(&bytes).unwrap();
        assert!(matches!(
            decoded,
            ServerMessage::Connected {
                protocol_version: 1,
                ..
            }
        ));

        // Test Snapshot
        let msg: Msg = ServerMessage::snapshot(TestState { counter: 42 }, 1, 1234567890);
        let bytes = encode(&msg).unwrap();
        let decoded: Msg = decode(&bytes).unwrap();
        if let ServerMessage::Snapshot { state, seq, .. } = decoded {
            assert_eq!(state.counter, 42);
            assert_eq!(seq, 1);
        } else {
            panic!("Expected Snapshot");
        }

        // Test Delta
        let msg: Msg = ServerMessage::delta(TestDelta::CounterChanged(43), 2, 1234567891);
        let bytes = encode(&msg).unwrap();
        let decoded: Msg = decode(&bytes).unwrap();
        if let ServerMessage::Delta { delta, seq, .. } = decoded {
            assert_eq!(delta, TestDelta::CounterChanged(43));
            assert_eq!(seq, 2);
        } else {
            panic!("Expected Delta");
        }

        // Test Notify
        let msg: Msg =
            ServerMessage::notify("test", TestEvent::Announcement("Hello".to_string()), None);
        let bytes = encode(&msg).unwrap();
        let decoded: Msg = decode(&bytes).unwrap();
        if let ServerMessage::Notify { domain, event, .. } = decoded {
            assert_eq!(domain, "test");
            assert_eq!(event, TestEvent::Announcement("Hello".to_string()));
        } else {
            panic!("Expected Notify");
        }
    }

    #[test]
    fn test_client_message_roundtrip() {
        type Msg = ClientMessage<TestAction>;

        // Test Ping
        let msg: Msg = ClientMessage::ping(1234567890);
        let bytes = encode(&msg).unwrap();
        let decoded: Msg = decode(&bytes).unwrap();
        if let ClientMessage::Ping { ts } = decoded {
            assert_eq!(ts, 1234567890);
        } else {
            panic!("Expected Ping");
        }

        // Test Action
        let op_id = OpId::from_raw(42);
        let msg: Msg = ClientMessage::action(op_id, TestAction::Increment);
        let bytes = encode(&msg).unwrap();
        let decoded: Msg = decode(&bytes).unwrap();
        if let ClientMessage::Action {
            op_id: decoded_op_id,
            action,
        } = decoded
        {
            assert_eq!(decoded_op_id, op_id);
            assert_eq!(action, TestAction::Increment);
        } else {
            panic!("Expected Action");
        }

        // Test Subscribe
        let msg: Msg = ClientMessage::subscribe(vec!["world".to_string(), "rewards".to_string()]);
        let bytes = encode(&msg).unwrap();
        let decoded: Msg = decode(&bytes).unwrap();
        if let ClientMessage::Subscribe { domains } = decoded {
            assert_eq!(domains, vec!["world", "rewards"]);
        } else {
            panic!("Expected Subscribe");
        }
    }

    #[test]
    fn test_presence_info_roundtrip() {
        let info = PresenceInfo {
            user_id: "user-123".to_string(),
            name: Some("Alice".to_string()),
            status: PresenceStatus::Active,
            connected_at: 1234567890,
        };

        let bytes = encode(&info).unwrap();
        let decoded: PresenceInfo = decode(&bytes).unwrap();

        assert_eq!(decoded.user_id, "user-123");
        assert_eq!(decoded.name, Some("Alice".to_string()));
        assert_eq!(decoded.status, PresenceStatus::Active);
        assert_eq!(decoded.connected_at, 1234567890);
    }

    #[test]
    fn test_signal_payload_roundtrip() {
        // Test Offer
        let signal = SignalPayload::Offer {
            sdp: "v=0...".to_string(),
        };
        let bytes = encode(&signal).unwrap();
        let decoded: SignalPayload = decode(&bytes).unwrap();
        assert!(matches!(decoded, SignalPayload::Offer { .. }));

        // Test IceCandidate
        let signal = SignalPayload::IceCandidate {
            candidate: "candidate:...".to_string(),
            sdp_mid: Some("0".to_string()),
            sdp_m_line_index: Some(0),
        };
        let bytes = encode(&signal).unwrap();
        let decoded: SignalPayload = decode(&bytes).unwrap();
        assert!(matches!(decoded, SignalPayload::IceCandidate { .. }));
    }
}
