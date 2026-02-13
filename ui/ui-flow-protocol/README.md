# ui-flow-protocol

Wire protocol types for the unified realtime communication system.

## Overview

This crate defines the message format for client-server WebSocket communication using:

- **u16 integer tags** for message type discrimination (efficient, forward-compatible)
- **MessagePack** binary serialization (compact, fast)
- **Generic type parameters** for application-specific State, Delta, Event, and Action types

## Message Types

### Server → Client

| Tag | Name | Purpose |
|-----|------|---------|
| 1000 | Connected | Connection established, provides session ID |
| 1001 | Snapshot | Full state synchronization |
| 1002 | Delta | Single state change |
| 1003 | Deltas | Batched state changes |
| 1004 | Presence | Online users list |
| 1005 | Notify | Application event (not state change) |
| 1006 | ActionOk | Action completed successfully |
| 1007 | ActionErr | Action failed |
| 1008 | Progress | Long-running action progress |
| 1009 | Pong | Keepalive response |
| 1010 | Error | Protocol/server error |
| 1011 | Signal | WebRTC signalling |

### Client → Server

| Tag | Name | Purpose |
|-----|------|---------|
| 2000 | Subscribe | Subscribe to domain events |
| 2001 | Unsubscribe | Unsubscribe from domain |
| 2002 | Action | Request state change |
| 2003 | Ping | Keepalive request |
| 2004 | Signal | WebRTC signalling |
| 2005 | SetPresence | Update presence status |

## Usage

### Define your application types

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyState {
    pub counter: u64,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MyDelta {
    CounterChanged { value: u64 },
    ItemAdded { item: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MyEvent {
    UserJoined { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MyAction {
    Increment,
    AddItem { text: String },
}
```

### Encode and decode messages

```rust
use ui_flow_protocol::{ServerMessage, ClientMessage, OpId, encode, decode};

// Type aliases for convenience
type ServerMsg = ServerMessage<MyState, MyDelta, MyEvent>;
type ClientMsg = ClientMessage<MyAction>;

// Server sending a snapshot
let msg: ServerMsg = ServerMessage::Snapshot {
    state: MyState { counter: 0, items: vec![] },
    seq: 1,
    timestamp: 1234567890,
};
let bytes = encode(&msg)?;

// Client sending an action
let msg: ClientMsg = ClientMessage::action(OpId::new(), MyAction::Increment);
let bytes = encode(&msg)?;

// Decoding
let decoded: ServerMsg = decode(&bytes)?;
```

## Wire Format

Messages are serialized as MessagePack with the following structure:

```
{
  "t": 1001,           // u16 tag (discriminates message type)
  ...payload fields    // varies by message type
}
```

The `t` field uses `#[serde(rename = "t")]` and integer values via `serde_repr` for efficiency.

## Design Rationale

See the full design document at `docs/unified-realtime-protocol.md` for:

- Why u16 integer tags instead of string tags
- Snapshot + Delta synchronization pattern
- Optimistic UI with OpId correlation
- Presence and notification design
