//! Discord Activities for macroquad/miniquad apps — no wasm-bindgen.
//!
//! Discord's Embedded App SDK is a TypeScript wrapper over a `postMessage` RPC
//! with the host client. This crate speaks that protocol directly, so a
//! macroquad game can be an Activity without the SDK, and without wasm-bindgen
//! — which cannot run under miniquad at all.
//!
//! # Shape
//!
//! Identical to `wallet-miniquad`, deliberately: async work is fired as a
//! request that returns a [`ReqId`], and polled across frames. A game loop has
//! nowhere to await, so the bridge doesn't pretend otherwise.
//!
//! ```no_run
//! use discord_miniquad::{Activity, Phase};
//!
//! // In your app state:
//! let mut activity = Activity::new("YOUR_CLIENT_ID");
//!
//! // Once per frame:
//! match activity.update() {
//!     Phase::Connecting => { /* draw a splash */ }
//!     Phase::Ready => { /* the RPC channel is open */ }
//!     Phase::Failed(err) => { /* not in Discord, or handshake refused */ }
//! }
//! ```
//!
//! # Setup
//!
//! Serve `js/discord.js` alongside the wasm and load it before the miniquad
//! bundle, the same way `wallet.js` is loaded. Discord launches the iframe
//! with `?frame_id=…`, which the handshake requires.
//!
//! # What this does not do
//!
//! No OAuth token exchange — that step is a POST to *your* server (Discord's
//! sample uses `/.proxy/api/token`, since Activity network calls route through
//! their proxy). Trading a code for a token in the client would leak the
//! client secret, so the crate deliberately stops at handing you the code.

mod bridge;

pub use bridge::{launch_query, LaunchContext, PollResult, ReqId};

/// The JS half of the bridge, for stamping into a web build.
///
/// Same convention as `wallet_miniquad::PLUGIN_JS`: an app's `build.rs` writes
/// this to `web/discord.js` so the plugin ships in lockstep with the Rust that
/// calls it — a stale copy would fail on the missing imports at instantiate.
///
/// ```no_run
/// # fn main() -> std::io::Result<()> {
/// std::fs::write("web/discord.js", discord_miniquad::PLUGIN_JS)?;
/// # Ok(()) }
/// ```
pub const PLUGIN_JS: &str = include_str!("../js/discord.js");

/// Plugin version handshake. miniquad's `gl.js` looks for an export named
/// `<plugin>_crate_version` and compares it to the plugin's JS `version`
/// field; `discord.js` declares `version: 1`. Without this export gl.js logs
/// "Plugin discord is present in JS bundle, but is not used in the rust code"
/// at boot — cosmetic on the open web, but it is the sanctioned handshake and
/// the same hook every sibling plugin (`wallet`, `platform`) exports.
///
/// Bump both sides together when the JS surface changes incompatibly.
///
/// Only emitted on wasm32 — on native the symbol is meaningless. Edition
/// 2024: `no_mangle` is an unsafe attribute.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
extern "C" fn discord_crate_version() -> u32 {
    1
}

/// Where the connection has got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Handshake sent, waiting for `READY`.
    Connecting,
    /// RPC channel open — [`Activity::command`] will be accepted.
    Ready,
    /// Not running inside Discord, or the client refused the handshake.
    Failed(String),
}

/// A connection to the host Discord client.
///
/// Drive it by calling [`Activity::update`] once per frame.
pub struct Activity {
    context: LaunchContext,
    connect: Option<ReqId>,
    phase: Phase,
}

impl Activity {
    /// Begin the handshake.
    ///
    /// Cheap and non-blocking: outside Discord this settles to
    /// [`Phase::Failed`] on the first [`Activity::update`] rather than hanging,
    /// so a game can run standalone in a browser tab and only light up its
    /// Discord features when embedded.
    pub fn new(client_id: &str) -> Self {
        let context = bridge::launch_context();
        let connect = context.is_embedded().then(|| bridge::connect(client_id));

        Self {
            phase: if connect.is_some() {
                Phase::Connecting
            } else {
                Phase::Failed("not launched as a Discord Activity".to_string())
            },
            context,
            connect,
        }
    }

