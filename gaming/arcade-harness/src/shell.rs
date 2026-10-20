//! `Arcade` — the shell as a gamekit [`Game`]: connect, seed, play on the
//! kit's fixed tick, record, submit, show the result, on any gamekit stage.
//!
//! The successor to [`run_game`](crate::run_game), which keeps its own frame
//! loop, reads the keyboard directly and draws in window pixels. Here the
//! loop is the kit's, so a game gets what every gamekit client gets:
//! [`Stage::Board`](macroquad_gamekit::Stage::Board) and the other stages,
//! captures with `--shot`, scripted keys with `--keys`, `--frame-log`, the
//! glyph gate and the preloader.
//!
//! A game supplies an [`ArcadeView`]: its verbs as gamekit
//! [`Actions`], how a tick's input is read from them, and how to draw.
//!
//! ```ignore
//! #[macroquad::main(window_conf)]
//! async fn main() {
//!     run_arcade::<XenoInvaders, _, _>(
//!         Stage::board(vec2(WIDTH, HEIGHT)),
//!         AutoBackend::detect(CLIENT_ID, api_base()),
//!         InvadersView::new(),
//!         HarnessConfig::new("XENO INVADERS"),
//!     )
//!     .await;
//! }
//! ```
//!
//! # One tick, one input
//!
//! The kit calls [`Game::tick`] at the harness's tick rate. Each tick asks
//! the view for exactly one input, records it, and steps the game with it.
//! The recording is therefore the ticks the game actually ran, whatever the
//! frame rate, which is what the server replays.
//!
//! A captured run is a real run: `--keys "0:Space"` starts offline at the
//! first frame, and `--shot out.png --at 4` captures four seconds in.

use arcade_core::api::{
    SEED_PATH, SUBMIT_PATH, ScoreSubmission, SeedGrant, SeedRequest, SubmitOutcome,
};
use arcade_core::{ArcadeGame, GameRecording, InputRecorder};
use macroquad::prelude::*;
use macroquad_gamekit::{
    Actions, Backdrop, Binding, Frame, Game, Gradient, Stage, TextStyle, Tick, app, draw_run,
    fill_gradient, measure_run,
};

use crate::backend::{ArcadeBackend, BackendState};
use crate::session::{self, Pending, Resolved};
use crate::{HarnessConfig, SubmissionState, TitleStatus};

/// What a game supplies to run in the [`Arcade`] shell.
pub trait ArcadeView<G: ArcadeGame> {
    /// The game's verbs: move, fire, hop.
    type Action: Copy + PartialEq;

    /// The verbs, bound to keys, swipes, taps and pad buttons. Asked once.
    fn actions(&self) -> Actions<Self::Action>;

    /// The input for the next tick, read from the actions. Called exactly once
    /// per tick while playing.
    ///
    /// For a held-input game (`ArcadeGame::held_input`, the default) read
    /// [`Actions::held`]. For an instant-input game, spend presses with
    /// [`Actions::take`], so a press between ticks is not lost.
    fn input(&mut self, actions: &mut Actions<Self::Action>) -> Option<G::Input>;

    /// Draw the game, HUD included, in stage units.
    fn draw(&mut self, game: &G, frame: &Frame);

    /// The preloader's colours, and the default screens' background.
    fn backdrop(&self) -> Backdrop;

    /// The display font the default title and game-over screens set titles
    /// and scores in; `None` is macroquad's built-in font, which covers ASCII
    /// only. Load it with the kit's `load_font`, so the glyph gate warms it.
    fn font(&self) -> Option<Font> {
        None
    }

    /// The font for the default screens' prompts and status lines. The
    /// display font unless a game gives another.
    fn body_font(&self) -> Option<Font> {
        self.font()
    }

