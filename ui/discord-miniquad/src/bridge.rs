//! Discord Embedded App RPC over the miniquad plugin protocol.
//!
//! Same shape as `wallet-miniquad`: each call returns a [`ReqId`] immediately,
//! the result lands in a JS-side map, and Rust [`poll`]s each frame until the
//! status flips off `Pending`. One request → one id; `poll` consumes the entry.
//!
//! Native builds get stubs so a macroquad app using this still compiles (and
//! runs, degraded) on desktop — the same `#[cfg]` split `wallet-miniquad` uses.

use serde::Deserialize;

/// Opaque handle to an in-flight Discord request.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct ReqId(pub i32);

/// Outcome of polling a request. `Pending` means try again next frame.
///
/// `data` is a JSON string in both terminal cases: the command's response
/// payload on `Ok`, and Discord's error payload (or a transport message) on
/// `Err`. Parsing is the caller's job, because only the caller knows which
/// command it issued.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum PollResult {
    Pending,
    Ok { data: String },
    Err { data: String },
}

/// What Discord passed the iframe at launch, from the query string.
///
/// [`Self::custom_id`] is the passthrough slot: whatever an interaction
/// attached when responding with `LAUNCH_ACTIVITY` shows up here, which is how
/// an Activity learns *what it was opened for* — a mission run id, a squad, a
/// match — without a round trip.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LaunchContext {
    /// Required by the handshake. Absent means we are not inside Discord.
    #[serde(default)]
    pub frame_id: Option<String>,
    #[serde(default)]
    pub instance_id: Option<String>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub custom_id: Option<String>,
    #[serde(default)]
    pub referrer_id: Option<String>,
    #[serde(default)]
    pub guild_id: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub location_id: Option<String>,
    #[serde(default)]
    pub mobile_app_version: Option<String>,
}

impl LaunchContext {
    /// Whether this looks like a real Activity launch.
    ///
    /// `frame_id` is the one parameter the handshake cannot proceed without,
    /// so its presence is the honest test — not `instance_id`, which is absent
    /// in some launch surfaces.
    pub fn is_embedded(&self) -> bool {
        self.frame_id.is_some()
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::{LaunchContext, PollResult, ReqId};
    use sapp_jsutils::JsObject;

    // `wasm_import_module = "env"` is load-bearing since Rust 1.96: rustc no
    // longer passes `--allow-undefined` to wasm-ld, so an extern block whose
    // bodies JS supplies at instantiate is a hard link error unless it says
    // which import module it belongs to. "env" is where miniquad's plugin
    // protocol registers everything (`importObject.env.<name>`).
    // https://blog.rust-lang.org/2026/04/04/changes-to-webassembly-targets-and-handling-undefined-symbols/
    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        fn discord_launch_context() -> JsObject;
        fn discord_launch_query(key: JsObject) -> JsObject;
        fn discord_connect(client_id: JsObject) -> i32;
        fn discord_command(cmd: JsObject, args_json: JsObject) -> i32;
        fn discord_http_post(url: JsObject, body: JsObject) -> i32;
        fn discord_poll(req_id: i32) -> JsObject;
    }

    fn js_to_string(js: JsObject) -> String {
        let mut s = String::new();
        js.to_string(&mut s);
        s
    }

    pub fn launch_context() -> LaunchContext {
        serde_json::from_str(&js_to_string(unsafe { discord_launch_context() }))
            .unwrap_or_default()
    }

    pub fn launch_query(key: &str) -> Option<String> {
        let v = js_to_string(unsafe { discord_launch_query(JsObject::string(key)) });
        (!v.is_empty()).then_some(v)
    }

    pub fn connect(client_id: &str) -> ReqId {
        ReqId(unsafe { discord_connect(JsObject::string(client_id)) })
    }

    pub fn command(cmd: &str, args_json: &str) -> ReqId {
        ReqId(unsafe { discord_command(JsObject::string(cmd), JsObject::string(args_json)) })
    }

    pub fn http_post(url: &str, body: &str) -> ReqId {
        ReqId(unsafe { discord_http_post(JsObject::string(url), JsObject::string(body)) })
    }

    pub fn poll(id: ReqId) -> PollResult {
        serde_json::from_str(&js_to_string(unsafe { discord_poll(id.0) })).unwrap_or(
            PollResult::Err {
                data: "malformed poll payload".to_string(),
            },
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::{LaunchContext, PollResult, ReqId};

    /// Native builds are never inside Discord, so the context is empty and
    /// `is_embedded()` is false — which is what a caller should branch on
    /// rather than on `cfg`.
    pub fn launch_context() -> LaunchContext {
        LaunchContext::default()
    }

    pub fn launch_query(_key: &str) -> Option<String> {
        None
    }

    pub fn connect(_client_id: &str) -> ReqId {
        ReqId(0)
    }

    pub fn command(_cmd: &str, _args_json: &str) -> ReqId {
        ReqId(0)
    }

    pub fn http_post(_url: &str, _body: &str) -> ReqId {
        ReqId(0)
    }

    pub fn poll(_id: ReqId) -> PollResult {
        PollResult::Err {
            data: "discord bridge is only available on wasm32".to_string(),
        }
    }
}

pub use imp::{command, connect, http_post, launch_context, launch_query, poll};
