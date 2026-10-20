//! `Connection` — a lockstep room client's connection for a macroquad game:
//! arcade-room's [`Client`] over quad-net's WebSocket, natively and on the
//! web.
//!
//! A game hands it its controls once a frame, already encoded as the room
//! expects ([`Connection::update`]), and reads the client back for what to
//! draw ([`Connection::client`]). Everything between (connecting, the hello,
//! the welcome, sending, noticing a lost connection, rejoining) is here, so a
//! second lockstep game does not copy it, and copies of workarounds do not
//! drift apart.
//!
//! # quad-net's limits, and what this does about them
//!
//! - **Nothing may be sent before the socket opens**: the browser throws.
//!   Frames wait until the socket says it is connected.
//! - **A close is never reported.** While seated, the room sends a batch every
//!   100 ms, so silence is the signal: [`Config::silence_ms`] without a frame
//!   is a lost connection.
//! - **Natively, connecting blocks and panics on failure, and so does sending
//!   on a closed connection.** Both are caught ([`QuadSocket`]) and treated as
//!   a lost connection, which reconnects after [`Config::retry_ms`].
//! - **On the web there is one socket per page, and its "connected" flag
//!   never resets**, so a second connection would send before it opened. The
//!   web client therefore does not reconnect in place: it says it is
//!   rejoining and reloads the page ([`OnLoss::Reload`]), which is what a
//!   phone needs after being locked or switching tabs anyway. The room keeps
//!   the seat for its grace period, and the same player name gets it back.
//!
//! Times passed in are wall-clock ms, not a game's frame clock, which stops
//! while a tab is hidden.
//!
//! The socket is behind [`Transport`], so the lifecycle is tested against an
//! in-process room without a network.

use std::panic::{AssertUnwindSafe, catch_unwind};

use arcade_room::client::hello;
use arcade_room::{Client, RoomSim, Seat};
use quad_net::web_socket::WebSocket;

/// A connection to a room: what [`Connection`] needs of a socket.
pub trait Transport: Sized {
    /// Open a connection to `url`. It need not be open yet.
    fn connect(url: &str) -> Result<Self, String>;
    /// Whether it is open: nothing is sent until it is.
    fn connected(&self) -> bool;
    /// Send one frame. An error is a lost connection.
    fn send(&self, bytes: &[u8]) -> Result<(), String>;
    /// The next frame received, if any.
    fn try_recv(&mut self) -> Option<Vec<u8>>;
}

/// quad-net's WebSocket, its panics caught.
pub struct QuadSocket(WebSocket);

impl Transport for QuadSocket {
    fn connect(url: &str) -> Result<Self, String> {
        match catch_unwind(AssertUnwindSafe(|| WebSocket::connect(url))) {
            Ok(Ok(socket)) => Ok(Self(socket)),
            Ok(Err(e)) => Err(format!("{e:?}")),
            Err(_) => Err("connecting panicked".to_string()),
        }
    }

    fn connected(&self) -> bool {
        self.0.connected()
    }

    fn send(&self, bytes: &[u8]) -> Result<(), String> {
        catch_unwind(AssertUnwindSafe(|| self.0.send_bytes(bytes)))
            .map_err(|_| "the connection closed".to_string())
    }

    fn try_recv(&mut self) -> Option<Vec<u8>> {
        self.0.try_recv()
    }
}

/// What to do when the connection is lost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnLoss {
    /// Reload the page after [`Config::reload_after_ms`]: the web, where
    /// quad-net cannot open a second socket.
    Reload,
    /// Reconnect after [`Config::retry_ms`]: natively.
    Reconnect,
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Ticks between the client's hash checks: as the room's `hash_every`.
    pub hash_every: u64,
    /// Silence, in ms, after which a connection counts as lost.
    pub silence_ms: u64,
    /// Ms between attempts to connect, natively.
    pub retry_ms: u64,
    /// Ms the web shows "rejoining" before it reloads.
    pub reload_after_ms: u64,
    pub on_loss: OnLoss,
    /// A real player: one who keeps the world running. A load test's bot is
    /// not, so bots alone never keep a world awake.
    pub real: bool,
}

impl Default for Config {
    /// The room's defaults, a real player, and the right [`OnLoss`] for the
    /// target.
    fn default() -> Self {
        Self {
            hash_every: 60,
            silence_ms: 4_000,
            retry_ms: 2_000,
            reload_after_ms: 1_500,
            real: true,
            on_loss: if cfg!(target_arch = "wasm32") {
                OnLoss::Reload
            } else {
                OnLoss::Reconnect
            },
        }
    }
}

enum Phase<S: RoomSim> {
    /// Not connected: waiting to try again, since `since` (0 at first).
    Idle {
        since: u64,
    },
    /// Connected or connecting, hello sent, waiting on the welcome.
    Joining,
    Playing(Box<Client<S>>),
    /// The room said no, or the web connection was lost.
    Stopped(String),
}

