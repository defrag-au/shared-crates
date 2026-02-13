//! # flow-demo
//!
//! Demo worker showcasing the unified realtime protocol with a Leptos frontend.
//!
//! This worker demonstrates:
//! - WebSocket connections via Durable Objects
//! - MessagePack binary protocol
//! - Snapshot + delta state synchronization
//! - Presence tracking
//! - Optimistic UI with action feedback
//!
//! ## Endpoints
//!
//! - `GET /ws/:room_id` - Counter/chat demo WebSocket
//! - `GET /memory/:room_id` - Memory game WebSocket

pub mod assets;
mod memory_session;
mod session;
mod types;

pub use memory_session::MemoryGameSessionDO;
pub use session::FlowDemoSessionDO;

use tracing::Level;
use worker::*;

/// Initialize tracing and panic hook
#[event(start)]
fn start() {
    worker_utils::init_tracing(Some(Level::DEBUG));
    worker_utils::set_panic_hook();
}

/// Main fetch handler
#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    Router::new()
        .get("/health", |_, _| Response::ok("OK"))
        .get_async("/ws/:room_id", handle_demo_websocket)
        .get_async("/memory/:room_id", handle_memory_websocket)
        .run(req, env)
        .await
}

/// Handle WebSocket upgrade request for the counter/chat demo
async fn handle_demo_websocket(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let room_id = ctx
        .param("room_id")
        .map(|s| s.as_str())
        .unwrap_or("default");

    let namespace = ctx.env.durable_object("FLOW_SESSIONS")?;
    let stub = namespace.id_from_name(room_id)?.get_stub()?;
    stub.fetch_with_request(req).await
}

/// Handle WebSocket upgrade request for memory game rooms
async fn handle_memory_websocket(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let room_id = ctx
        .param("room_id")
        .map(|s| s.as_str())
        .unwrap_or("default");

    let namespace = ctx.env.durable_object("MEMORY_SESSIONS")?;
    let stub = namespace.id_from_name(room_id)?.get_stub()?;
    stub.fetch_with_request(req).await
}
