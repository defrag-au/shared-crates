//! Does `ui-flow` work under macroquad, end to end, against a real server?
//!
//! `ui-flow` has a `macroquad` feature — quad-net for the socket, miniquad for
//! time, no wasm-bindgen — but **nothing has ever used it**. It compiles; that
//! is not the same as working. This connects it to a live ui-flow service and
//! reports exactly what arrives.
//!
//! # Why a separate probe rather than testing inside the Activity
//!
//! Because the Activity conflates three unknowns, and the first attempt proved
//! how expensive that is: a raw-socket echo test in the Discord iframe timed
//! out, and the cause turned out to be our own echo endpoint failing to
//! outlive its response — not Discord, not macroquad, not quad-net. Three
//! suspects, one symptom, no way to tell them apart.
//!
//! So separate them:
//!
//! 1. **This probe in a plain browser** → does the macroquad ui-flow client
//!    work at all? `flow-demo` is deployed and its Leptos frontend already
//!    talks to it, so the server is known-good and a failure here is ours.
//! 2. **This probe inside the Activity** → does Discord's proxy pass a
//!    WebSocket? Only meaningful once (1) is green.
//!
//! A red (2) after a green (1) is a real finding about Discord. A red (2) on
//! its own means nothing, which is exactly where the last round left us.
//!
//! # Polling, not callbacks
//!
//! `FlowConnection` (callback-based) is `web-sys` only —
//! `PollingFlowConnection` is the one that "works with both transports". That
//! suits macroquad regardless: a game loop already has a natural place to
//! drain events, and it avoids `Rc<RefCell<_>>` plumbing entirely.
//!
//! # Service-agnostic on purpose
//!
//! `State`/`Delta`/`Event` are `serde_json::Value`. ui-flow only asks for
//! `DeserializeOwned`, so the probe never needs a service's real types and can
//! be aimed anywhere — flow-demo now, a missions DO later.

use macroquad::prelude::*;
use ui_flow::{FlowEvent, PollingFlowConnection};

/// Default target: the deployed `flow-demo` worker, whose Leptos frontend is
/// already known to work against it.
const DEFAULT_URL: &str = "wss://flowdemo.cnft.dev/ws/mq-probe";

type Json = serde_json::Value;
type Connection = PollingFlowConnection<Json, Json, Json, Json>;

/// Everything the probe has learned.
#[derive(Default)]
struct Log {
    status: String,
    connection_id: Option<String>,
    snapshots: u32,
    deltas: u32,
    events: u32,
    errors: Vec<String>,
    /// Most recent payload, truncated — proof that bytes arrived *and parsed*,
    /// which is more convincing than a counter alone.
    last: Option<String>,
}

impl Log {
    fn note(&mut self, value: &Json) {
        self.last = Some(value.to_string().chars().take(160).collect());
    }
}

fn window_conf() -> Conf {
    Conf {
        window_title: "ui-flow × macroquad probe".to_string(),
        window_width: 960,
        window_height: 540,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let url = DEFAULT_URL.to_string();
    let mut log = Log {
        status: "connecting…".into(),
        ..Default::default()
    };

    let mut connection = match Connection::connect(&url) {
        Ok(connection) => Some(connection),
        Err(e) => {
            log.errors.push(format!("connect failed outright: {e:?}"));
            log.status = "failed".into();
            None
        }
    };

    loop {
        // Drain everything queued this frame. One event per frame would make a
        // burst of deltas look like a slow connection.
        if let Some(connection) = connection.as_mut() {
            while let Some(event) = connection.poll() {
                apply(&mut log, event);
            }
            log.status = format!("{:?}", connection.status());
        }

        clear_background(Color::from_rgba(0x0b, 0x0d, 0x12, 255));
        draw(&log, &url, connection.as_ref());
        next_frame().await;
    }
}

fn apply(log: &mut Log, event: FlowEvent<Json, Json, Json>) {
    match event {
        FlowEvent::Connected(id) => log.connection_id = Some(id),
        FlowEvent::Snapshot { state, .. } => {
            log.snapshots += 1;
            log.note(&state);
        }
        FlowEvent::Delta { delta, .. } => {
            log.deltas += 1;
            log.note(&delta);
        }
        FlowEvent::Deltas { deltas, .. } => {
            log.deltas += deltas.len() as u32;
            if let Some(last) = deltas.last() {
                log.note(last);
            }
        }
        FlowEvent::Notify { event, .. } => {
            log.events += 1;
            log.note(&event);
        }
        FlowEvent::ActionErr { message, .. } => log.errors.push(message),
        // Presence, progress, action-ok and status changes are all real
        // traffic but say nothing extra about whether the transport works.
        _ => {}
    }
}

fn draw(log: &Log, url: &str, connection: Option<&Connection>) {
    let fog = Color::from_rgba(0xd5, 0xd9, 0xe0, 255);
    let mist = Color::from_rgba(0x8b, 0x93, 0xa1, 255);
    let good = Color::from_rgba(0x57, 0xf2, 0x87, 255);
    let bad = Color::from_rgba(0xfb, 0x71, 0x85, 255);

    let x = 40.0;
    // `y` is threaded through explicitly rather than captured: a closure
    // holding `&mut y` blocks the `y += …` spacing nudges between sections.
    let mut y = 60.0;
    let mut line = |y: &mut f32, text: &str, size: f32, colour: Color| {
        draw_text(text, x, *y, size, colour);
        *y += size * 1.35;
    };

    line(&mut y, "ui-flow x macroquad probe", 34.0, fog);
    y += 6.0;
    line(&mut y, url, 18.0, mist);
    y += 10.0;

    // `is_connected` is ui-flow's own view — the thing under test — rather
    // than our inference from events having arrived.
    let connected = connection.map(|c| c.is_connected()).unwrap_or(false);
    line(
        &mut y,
        &format!("connected: {connected}   status: {}", log.status),
        22.0,
        if connected { good } else { bad },
    );
    if let Some(id) = &log.connection_id {
        line(&mut y, &format!("connection id: {id}"), 18.0, mist);
    }

    y += 8.0;
    line(
        &mut y,
        &format!(
            "snapshots {}   deltas {}   events {}",
            log.snapshots, log.deltas, log.events
        ),
        22.0,
        if log.snapshots > 0 { good } else { mist },
    );

    // A snapshot is the real proof: the socket opened, framed correctly, and
    // the payload deserialised. A connection that opens and stalls — the
    // failure mode the earlier echo test could not distinguish — stops short
    // of this line.
    if log.snapshots > 0 {
        y += 6.0;
        line(
            &mut y,
            "SNAPSHOT RECEIVED - transport works end to end",
            22.0,
            good,
        );
    }

    if let Some(last) = &log.last {
        y += 8.0;
        line(&mut y, "last payload:", 18.0, mist);
        for chunk in last.as_bytes().chunks(88) {
            line(&mut y, &String::from_utf8_lossy(chunk), 16.0, mist);
        }
    }

    for err in log.errors.iter().take(4) {
        y += 4.0;
        line(&mut y, err, 16.0, bad);
    }
}