/// One player's connection to one room.
pub struct Connection<S: RoomSim, T: Transport = QuadSocket> {
    url: String,
    player: String,
    cfg: Config,
    socket: Option<T>,
    /// Frames waiting for the socket to open.
    pending: Vec<Vec<u8>>,
    phase: Phase<S>,
    /// When it last heard anything from the room.
    heard: u64,
    /// When to reload the page to rejoin.
    reload_at: Option<u64>,
}

impl<S: RoomSim, T: Transport> Connection<S, T> {
    /// A connection to `url` for `player`, a name stable for this run of the
    /// game: what gets the seat back after a short drop. It connects on the
    /// first [`update`](Self::update).
    pub fn new(url: impl Into<String>, player: impl Into<String>) -> Self {
        Self::with_config(url, player, Config::default())
    }

    pub fn with_config(url: impl Into<String>, player: impl Into<String>, cfg: Config) -> Self {
        Self {
            url: url.into(),
            player: player.into(),
            cfg,
            socket: None,
            pending: Vec::new(),
            phase: Phase::Idle { since: 0 },
            heard: 0,
            reload_at: None,
        }
    }

    /// One line on how the connection is doing, for the screen.
    pub fn status(&self) -> String {
        match &self.phase {
            Phase::Idle { .. } => format!("connecting to {}...", self.url),
            Phase::Joining => "joining the world...".to_string(),
            Phase::Playing(c) => format!("seat {} - {} ms", c.seat(), c.rtt_ms()),
            Phase::Stopped(why) => why.clone(),
        }
    }

    /// The client, once seated: the worlds to draw, the seat, the timing.
    pub fn client(&self) -> Option<&Client<S>> {
        match &self.phase {
            Phase::Playing(c) => Some(c),
            _ => None,
        }
    }

    pub fn client_mut(&mut self) -> Option<&mut Client<S>> {
        match &mut self.phase {
            Phase::Playing(c) => Some(c),
            _ => None,
        }
    }

    /// This player's seat, once seated.
    pub fn seat(&self) -> Option<Seat> {
        self.client().map(Client::seat)
    }

    /// Whether the room turned this player away, or the web connection was
    /// lost and the page is about to reload.
    pub fn stopped(&self) -> bool {
        matches!(self.phase, Phase::Stopped(_))
    }

    /// Once a frame: this player's controls, encoded for the room, at `now`
    /// ms on the wall clock.
    pub fn update(&mut self, input: &[u8], now: u64) {
        if self.reload_at.is_some_and(|at| now >= at) {
            self.reload_at = None;
            miniquad_platform::reload_page();
        }
        self.connect_if_due(now);
        self.receive(now);
        if let Phase::Playing(c) = &mut self.phase {
            c.input(input, now);
            c.poll(now);
            self.pending.extend(c.drain());
        }
        self.send(now);
        self.notice_silence(now);
    }

    fn connect_if_due(&mut self, now: u64) {
        let Phase::Idle { since } = self.phase else {
            return;
        };
        if since != 0 && now < since + self.cfg.retry_ms {
            return;
        }
        match T::connect(&self.url) {
            Ok(socket) => {
                self.socket = Some(socket);
                self.pending = vec![hello::<S>(&self.player, self.cfg.real)];
                self.phase = Phase::Joining;
                self.heard = now;
            }
            Err(_) => self.phase = Phase::Idle { since: now.max(1) },
        }
    }

    fn receive(&mut self, now: u64) {
        let Some(socket) = &mut self.socket else {
            return;
        };
        while let Some(frame) = socket.try_recv() {
            self.heard = now;
            self.phase = match std::mem::replace(&mut self.phase, Phase::Joining) {
                Phase::Joining => match Client::<S>::welcome(&frame, now, self.cfg.hash_every) {
                    Ok(c) => Phase::Playing(Box::new(c)),
                    Err(why) => Phase::Stopped(format!("refused: {why}")),
                },
                Phase::Playing(mut c) => {
                    if let Err(why) = c.receive(&frame, now) {
                        eprintln!("{}: a bad frame from the room: {why}", S::GAME);
                    }
                    Phase::Playing(c)
                }
                other => other,
            };
        }
    }

    fn send(&mut self, now: u64) {
        let Some(socket) = &self.socket else { return };
        if !socket.connected() || self.pending.is_empty() {
            return;
        }
        let frames = std::mem::take(&mut self.pending);
        if let Err(why) = frames.iter().try_for_each(|f| socket.send(f)) {
            self.lose(&why, now);
        }
    }

    fn notice_silence(&mut self, now: u64) {
        let seated = matches!(self.phase, Phase::Joining | Phase::Playing(_));
        if seated && now > self.heard + self.cfg.silence_ms {
            self.lose("the world stopped answering", now);
        }
    }

    fn lose(&mut self, why: &str, now: u64) {
        self.socket = None;
        self.pending.clear();
        self.phase = match self.cfg.on_loss {
            OnLoss::Reload => {
                self.reload_at = Some(now + self.cfg.reload_after_ms);
                Phase::Stopped(format!("{why} - rejoining..."))
            }
            OnLoss::Reconnect => {
                eprintln!("{}: {why}; reconnecting", S::GAME);
                Phase::Idle {
                    since: self.heard.max(1),
                }
            }
        };
    }
}

#[cfg(test)]
mod tests;