    /// The title screen. The default centres the title, a prompt and the
    /// connection status.
    fn draw_title(&mut self, title: &str, status: &TitleStatus, frame: &Frame) {
        let size = frame.viewport.size;
        let (display, body) = (self.font(), self.body_font());
        let pulse = (frame.clock * 2.0).sin() * 0.5 + 0.5;
        let status_color = match status.backend {
            BackendState::Ready => Color::new(0.5, 0.8, 0.5, 1.0),
            _ => Color::new(0.6, 0.6, 0.6, 1.0),
        };
        let lines = [
            (title.to_string(), &display, 0.40, size.x / 10.0, GREEN),
            (
                "PRESS SPACE OR TAP TO START".to_string(),
                &body,
                0.56,
                size.x / 30.0,
                Color::new(0.5, 0.8, 1.0, pulse),
            ),
            (status.line(), &body, 0.66, size.x / 44.0, status_color),
        ];
        for (text, font, y, px, color) in lines {
            centred(
                &text,
                &TextStyle::new(font.clone(), px, color),
                size.y * y,
                frame,
            );
        }
    }

    /// The game-over screen. The default centres the score (the server's,
    /// once it has replayed the run), the submission's outcome and a prompt.
    fn draw_game_over(
        &mut self,
        game: &G,
        recording: &GameRecording<G::Input>,
        submission: &SubmissionState,
        frame: &Frame,
    ) {
        let size = frame.viewport.size;
        let (display, body) = (self.font(), self.body_font());
        let shown = match submission {
            SubmissionState::Verified { score, .. } => *score,
            _ => game.score(),
        };
        let pulse = (frame.clock * 2.0).sin() * 0.5 + 0.5;
        let outcome_color = match submission {
            SubmissionState::Verified { .. } => GREEN,
            SubmissionState::Rejected(_) => RED,
            _ => Color::new(0.6, 0.6, 0.6, 1.0),
        };
        let lines = [
            ("GAME OVER".to_string(), &display, 0.30, size.x / 10.0, RED),
            (
                format!("FINAL SCORE: {shown}"),
                &display,
                0.43,
                size.x / 16.0,
                WHITE,
            ),
            (
                format!(
                    "{} transitions over {} ticks",
                    recording.transitions.len(),
                    recording.total_ticks
                ),
                &body,
                0.51,
                size.x / 50.0,
                Color::new(0.45, 0.45, 0.45, 1.0),
            ),
            (submission.line(), &body, 0.59, size.x / 36.0, outcome_color),
            (
                "PRESS SPACE OR TAP TO PLAY AGAIN".to_string(),
                &body,
                0.72,
                size.x / 30.0,
                Color::new(0.5, 0.8, 1.0, pulse),
            ),
        ];
        for (text, font, y, px, color) in lines {
            centred(
                &text,
                &TextStyle::new(font.clone(), px, color),
                size.y * y,
                frame,
            );
        }
    }
}

/// Draw `text` centred across the stage, its baseline at `y`, shrunk to fit
/// within the stage's width (less a margin) if it is wider: a status line
/// carries a server's or a backend's reason, whose length no screen chose.
pub fn centred(text: &str, style: &TextStyle, y: f32, frame: &Frame) {
    if !frame.text {
        return;
    }
    let room = frame.viewport.size.x * 0.92;
    let mut style = style.clone();
    let mut width = measure_run(text, &style, frame.px).width;
    if width > room {
        style.size *= room / width;
        width = measure_run(text, &style, frame.px).width;
    }
    let x = (frame.viewport.size.x - width) / 2.0;
    draw_run(text, &style, vec2(x, y), frame.px, 1.0);
}

/// The shell's own verb: go, on the title and game-over screens.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Confirm;

/// Space, Enter, or a tap. The tap is not a nicety: a Discord Activity runs on
/// mobile, where a screen that only says PRESS SPACE is a dead end.
fn confirm() -> Actions<Confirm> {
    Actions::new(&[
        (Confirm, Binding::Key(KeyCode::Space)),
        (Confirm, Binding::Key(KeyCode::Enter)),
        (Confirm, Binding::Tap),
    ])
}

