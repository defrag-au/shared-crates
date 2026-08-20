//! Macroquad shell for [`arcade_core`] games.
//!
//! Owns everything between "the page loaded" and "the score is on the board":
//! connecting to whatever surface the game was launched on, signing in,
//! asking the server for a seed, the fixed-timestep play loop, recording
//! inputs, submitting the recording, and showing what came back.
//!
//! A game supplies three things and no networking at all:
//!
//! 1. A type implementing [`arcade_core::ArcadeGame`]
//! 2. An [`InputMapper`] — read the keyboard/pointer, produce an input
//! 3. A [`GameRenderer`] — draw the world and the HUD
//!
//! # Why this is a shell and not a library of helpers
//!
//! Every game binary here is separately compiled and separately loaded — that
//! is how the arcade avoids shipping one enormous wasm — which means every
//! game is also a fresh opportunity to get the seed handshake, the submit
//! payload, or the offline fallback subtly wrong. Those are the parts a player
//! cannot see failing: a game that quietly never submits looks exactly like a
//! game whose player is bad at it.
//!
//! ```ignore
//! use arcade_harness::{backend::DiscordBackend, run_game, HarnessConfig};
//!
//! #[macroquad::main("Xeno Invaders")]
//! async fn main() {
//!     run_game::<XenoInvaders, _, _, _>(
//!         DiscordBackend::new(CLIENT_ID),
//!         MyRenderer,
//!         MyInput,
//!         HarnessConfig::new("XENO INVADERS"),
//!     )
//!     .await;
//! }
//! ```

use arcade_core::api::{ScoreSubmission, SeedGrant, SeedRequest, SubmitOutcome, SEED_PATH,
    SUBMIT_PATH};
use arcade_core::{ArcadeGame, GameRecording, InputRecorder};
use backend::{ArcadeBackend, BackendState};
use macroquad::prelude::*;
use session::{Pending, Resolved};

pub mod backend;
pub mod session;

/// Maps keyboard/pointer state to game input.
pub trait InputMapper<G: ArcadeGame> {
    fn capture(&self) -> Option<G::Input>;
}

/// What the player is waiting on before a run can start.
pub struct TitleStatus<'a> {
    /// Connection progress, or why there is no server.
    pub backend: &'a BackendState,
    /// Who the score will be credited to, once known.
    pub player_name: Option<&'a str>,
    /// Set when the last attempt to start a run failed.
    pub error: Option<&'a str>,
}

impl TitleStatus<'_> {
    /// One line describing the connection, suitable for a title screen.
    pub fn line(&self) -> String {
        if let Some(error) = self.error {
            return format!("! {error}");
        }
        match self.backend {
            BackendState::Connecting(what) => format!("{what}…"),
            BackendState::Offline(why) => format!("offline — {why}"),
            BackendState::Ready => match self.player_name {
                Some(name) => format!("playing as {name}"),
                None => "connected".to_string(),
            },
        }
    }
}

/// What became of a finished run.
pub enum SubmissionState {
    /// No server this session; the score is local and honest, just unrecorded.
    Offline(String),
    Submitting,
    /// The server replayed the recording and agreed.
    Verified {
        /// The score the *server* computed. Shown in preference to the local
        /// one — if they ever differ, the server's is the true one and hiding
        /// that would make a real bug invisible.
        score: u64,
        rank: Option<u32>,
        reward_eligible: bool,
    },
    /// The server replayed and disagreed, or refused the submission.
    Rejected(String),
}

impl SubmissionState {
    /// One line describing the outcome, suitable for a game-over screen.
    pub fn line(&self) -> String {
        match self {
            Self::Offline(why) => format!("not submitted — {why}"),
            Self::Submitting => "submitting…".to_string(),
            Self::Verified {
                rank,
                reward_eligible,
                ..
            } => {
                let rank = match rank {
                    Some(rank) => format!("verified — rank #{rank}"),
                    // A verified score with no rank means the per-epoch
                    // submission cap was hit. Say so, rather than showing a
                    // blank where a rank should be.
                    None => "verified — no ranked slots left this epoch".to_string(),
                };
                if *reward_eligible {
                    format!("{rank} ★ playing for rewards")
                } else {
                    rank
                }
            }
            Self::Rejected(why) => format!("not counted — {why}"),
        }
    }
}

/// Renders the game state.
pub trait GameRenderer<G: ArcadeGame> {
    /// Draw the game world.
    fn draw(&self, game: &G);

    /// Draw the HUD overlay (score, lives, wave).
    fn draw_hud(&self, game: &G);