    /// Advance the handshake. Call once per frame.
    pub fn update(&mut self) -> Phase {
        if let Some(id) = self.connect {
            match bridge::poll(id) {
                PollResult::Pending => {}
                PollResult::Ok { .. } => {
                    self.connect = None;
                    self.phase = Phase::Ready;
                }
                PollResult::Err { data } => {
                    self.connect = None;
                    self.phase = Phase::Failed(data);
                }
            }
        }
        self.phase.clone()
    }

    /// What Discord passed at launch — including `custom_id`, the passthrough
    /// an interaction attached when it opened this Activity.
    pub fn context(&self) -> &LaunchContext {
        &self.context
    }

    /// Issue an RPC command once [`Phase::Ready`].
    ///
    /// Generic on purpose. The SDK enumerates ~29 commands; the wire shape is
    /// the same for all of them, so wrapping each here would be a second list
    /// to keep in sync with Discord's. Build `args` with serde and parse the
    /// response the same way — you know which command you sent.
    ///
    /// ```no_run
    /// # use discord_miniquad::Activity;
    /// # let activity = Activity::new("id");
    /// let req = activity.command("authorize", r#"{"scopes":["identify"]}"#);
    /// ```
    pub fn command(&self, cmd: &str, args_json: &str) -> ReqId {
        bridge::command(cmd, args_json)
    }

    /// Poll a request issued by [`Activity::command`] or [`Activity::http_post`].
    pub fn poll(&self, id: ReqId) -> PollResult {
        bridge::poll(id)
    }

    /// Same-origin JSON POST, resolved under the Activity's mapped root.
    ///
    /// Exists for the code-for-token exchange: `AUTHORIZE` yields a code, the
    /// client secret lives on the server, so the code goes up as
    /// `http_post("api/token", …)` and a token comes back. Discord's proxy
    /// rewrites the relative URL to the mapped host, which is why it is
    /// relative and not absolute — an absolute URL to a host without a mapping
    /// is blocked by the Activity CSP.
    ///
    /// Deliberately not a general HTTP client. `Ok { data }` is the response
    /// text on 2xx; `Err { data }` is `"<status>: <body>"` otherwise.
    pub fn http_post(&self, url: &str, body_json: &str) -> ReqId {
        bridge::http_post(url, body_json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Off-wasm there is no Discord, and the type must say so without
    /// panicking — a macroquad game is developed natively and only ever meets
    /// a real client at the end.
    #[test]
    fn native_reports_not_embedded() {
        let mut activity = Activity::new("client");
        assert!(!activity.context().is_embedded());
        assert!(matches!(activity.update(), Phase::Failed(_)));
    }

    #[test]
    fn launch_context_tolerates_missing_params() {
        // Every field optional: launch surfaces differ in what they attach,
        // and a missing `channel_id` must not cost us the `custom_id` beside
        // it.
        let ctx: LaunchContext = serde_json::from_str(r#"{"frame_id":"abc","custom_id":"run_42"}"#)
            .expect("partial context must parse");
        assert!(ctx.is_embedded());
        assert_eq!(ctx.custom_id.as_deref(), Some("run_42"));
        assert!(ctx.channel_id.is_none());
    }

    #[test]
    fn empty_context_is_not_embedded() {
        let ctx: LaunchContext = serde_json::from_str("{}").unwrap();
        assert!(!ctx.is_embedded());
    }

    #[test]
    fn poll_results_parse() {
        let pending: PollResult = serde_json::from_str(r#"{"status":"pending"}"#).unwrap();
        assert!(matches!(pending, PollResult::Pending));

        let ok: PollResult = serde_json::from_str(r#"{"status":"ok","data":"{}"}"#).unwrap();
        assert!(matches!(ok, PollResult::Ok { .. }));

        let err: PollResult = serde_json::from_str(r#"{"status":"err","data":"nope"}"#).unwrap();
        assert!(matches!(err, PollResult::Err { .. }));
    }
}
