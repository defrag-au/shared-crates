//! Notification-only connection mode for widgets that don't need state sync.
//!
//! This module provides a simplified API for widgets that only need to receive
//! notification events and optionally send actions, without state synchronization.
//!
//! # Example
//!
//! ```ignore
//! use ui_flow::notify::{NotifyConnection, NoAction};
//! use my_app::WidgetEvent;
//!
//! // Notification-only (no actions)
//! let conn = NotifyConnection::<WidgetEvent, NoAction>::builder()
//!     .url("wss://example.com/ws")
//!     .on_notify(|domain, event, _| {
//!         log::info!("Got event in {}: {:?}", domain, event);
//!     })
//!     .connect()?;
//!
//! // Subscribe to domains after connect
//! conn.subscribe(vec!["widget_bridge:blackflag".into()])?;
//!
//! // With actions
//! let conn = NotifyConnection::<WidgetEvent, MyAction>::builder()
//!     .url("wss://example.com/ws")
//!     .on_notify(|domain, event, _| { /* handle */ })
//!     .on_action_complete(|op_id| { /* action succeeded */ })
//!     .connect()?;
//!
//! // Send an action
//! let op_id = conn.send(MyAction::Apply { placement_id })?;
//! ```

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::rc::Rc;

use crate::connection::{FlowConnection, FlowConnectionBuilder, FlowError, ReconnectConfig};
use crate::status::ConnectionStatus;
use ui_flow_protocol::OpId;

/// Unit type for connections that don't use state synchronization
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct NoState;

/// Unit type for connections that don't use delta updates
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct NoDelta;

/// Unit type for connections that don't send actions
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct NoAction;

/// Builder for notification-only connections.
///
/// This is a convenience wrapper around `FlowConnectionBuilder` that uses
/// unit types for State and Delta, simplifying the API for notification-only use cases.
#[allow(clippy::type_complexity)]
pub struct NotifyConnectionBuilder<Event, Action = NoAction> {
    url: Option<String>,
    reconnect_config: ReconnectConfig,
    on_status: Option<Rc<dyn Fn(ConnectionStatus)>>,
    on_connected: Option<Rc<dyn Fn(String)>>,
    on_notify: Option<Rc<dyn Fn(String, Event, Option<OpId>)>>,
    on_action_complete: Option<Rc<dyn Fn(OpId)>>,
    on_action_error: Option<Rc<dyn Fn(OpId, Option<String>, String)>>,
    on_error: Option<Rc<dyn Fn(String, bool)>>,
    _action: std::marker::PhantomData<Action>,
}

