//! How a game reaches the score API, and who it reaches it as.
//!
//! Identity and transport are one concept here, not two, because the surface
//! decides both together. Inside Discord the player is whoever the Activity
//! OAuth chain resolves to and the API is reachable only as a relative URL
//! under the mapped root; in a browser tab the player is whoever the handoff
//! token names and the API is an absolute origin. Splitting them would mean
//! every game wiring up a matching pair by hand.
//!
//! A backend is polled, never awaited: a macroquad frame has nowhere to await,
//! and pretending otherwise is how a game loop ends up blocking on a network
//! call.
//!
//! # Offline is a first-class state
//!
//! [`BackendState::Offline`] is not a failure to handle later — it is the
//! normal state on a native dev build, and the correct state when Discord is
//! not there. A game must stay playable in it; only the leaderboard goes away.

use serde::{Deserialize, Serialize};

#[cfg(feature = "discord")]
pub mod discord;
#[cfg(feature = "web")]
pub mod web;

/// Handle to an in-flight request.
pub type ReqId = u32;

/// The result of polling a request.
pub enum Poll {
    Pending,
    /// Response body (2xx).
    Ok(String),
    /// Transport failure, or a non-2xx status with its body.
    Err(String),
}

/// Whether the backend can carry a request yet.
#[derive(Clone, Debug, PartialEq)]
pub enum BackendState {
    /// Still working through connect/auth. The string is progress to show a
    /// player rather than a spinner with no explanation.
    Connecting(&'static str),
    /// Requests will be carried, authenticated.
    Ready,
    /// No server this session. Carries why, for the title screen — "not
    /// launched from Discord" is a different situation from "auth was
    /// refused", and a player who cannot tell will retry the wrong thing.
    Offline(String),
}

/// A connection to the score API, with an identity attached.
pub trait ArcadeBackend {
    /// Advance connect/auth work. Call once per frame, before anything else.
    fn update(&mut self) -> BackendState;

    /// POST JSON to an API path (`/api/seed`). Authentication is the
    /// backend's business — a caller never handles the token.
    ///
    /// Only meaningful once [`BackendState::Ready`]; calling earlier is
    /// allowed and fails through [`Poll::Err`] rather than panicking, because
    /// a race between "ready this frame" and "ready next frame" should not be
    /// a crash.
    fn post(&mut self, path: &str, body_json: &str) -> ReqId;

    /// Poll a request issued by [`Self::post`].
    fn poll(&mut self, id: ReqId) -> Poll;

    /// Display name for the local player, once known. Shown on the title
    /// screen so a player can see *who* the leaderboard will credit before
    /// they spend a run finding out.
    fn player_name(&self) -> Option<&str> {
        None
    }
}

/// No server: play, score locally, submit nothing.
///
/// The default on native builds, and the fallback when a surface cannot be
/// identified. Every request fails immediately rather than hanging, so a
/// session driving this backend moves straight past the network states
/// instead of waiting out a timeout that will never come.
pub struct OfflineBackend {
    reason: String,
}

impl OfflineBackend {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl Default for OfflineBackend {
    fn default() -> Self {
        Self::new("no arcade server on this build")
    }
}

impl ArcadeBackend for OfflineBackend {
    fn update(&mut self) -> BackendState {
        BackendState::Offline(self.reason.clone())
    }

    fn post(&mut self, _path: &str, _body_json: &str) -> ReqId {
        0
    }

    fn poll(&mut self, _id: ReqId) -> Poll {
        Poll::Err(self.reason.clone())
    }
}

/// Picks a backend from how the page was launched.
///
/// This is what lets one game binary serve both surfaces. A Discord Activity
/// iframe is launched with `?frame_id=…` — the handshake requires it, so its
/// presence is not a heuristic but the same fact `discord.js` keys off. Absent,
/// the page is an ordinary tab and the handoff token (if any) is in the URL.
///
/// Deciding synchronously matters: the alternative is starting the Discord
/// chain and falling back when it times out, which shows every browser-tab
/// player a "connecting to Discord" screen that is a lie.
#[cfg(all(feature = "discord", feature = "web"))]
pub enum AutoBackend {
    Discord(discord::DiscordBackend),
    Web(web::WebBackend),
}

#[cfg(all(feature = "discord", feature = "web"))]
impl AutoBackend {
    /// `mount` is the path prefix the bundle is served under inside Discord
    /// (`/arcade`); `api_base` is the absolute score-API origin used in a
    /// browser tab, where there is no proxy to route through.
    pub fn detect(client_id: &str, mount: &str, api_base: &str) -> Self {
        if miniquad_platform::launch_query("frame_id").is_some() {
            Self::Discord(discord::DiscordBackend::new(client_id).with_mount(mount))
        } else {
            Self::Web(web::WebBackend::from_page(api_base))
        }
    }
}

#[cfg(all(feature = "discord", feature = "web"))]
impl ArcadeBackend for AutoBackend {
    fn update(&mut self) -> BackendState {
        match self {
            Self::Discord(b) => b.update(),
            Self::Web(b) => b.update(),
        }
    }

    fn post(&mut self, path: &str, body_json: &str) -> ReqId {
        match self {
            Self::Discord(b) => b.post(path, body_json),
            Self::Web(b) => b.post(path, body_json),
        }
    }

    fn poll(&mut self, id: ReqId) -> Poll {
        match self {
            Self::Discord(b) => b.poll(id),
            Self::Web(b) => b.poll(id),
        }
    }

    fn player_name(&self) -> Option<&str> {
        match self {
            Self::Discord(b) => b.player_name(),
            Self::Web(b) => b.player_name(),
        }
    }
}

/// What the platform's Activity token exchange returns.
///
/// Shared by the Discord backend and any caller that runs the exchange itself.
/// Only `widget_token` is used here — `access_token` is Discord's, for RPC
/// commands the harness does not issue.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenExchangeResponse {
    pub access_token: String,
    pub widget_token: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub display_name: String,
}

/// What the platform's Activity token exchange expects.
#[derive(Debug, Serialize)]
pub struct TokenExchangeRequest<'a> {
    pub code: &'a str,
    pub client_id: &'a str,
    pub guild_id: Option<&'a str>,
}
