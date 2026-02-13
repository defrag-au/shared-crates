//! web-sys WebSocket transport implementation

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CloseEvent, MessageEvent, WebSocket};

use super::{WebSocketEvent, WebSocketTransport};

/// WebSocket transport using web-sys (standard browser WebSocket)
pub struct WebSysTransport {
    ws: WebSocket,
    events: Rc<RefCell<VecDeque<WebSocketEvent>>>,
    connected: Rc<RefCell<bool>>,
    // Store closures to prevent them from being dropped
    _closures: Vec<Closure<dyn FnMut(JsValue)>>,
}

/// Error type for web-sys transport
#[derive(Debug)]
pub enum WebSysTransportError {
    /// Failed to create WebSocket
    Creation(String),
    /// Failed to send data
    Send(String),
    /// Not connected
    NotConnected,
}

impl std::fmt::Display for WebSysTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Creation(msg) => write!(f, "Failed to create WebSocket: {msg}"),
            Self::Send(msg) => write!(f, "Failed to send: {msg}"),
            Self::NotConnected => write!(f, "Not connected"),
        }
    }
}

impl std::error::Error for WebSysTransportError {}

impl WebSocketTransport for WebSysTransport {
    type Error = WebSysTransportError;

    fn connect(url: &str) -> Result<Self, Self::Error> {
        let ws =
            WebSocket::new(url).map_err(|e| WebSysTransportError::Creation(format!("{e:?}")))?;

        // Use binary mode for MessagePack
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let events: Rc<RefCell<VecDeque<WebSocketEvent>>> = Rc::new(RefCell::new(VecDeque::new()));
        let connected = Rc::new(RefCell::new(false));
        let mut closures = Vec::new();

        // onopen
        {
            let events = events.clone();
            let connected = connected.clone();
            let onopen = Closure::wrap(Box::new(move |_: JsValue| {
                *connected.borrow_mut() = true;
                events.borrow_mut().push_back(WebSocketEvent::Open);
            }) as Box<dyn FnMut(JsValue)>);
            ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
            closures.push(onopen);
        }

        // onmessage
        {
            let events = events.clone();
            let onmessage = Closure::wrap(Box::new(move |event: JsValue| {
                let event: MessageEvent = event.unchecked_into();
                let data = event.data();

                let bytes: Vec<u8> =
                    if let Some(array_buffer) = data.dyn_ref::<js_sys::ArrayBuffer>() {
                        let uint8_array = js_sys::Uint8Array::new(array_buffer);
                        uint8_array.to_vec()
                    } else if let Some(text) = data.as_string() {
                        // Fallback for text messages
                        text.into_bytes()
                    } else {
                        return;
                    };

                events
                    .borrow_mut()
                    .push_back(WebSocketEvent::Message(bytes));
            }) as Box<dyn FnMut(JsValue)>);
            ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            closures.push(onmessage);
        }

        // onclose
        {
            let events = events.clone();
            let connected = connected.clone();
            let onclose = Closure::wrap(Box::new(move |event: JsValue| {
                *connected.borrow_mut() = false;
                let event: CloseEvent = event.unchecked_into();
                events.borrow_mut().push_back(WebSocketEvent::Close {
                    code: event.code(),
                    reason: event.reason(),
                });
            }) as Box<dyn FnMut(JsValue)>);
            ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
            closures.push(onclose);
        }

        // onerror
        {
            let events = events.clone();
            let onerror = Closure::wrap(Box::new(move |_: JsValue| {
                events
                    .borrow_mut()
                    .push_back(WebSocketEvent::Error("WebSocket error".into()));
            }) as Box<dyn FnMut(JsValue)>);
            ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
            closures.push(onerror);
        }

        Ok(Self {
            ws,
            events,
            connected,
            _closures: closures,
        })
    }

    fn send(&self, data: &[u8]) -> Result<(), Self::Error> {
        if !*self.connected.borrow() {
            return Err(WebSysTransportError::NotConnected);
        }
        self.ws
            .send_with_u8_array(data)
            .map_err(|e| WebSysTransportError::Send(format!("{e:?}")))
    }

    fn poll(&mut self) -> Option<WebSocketEvent> {
        self.events.borrow_mut().pop_front()
    }

    fn is_connected(&self) -> bool {
        *self.connected.borrow()
    }

    fn close(&mut self) {
        let _ = self.ws.close();
        *self.connected.borrow_mut() = false;
    }
}