impl<Event, Action> NotifyConnectionBuilder<Event, Action>
where
    Event: DeserializeOwned + 'static,
    Action: Serialize + 'static,
{
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            url: None,
            reconnect_config: ReconnectConfig::default(),
            on_status: None,
            on_connected: None,
            on_notify: None,
            on_action_complete: None,
            on_action_error: None,
            on_error: None,
            _action: std::marker::PhantomData,
        }
    }

    /// Set the WebSocket URL
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Configure reconnection behavior
    pub fn reconnect_config(mut self, config: ReconnectConfig) -> Self {
        self.reconnect_config = config;
        self
    }

    /// Callback when connection status changes
    pub fn on_status<F>(mut self, f: F) -> Self
    where
        F: Fn(ConnectionStatus) + 'static,
    {
        self.on_status = Some(Rc::new(f));
        self
    }

    /// Callback when connection is established (receives connection_id)
    pub fn on_connected<F>(mut self, f: F) -> Self
    where
        F: Fn(String) + 'static,
    {
        self.on_connected = Some(Rc::new(f));
        self
    }

    /// Callback when a notification event is received.
    ///
    /// Parameters: (domain, event, correlation_id)
    /// - `domain`: The notification domain (e.g., "widget_bridge:blackflag")
    /// - `event`: The deserialized event
    /// - `correlation_id`: Optional OpId linking to a triggering action
    pub fn on_notify<F>(mut self, f: F) -> Self
    where
        F: Fn(String, Event, Option<OpId>) + 'static,
    {
        self.on_notify = Some(Rc::new(f));
        self
    }

    /// Callback when an action completes successfully
    pub fn on_action_complete<F>(mut self, f: F) -> Self
    where
        F: Fn(OpId) + 'static,
    {
        self.on_action_complete = Some(Rc::new(f));
        self
    }

    /// Callback when an action fails (op_id, error_code, message)
    pub fn on_action_error<F>(mut self, f: F) -> Self
    where
        F: Fn(OpId, Option<String>, String) + 'static,
    {
        self.on_action_error = Some(Rc::new(f));
        self
    }

    /// Callback for connection/protocol errors (message, is_fatal)
    pub fn on_error<F>(mut self, f: F) -> Self
    where
        F: Fn(String, bool) + 'static,
    {
        self.on_error = Some(Rc::new(f));
        self
    }

    /// Build and connect
    pub fn connect(self) -> Result<NotifyConnection<Event, Action>, FlowError> {
        let url = self
            .url
            .ok_or_else(|| FlowError::Configuration("URL is required".into()))?;

        // Build the underlying FlowConnection with unit State/Delta types
        let mut builder: FlowConnectionBuilder<NoState, NoDelta, Event, Action> =
            FlowConnection::builder();

        builder = builder.url(&url).reconnect_config(self.reconnect_config);

        // Wire up callbacks
        if let Some(cb) = self.on_status {
            builder = builder.on_status(move |s| cb(s));
        }

        if let Some(cb) = self.on_connected {
            builder = builder.on_connected(move |conn_id| cb(conn_id));
        }

        if let Some(cb) = self.on_notify {
            builder = builder.on_notify(move |domain, event, correlation_id| {
                cb(domain, event, correlation_id);
            });
        }

        if let Some(cb) = self.on_action_complete {
            builder = builder.on_action_complete(move |op_id| cb(op_id));
        }

        if let Some(cb) = self.on_action_error {
            builder = builder.on_action_error(move |op_id, code, msg| cb(op_id, code, msg));
        }

        if let Some(cb) = self.on_error {
            builder = builder.on_error(move |msg, fatal| cb(msg, fatal));
        }

        // Ignore state callbacks - we don't use them in notify-only mode
        builder = builder
            .on_snapshot(|_, _| {})
            .on_delta(|_, _| {})
            .on_deltas(|_, _| {});

        let inner = builder.connect()?;

        Ok(NotifyConnection {
            inner,
            _event: std::marker::PhantomData,
        })
    }
}

impl<Event, Action> Default for NotifyConnectionBuilder<Event, Action>
where
    Event: DeserializeOwned + 'static,
    Action: Serialize + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Connection handle for notification-only mode.
///
/// Provides a simplified API focused on receiving notifications and
/// optionally sending actions, without state synchronization.
pub struct NotifyConnection<Event, Action = NoAction> {
    inner: FlowConnection<Action>,
    _event: std::marker::PhantomData<Event>,
}

impl<Event, Action> NotifyConnection<Event, Action>
where
    Event: DeserializeOwned + 'static,
    Action: Serialize + 'static,
{
    /// Create a new builder
    pub fn builder() -> NotifyConnectionBuilder<Event, Action> {
        NotifyConnectionBuilder::new()
    }

    /// Get current connection status
    pub fn status(&self) -> ConnectionStatus {
        self.inner.status()
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    /// Disconnect and clean up
    pub fn disconnect(&self) {
        self.inner.disconnect();
    }

    /// Subscribe to notification domains.
    ///
    /// Domains use the format `"{type}:{scope}"`, e.g.:
    /// - `"widget_bridge:blackflag"` - Widget events for blackflag world
    /// - `"rewards:blackflag"` - Reward events for blackflag world
    ///
    /// Call this after connection is established (in `on_connected` callback
    /// or after checking `is_connected()`).
    pub fn subscribe(&self, domains: Vec<String>) -> Result<(), FlowError> {
        self.inner.subscribe(domains)
    }

    /// Unsubscribe from domains
    pub fn unsubscribe(&self, domains: Vec<String>) -> Result<(), FlowError> {
        self.inner.unsubscribe(domains)
    }

    /// Send an action to the server, returning the operation ID for tracking.
    ///
    /// Use `on_action_complete` and `on_action_error` callbacks to handle results.
    pub fn send(&self, action: Action) -> Result<OpId, FlowError> {
        let op_id = OpId::new();
        self.inner.send_action(op_id, action)?;
        Ok(op_id)
    }

    /// Send an action with a specific operation ID
    pub fn send_with_id(&self, op_id: OpId, action: Action) -> Result<(), FlowError> {
        self.inner.send_action(op_id, action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_types_serialize() {
        // Ensure unit types can serialize/deserialize
        let state = NoState;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "null");

        let delta = NoDelta;
        let json = serde_json::to_string(&delta).unwrap();
        assert_eq!(json, "null");

        let action = NoAction;
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(json, "null");
    }
}