enum Phase<G: ArcadeGame> {
    Title {
        error: Option<String>,
    },
    AwaitingSeed(Pending<SeedGrant>),
    Playing {
        game: G,
        seed: u64,
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

impl<G: ArcadeGame> Phase<G> {
    fn start(seed: u64, challenge: Option<String>) -> Self {
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

    fn name(&self) -> &'static str {
        match self {
            Phase::Title { .. } => "title",
            Phase::AwaitingSeed(_) => "awaiting seed",
            Phase::Playing { .. } => "playing",
            Phase::GameOver { .. } => "game over",
        }
    }
}

/// The arcade shell as a gamekit [`Game`]. Usually run with [`run_arcade`].
pub struct Arcade<G: ArcadeGame, B, V: ArcadeView<G>> {
    backend: B,
    view: V,
    config: HarnessConfig,
    state: BackendState,
    phase: Phase<G>,
    actions: Actions<V::Action>,
    confirm: Actions<Confirm>,
}

impl<G, B, V> Arcade<G, B, V>
where
    G: ArcadeGame,
    B: ArcadeBackend,
    V: ArcadeView<G>,
{
    pub fn new(backend: B, view: V, config: HarnessConfig) -> Self {
        let actions = view.actions();
        Self {
            backend,
            view,
            config,
            state: BackendState::Connecting("starting"),
            phase: Phase::Title { error: None },
            actions,
            confirm: confirm(),
        }
    }

    /// Begin a run: nothing pressed on the title screen carries into it.
    fn begin(&mut self, seed: u64, challenge: Option<String>) -> Phase<G> {
        self.actions.clear();
        Phase::start(seed, challenge)
    }

    /// Hand a finished recording to the server, or say why it cannot go.
    fn submit(
        &mut self,
        challenge: Option<String>,
        recording: &GameRecording<G::Input>,
    ) -> (SubmissionState, Option<Pending<SubmitOutcome>>) {
        match (&self.state, challenge) {
            (BackendState::Ready, Some(challenge)) => (
                SubmissionState::Submitting,
                Some(session::post(
                    &mut self.backend,
                    SUBMIT_PATH,
                    &ScoreSubmission {
                        game_id: G::game_id().to_string(),
                        challenge,
                        recording: recording.clone(),
                    },
                )),
            ),
            _ => (
                SubmissionState::Offline(match &self.state {
                    BackendState::Offline(why) => why.clone(),
                    _ => "this run was started offline".to_string(),
                }),
                None,
            ),
        }
    }
}

impl<G, B, V> Game for Arcade<G, B, V>
where
    G: ArcadeGame,
    B: ArcadeBackend,
    V: ArcadeView<G>,
{
    fn update(&mut self, frame: &Frame) {
        // First: the backend's connect and auth work only advances when it is
        // polled, so skipping a frame stalls the chain.
        self.state = self.backend.update();
        self.confirm.update(frame.input, frame.dt);
        self.actions.update(frame.input, frame.dt);
        let go = self.confirm.take(Confirm);

        let phase = std::mem::replace(&mut self.phase, Phase::Title { error: None });
        self.phase = match phase {
            // Starting is refused while connecting: a run begun a frame before
            // the token arrives has no challenge and cannot be submitted, and
            // the player would only learn that after playing it.
            Phase::Title { error } if go => match &self.state {
                BackendState::Connecting(_) => Phase::Title { error },
                BackendState::Offline(_) => self.begin(self.config.offline_seed, None),
                BackendState::Ready => Phase::AwaitingSeed(session::post(
                    &mut self.backend,
                    SEED_PATH,
                    &SeedRequest {
                        game_id: G::game_id().to_string(),
                    },
                )),
            },
            Phase::AwaitingSeed(request) => match session::poll(&mut self.backend, &request) {
                Resolved::Pending => Phase::AwaitingSeed(request),
                Resolved::Ok(grant) => self.begin(grant.seed, Some(grant.challenge)),
                // Back to the title with the reason, rather than into an
                // unsubmittable local run the player did not ask for.
                Resolved::Err(e) => Phase::Title { error: Some(e) },
            },
            Phase::GameOver {
                game,
                recording,
                mut submission,
                request,
            } => {
                let request =
                    request.and_then(|request| match session::poll(&mut self.backend, &request) {
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
                    });
                // Not while a submission is in flight: leaving would discard
                // the result the player is waiting to see.
                let settled = !matches!(submission, SubmissionState::Submitting);
                if settled && go {
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
            other => other,
        };
    }

    fn tick_rate(&self) -> Option<f32> {
        Some(self.config.tick_rate as f32)
    }

    fn tick(&mut self, _tick: &Tick, frame: &Frame) {
        let Phase::Playing {
            game,
            recorder,
            seed,
            ..
        } = &mut self.phase
        else {
            return;
        };
        let input = self.view.input(&mut self.actions);
        recorder.record(game.current_tick(), input.as_ref(), f64::from(frame.clock));
        game.tick(input.as_ref());
        if !game.is_over() {
            return;
        }
        let recording = GameRecording {
            game_id: G::game_id().to_string(),
            seed: *seed,
            transitions: std::mem::take(recorder).finish(),
            total_ticks: game.current_tick(),
            claimed_score: game.score(),
        };
        let Phase::Playing {
            game, challenge, ..
        } = std::mem::replace(&mut self.phase, Phase::Title { error: None })
        else {
            unreachable!("matched as playing above");
        };
        let (submission, request) = self.submit(challenge, &recording);
        self.phase = Phase::GameOver {
            game,
            recording,
            submission,
            request,
        };
    }

    fn draw(&mut self, frame: &Frame) {
        let b = self.view.backdrop();
        let size = frame.viewport.size;
        match &self.phase {
            Phase::Title { error } => {
                fill(b, size);
                let status = TitleStatus {
                    backend: &self.state,
                    player_name: self.backend.player_name(),
                    error: error.as_deref(),
                };
                self.view.draw_title(&self.config.title, &status, frame);
            }
            Phase::AwaitingSeed(_) => {
                fill(b, size);
                let status = TitleStatus {
                    backend: &BackendState::Connecting("asking for a board"),
                    player_name: self.backend.player_name(),
                    error: None,
                };
                self.view.draw_title(&self.config.title, &status, frame);
            }
            Phase::Playing { game, .. } => self.view.draw(game, frame),
            Phase::GameOver {
                game,
                recording,
                submission,
                ..
            } => {
                fill(b, size);
                self.view.draw_game_over(game, recording, submission, frame);
            }
        }
    }

    fn backdrop(&self) -> Backdrop {
        self.view.backdrop()
    }

    fn capture_note(&self, frame: &Frame) -> String {
        let score = match &self.phase {
            Phase::Playing { game, .. } | Phase::GameOver { game, .. } => {
                format!(", tick {}, score {}", game.current_tick(), game.score())
            }
            _ => String::new(),
        };
        format!(
            "{} t={:.2}s, {}{score}",
            G::game_id(),
            frame.clock,
            self.phase.name()
        )
    }
}

/// The default screens' background: the backdrop's gradient over the stage.
fn fill(b: Backdrop, size: Vec2) {
    fill_gradient(
        Rect::new(0.0, 0.0, size.x, size.y),
        0.0,
        Gradient::Vertical {
            top: b.top,
            bottom: b.bottom,
        },
    );
}

/// Run `G` in the arcade shell on `stage`, until the window closes.
pub async fn run_arcade<G, B, V>(stage: Stage, backend: B, view: V, config: HarnessConfig)
where
    G: ArcadeGame,
    B: ArcadeBackend,
    V: ArcadeView<G>,
{
    app::run_on(stage, Arcade::new(backend, view, config)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::OfflineBackend;
    use arcade_core::verify_recording;
    use macroquad_gamekit::harness::{Args, FIXED_DT};
    use macroquad_gamekit::{InputSnapshot, Viewport};
    use serde::{Deserialize, Serialize};

    /// Scores a point every tick the button is held; over after 120 ticks.
    #[derive(Clone)]
    struct Holder {
        tick: u64,
        score: u64,
    }

    #[derive(Clone, PartialEq, Serialize, Deserialize)]
    struct Hold;

    impl ArcadeGame for Holder {
        type Input = Hold;
        fn new(_seed: u64) -> Self {
            Self { tick: 0, score: 0 }
        }
        fn tick(&mut self, input: Option<&Hold>) {
            self.score += u64::from(input.is_some());
            self.tick += 1;
        }
        fn is_over(&self) -> bool {
            self.tick >= 120
        }
        fn score(&self) -> u64 {
            self.score
        }
        fn current_tick(&self) -> u64 {
            self.tick
        }
        fn game_id() -> &'static str {
            "holder"
        }
    }

    #[derive(Clone, Copy, PartialEq)]
    struct Press;

    struct View;

    impl ArcadeView<Holder> for View {
        type Action = Press;
        fn actions(&self) -> Actions<Press> {
            Actions::new(&[(Press, Binding::Key(KeyCode::Z))])
        }
        fn input(&mut self, actions: &mut Actions<Press>) -> Option<Hold> {
            actions.held(Press).then_some(Hold)
        }
        fn draw(&mut self, _game: &Holder, _frame: &Frame) {}
        fn backdrop(&self) -> Backdrop {
            Backdrop {
                top: BLACK,
                bottom: BLACK,
                accent: WHITE,
            }
        }
    }

    /// Drive the shell as the loop would, one frame of `keys` at a time, a
    /// tick a frame. Returns the recording once the run is over.
    fn play(script: &str, frames: u32) -> Option<GameRecording<Hold>> {
        let args = Args::default();
        let script = macroquad_gamekit::KeyScript::parse_all(script).expect("script");
        let mut shell = Arcade::<Holder, _, _>::new(
            OfflineBackend::default(),
            View,
            HarnessConfig::new("HOLDER"),
        );
        let size = vec2(100.0, 100.0);
        for f in 0..frames {
            let mut input = InputSnapshot::default();
            input.script(&script, f);
            let frame = Frame {
                args: &args,
                dt: FIXED_DT,
                clock: f as f32 * FIXED_DT,
                frames: f,
                viewport: Viewport {
                    u: 1.0,
                    size,
                    dpi: 1.0,
                },
                px: 1.0,
                origin: Vec2::ZERO,
                bounds: Rect::new(0.0, 0.0, size.x, size.y),
                input: &input,
                alpha: 1.0,
                text: false,
                canvas: None,
            };
            shell.update(&frame);
            shell.tick(
                &Tick {
                    n: u64::from(f),
                    dt: FIXED_DT,
                },
                &frame,
            );
            if let Phase::GameOver { recording, .. } = &shell.phase {
                return Some(recording.clone());
            }
        }
        None
    }

    #[test]
    fn a_run_through_the_shell_replays_to_the_same_score() {
        // Start on frame 0, then hold Z for frames 10..50 and 70..80.
        let recording = play("0:Space,10-50:Z,70-80:Z", 400).expect("the run ends");
        assert_eq!(recording.total_ticks, 120);
        assert_eq!(recording.claimed_score, 50);
        assert_eq!(recording.transitions.len(), 4, "on, off, on, off");
        let verdict = verify_recording::<Holder>(&recording);
        assert!(verdict.valid, "{verdict:?}");
    }

    #[test]
    fn the_press_that_starts_a_run_does_not_reach_the_game() {
        // Space starts the run; were it bound in the game, a cleared buffer
        // keeps it from firing on the first tick. Z pressed on the same frame
        // as Space is likewise cleared, but holding it still counts.
        let recording = play("0:Space,0-5:Z", 400).expect("the run ends");
        let verdict = verify_recording::<Holder>(&recording);
        assert!(verdict.valid, "{verdict:?}");
    }

    #[test]
    fn nothing_starts_without_a_confirm() {
        assert!(play("10-50:Z", 300).is_none());
    }
}