    /// Draw the title screen. Default provided.
    fn draw_title(&self, game_name: &str, status: &TitleStatus) {
        clear_background(Color::new(0.02, 0.02, 0.06, 1.0));
        let cx = screen_width() / 2.0;
        let cy = screen_height() / 2.0;

        draw_text(game_name, cx - 140.0, cy - 40.0, 40.0, GREEN);

        let pulse = (get_time() * 2.0).sin() as f32 * 0.5 + 0.5;
        draw_text(
            "PRESS SPACE TO START",
            cx - 110.0,
            cy + 30.0,
            22.0,
            Color::new(0.5, 0.8, 1.0, pulse),
        );

        let status_line = status.line();
        draw_text(
            &status_line,
            cx - 140.0,
            cy + 90.0,
            18.0,
            match status.backend {
                BackendState::Ready => Color::new(0.5, 0.8, 0.5, 1.0),
                _ => Color::new(0.6, 0.6, 0.6, 1.0),
            },
        );
    }

    /// Draw the game-over screen. Default provided.
    fn draw_game_over(
        &self,
        game: &G,
        recording: &GameRecording<G::Input>,
        submission: &SubmissionState,
    ) {
        clear_background(Color::new(0.02, 0.02, 0.06, 1.0));
        let cx = screen_width() / 2.0;
        let cy = screen_height() / 2.0;

        draw_text("GAME OVER", cx - 100.0, cy - 80.0, 40.0, RED);

        // The local score, which is what the player just watched happen.
        // Replaced below by the server's if the two disagree.
        let shown = match submission {
            SubmissionState::Verified { score, .. } => *score,
            _ => game.score(),
        };
        draw_text(
            &format!("FINAL SCORE: {shown}"),
            cx - 120.0,
            cy - 30.0,
            30.0,
            WHITE,
        );

        draw_text(
            &format!(
                "{} transitions over {} ticks",
                recording.transitions.len(),
                recording.total_ticks,
            ),
            cx - 120.0,
            cy + 10.0,
            16.0,
            Color::new(0.45, 0.45, 0.45, 1.0),
        );

        draw_text(
            &submission.line(),
            cx - 160.0,
            cy + 50.0,
            18.0,
            match submission {
                SubmissionState::Verified { .. } => GREEN,
                SubmissionState::Rejected(_) => RED,
                _ => Color::new(0.6, 0.6, 0.6, 1.0),
            },
        );

        let pulse = (get_time() * 2.0).sin() as f32 * 0.5 + 0.5;
        draw_text(
            "PRESS SPACE TO PLAY AGAIN",
            cx - 130.0,
            cy + 120.0,
            20.0,
            Color::new(0.5, 0.8, 1.0, pulse),
        );
    }
}

/// Harness configuration.
pub struct HarnessConfig {
    /// Title shown on the attract screen.
    pub title: String,
    /// Game logic rate, in ticks per second.
    pub tick_rate: f64,
    /// Seed used when there is no server to ask.
    ///
    /// Offline play still has to be deterministic and replayable — a local run
    /// is a real recording, it just has nowhere to go.
    pub offline_seed: u64,
}

impl HarnessConfig {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            tick_rate: 60.0,
            offline_seed: 42,
        }
    }

    pub fn tick_rate(mut self, rate: f64) -> Self {
        self.tick_rate = rate;
        self
    }
}

/// The shell's own state. A game never sees this.
enum Phase<G: ArcadeGame> {
    /// Attract screen. `error` holds the last failure, so a player learns why
    /// their previous attempt to start did not.
    Title { error: Option<String> },
    /// Asked the server for a seed; nothing is playable yet.
    AwaitingSeed(Pending<SeedGrant>),
    Playing {
        game: G,
        seed: u64,
        /// `None` offline — there is nothing to quote back.
        challenge: Option<String>,
        recorder: InputRecorder<G::Input>,
    },
    GameOver {
        game: G,
        recording: GameRecording<G::Input>,
        submission: SubmissionState,
        request: Option<Pending<SubmitOutcome>>,
    },
}

