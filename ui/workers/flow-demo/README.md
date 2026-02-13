# flow-demo

A self-contained demonstration of the unified realtime protocol using Cloudflare Workers and Leptos.

## Purpose

This worker serves as:

1. **Reference implementation** - Shows how to build a realtime application using `ui-flow-protocol`
2. **Learning resource** - Well-documented code for Claude and other AI agents to understand the system
3. **Testing ground** - Quick way to verify the protocol works end-to-end

## Features

- **Counter** - Shared counter with increment/decrement (demonstrates delta sync)
- **Chat** - Real-time chat messages (demonstrates append-only state)
- **Presence** - Online users list (demonstrates presence tracking)

## Quick Start

### Local Development

**Terminal 1 - Worker:**
```bash
cd workers/flow-demo
wrangler dev
# Runs on http://localhost:8787
```

**Terminal 2 - Frontend (hot reload):**
```bash
cd workers/flow-demo/frontend
trunk serve
# Runs on http://localhost:8080 with live reload
# Proxies WebSocket to :8787
```

### Deploy to Cloudflare

```bash
cd workers/flow-demo
wrangler deploy --env dev
```

## Architecture

```
flow-demo/
├── src/
│   ├── lib.rs          # Worker entry point, HTTP router
│   ├── session.rs      # FlowDemoSessionDO - Durable Object for WebSocket state
│   └── types.rs        # DemoState, DemoDelta, DemoEvent, DemoAction
├── frontend/
│   ├── src/
│   │   ├── lib.rs      # Leptos app, WebSocket client
│   │   └── components/ # UI components (Counter, Chat, Presence)
│   ├── styles/
│   │   └── main.scss   # Dark theme styles
│   └── index.html      # WASM entry point
├── wrangler.toml       # Cloudflare config
└── README.md
```

## Protocol Flow

```
1. Client connects to /ws/:room_id
2. Worker routes to FlowDemoSessionDO (Durable Object)
3. DO upgrades to WebSocket, sends Connected message
4. DO sends Snapshot with current state
5. Client sends Action messages (Increment, SendMessage, etc.)
6. DO broadcasts Delta messages to all connected clients
7. Presence updates sent when users join/leave
```

## State Model

```rust
// Full state (sent in Snapshot)
struct DemoState {
    counter: u64,
    messages: Vec<ChatMessage>,
}

// State changes (sent in Delta)
enum DemoDelta {
    CounterChanged { value: u64 },
    MessageAdded { message: ChatMessage },
    UserJoined { user_id: String, user_name: String },
    UserLeft { user_id: String },
}

// Application events (sent in Notify, not state changes)
enum DemoEvent {
    Announcement { text: String },
    UserTyping { user_id: String, user_name: String },
}

// Client actions
enum DemoAction {
    Increment,
    Decrement,
    SendMessage { text: String },
    StartTyping,
}
```

## Key Patterns

### WebSocket Upgrade in Durable Object

```rust
async fn fetch(&self, req: Request) -> Result<Response> {
    let pair = WebSocketPair::new()?;
    self.state.accept_web_socket(&pair.server);
    
    // Store user info as attachment
    pair.server.serialize_attachment(&UserInfo { ... })?;
    
    Response::from_websocket(pair.client)
}
```

### Broadcasting to All Clients

```rust
fn broadcast<T: Serialize>(&self, msg: &T) -> Result<()> {
    let bytes = ui_flow_protocol::encode(msg)?;
    for ws in self.state.get_websockets() {
        let _ = ws.send_with_bytes(&bytes);
    }
    Ok(())
}
```

### Handling Client Actions

```rust
async fn websocket_message(&self, ws: WebSocket, msg: String) -> Result<()> {
    let client_msg: ClientMsg = ui_flow_protocol::decode(msg.as_bytes())?;
    
    match client_msg {
        ClientMessage::Action { op_id, action } => {
            match action {
                DemoAction::Increment => {
                    self.state.counter += 1;
                    self.broadcast(&ServerMessage::Delta {
                        delta: DemoDelta::CounterChanged { value: self.state.counter },
                        seq: self.next_seq(),
                        timestamp: now(),
                    })?;
                }
                // ... other actions
            }
        }
        // ... other message types
    }
}
```

## Extending This Demo

To add your own functionality:

1. **Add state fields** in `types.rs` `DemoState`
2. **Add delta variants** in `types.rs` `DemoDelta`  
3. **Add action variants** in `types.rs` `DemoAction`
4. **Handle actions** in `session.rs` `websocket_message()`
5. **Add UI components** in `frontend/src/components/`

## Notes

- No authentication (demo purposes only)
- Uses Hibernation API for cost-efficient idle connections
- Frontend uses Leptos 0.6 with client-side rendering (CSR)
- MessagePack binary protocol for efficiency
