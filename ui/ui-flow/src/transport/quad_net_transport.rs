//! quad-net WebSocket transport implementation
//!
//! This transport uses quad-net's WebSocket for macroquad/miniquad applications.
//! It provides a native WebSocket implementation on the web and uses ws-rs on desktop.

use quad_net::web_socket::WebSocket;

use super::{WebSocketEvent, WebSocketTransport};

/// WebSocket transport using quad-net (for macroquad/miniquad)
pub struct QuadNetTransport {
    ws: WebSocket,
    opened: bool,
    closed: bool,
}

/// Error type for quad-net transport
#[derive(Debug)]
pub enum QuadNetTransportError {
    /// Failed to create WebSocket connection
    Connection(String),
    /// Not connected
    NotConnected,
}

impl std::fmt::Display for QuadNetTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connection(msg) => write!(f, "Failed to connect: {msg}"),
            Self::NotConnected => write!(f, "Not connected"),
        }
    }
}

impl std::error::Error for QuadNetTransportError {}

impl WebSocketTransport for QuadNetTransport {
    type Error = QuadNetTransportError;

    fn connect(url: &str) -> Result<Self, Self::Error> {
        let ws = WebSocket::connect(url)
            .map_err(|e| QuadNetTransportError::Connection(format!("{e:?}")))?;

        Ok(Self {
            ws,
            opened: false,
            closed: false,
        })
    }

    fn send(&self, data: &[u8]) -> Result<(), Self::Error> {
        if !self.ws.connected() {
            return Err(QuadNetTransportError::NotConnected);
        }
        self.ws.send_bytes(data);
        Ok(())
    }

    fn poll(&mut self) -> Option<WebSocketEvent> {
        // Check for connection state changes
        let is_connected = self.ws.connected();

        // Emit Open event once when we become connected
        if is_connected && !self.opened {
            self.opened = true;
            return Some(WebSocketEvent::Open);
        }

        // Emit Close event once when we become disconnected (after having been connected)
        if !is_connected && self.opened && !self.closed {
            self.closed = true;
            return Some(WebSocketEvent::Close {
                code: 1000,
                reason: String::new(),
            });
        }

        // Check for incoming messages
        if let Some(bytes) = self.ws.try_recv() {
            return Some(WebSocketEvent::Message(bytes));
        }

        None
    }

    fn is_connected(&self) -> bool {
        self.ws.connected()
    }

    fn close(&mut self) {
        // quad-net doesn't have an explicit close method,
        // dropping the WebSocket will close it
        self.closed = true;
    }
}