/// Run a game: connect, sign in, seed, play, record, submit, repeat.
pub async fn run_game<G, B, R, I>(
    mut backend: B,
    renderer: R,
    input_mapper: I,
    config: HarnessConfig,
) where
    G: ArcadeGame + Clone,
    B: ArcadeBackend,
    R: GameRenderer<G>,
    I: InputMapper<G>,
{
    let tick_dt = 1.0 / config.tick_rate;
    let mut phase: Phase<G> = Phase::Title { error: None };
    let mut accumulator = 0.0;

    loop {
        // Before anything else: the backend's connect/auth work only advances
        // when it is polled, so skipping this on any frame stalls the chain.
        let state = backend.update();
        accumulator += get_frame_time() as f64;

        phase = match phase {
            Phase::Title { error } => {
                renderer.draw_title(
                    &config.title,
                    &TitleStatus {
                        backend: &state,
                        player_name: backend.player_name(),
                        error: error.as_deref(),
                    },
                );

                // Starting is refused while connecting, on purpose: a run
                // begun a frame before the token arrives has no challenge and
                // cannot be submitted, and the player would only find that out
                // after playing it.
                if is_key_pressed(KeyCode::Space) {
                    accumulator = 0.0;
                    match &state {
                        BackendState::Connecting(_) => Phase::Title { error: None },
                        BackendState::Offline(_) => {
                            start_run::<G>(config.offline_seed, None)
                        }
                        BackendState::Ready => Phase::AwaitingSeed(session::post(
                            &mut backend,
                            SEED_PATH,
                            &SeedRequest {
                                game_id: G::game_id().to_string(),
                            },
                        )),
                    }
                } else {
                    Phase::Title { error }
                }
            }

            Phase::AwaitingSeed(request) => {
                renderer.draw_title(
                    &config.title,
                    &TitleStatus {
                        backend: &state,
                        player_name: backend.player_name(),
                        error: None,
                    },
                );

                match session::poll(&mut backend, &request) {
                    Resolved::Pending => Phase::AwaitingSeed(request),
                    Resolved::Ok(grant) => {
                        accumulator = 0.0;
                        start_run::<G>(grant.seed, Some(grant.challenge))
                    }
                    // Back to the title with the reason, rather than dropping
                    // into an unsubmittable local run the player did not ask
                    // for.
                    Resolved::Err(e) => Phase::Title { error: Some(e) },
                }
            }

            Phase::Playing {
                mut game,
                seed,
                challenge,
                mut recorder,
            } => {
                // Captured once per frame and applied to every tick this frame,
                // so the recording matches what the server will replay
                // regardless of frame rate.
                let input = input_mapper.capture();
                let mut finished = None;

                while accumulator >= tick_dt {
                    recorder.record(game.current_tick(), input.as_ref(), get_time());
                    game.tick(input.as_ref());
                    accumulator -= tick_dt;

                    if game.is_over() {
                        finished = Some(GameRecording {
                            game_id: G::game_id().to_string(),
                            seed,
                            transitions: std::mem::take(&mut recorder).finish(),
                            total_ticks: game.current_tick(),
                            claimed_score: game.score(),
                        });
                        break;
                    }
                }

                match finished {
                    None => {
                        renderer.draw(&game);
                        renderer.draw_hud(&game);
                        Phase::Playing {
                            game,
                            seed,
                            challenge,
                            recorder,
                        }
                    }
                    Some(recording) => {
                        let (submission, request) = match (&state, challenge) {
                            (BackendState::Ready, Some(challenge)) => (
                                SubmissionState::Submitting,
                                Some(session::post(
                                    &mut backend,
                                    SUBMIT_PATH,
                                    &ScoreSubmission {
                                        game_id: G::game_id().to_string(),
                                        challenge,
                                        recording: recording.clone(),
                                    },
                                )),
                            ),
                            (_, _) => (
                                SubmissionState::Offline(match &state {
                                    BackendState::Offline(why) => why.clone(),
                                    _ => "this run was started offline".to_string(),
                                }),
                                None,
                            ),
                        };

                        Phase::GameOver {
                            game,
                            recording,
                            submission,
                            request,
                        }
                    }
                }
            }

            Phase::GameOver {
                game,
                recording,
                mut submission,
                request,
            } => {
                let request = match request {
                    None => None,
                    Some(request) => match session::poll(&mut backend, &request) {
                        Resolved::Pending => Some(request),
                        Resolved::Err(e) => {
                            submission = SubmissionState::Rejected(e);
                            None
                        }
                        Resolved::Ok(outcome) => {
                            submission = if outcome.verified {
                                SubmissionState::Verified {
                                    score: outcome.score,
                                    rank: outcome.rank,
                                    reward_eligible: outcome.reward_eligible,
                                }
                            } else {
                                SubmissionState::Rejected(
                                    outcome
                                        .reason
                                        .unwrap_or_else(|| "the server did not agree".to_string()),
                                )
                            };
                            None
                        }
                    },
                };

                renderer.draw_game_over(&game, &recording, &submission);

                // Not while a submission is in flight: leaving the screen would
                // discard the result the player is waiting to see.
                let settled = !matches!(submission, SubmissionState::Submitting);
                if settled && is_key_pressed(KeyCode::Space) {
                    Phase::Title { error: None }
                } else {
                    Phase::GameOver {
                        game,
                        recording,
                        submission,
                        request,
                    }
                }
            }
        };

        next_frame().await;
    }
}

fn start_run<G: ArcadeGame>(seed: u64, challenge: Option<String>) -> Phase<G> {
    Phase::Playing {
        game: G::new(seed),
        seed,
        challenge,
        recorder: if G::held_input() {
            InputRecorder::new()
        } else {
            InputRecorder::instant()
        },
    }
}
