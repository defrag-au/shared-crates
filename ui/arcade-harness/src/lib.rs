//! Macroquad play-loop for [`arcade_core`] games.
//!
//! Handles the main loop, input recording, game over screen, and replay
//! verification display. Game developers only need to provide:
//!
//! 1. A game struct implementing `ArcadeGame`
//! 2. An `InputMapper` that reads keyboard/mouse state → game input
//! 3. A `GameRenderer` that draws the game state
//!
//! # Example
//!
//! ```ignore
//! use arcade_harness::{run_game, InputMapper, GameRenderer};
//!
//! struct MyRenderer;
//! impl GameRenderer<MyGame> for MyRenderer {
//!     fn draw(&self, game: &MyGame) { /* draw stuff */ }
//!     fn draw_hud(&self, game: &MyGame) { /* score, lives, etc */ }
//! }
//!
//! struct MyInput;
//! impl InputMapper<MyGame> for MyInput {
//!     fn capture(&self) -> Option<<MyGame as ArcadeGame>::Input> { /* read keys */ }
//! }
//!
//! #[macroquad::main("My Game")]
//! async fn main() {
//!     run_game::<MyGame>(MyRenderer, MyInput, 42).await;
//! }
//! ```

use arcade_core::{ArcadeGame, GameRecording, InputRecorder};
use macroquad::prelude::*;

/// Maps keyboard/mouse state to game input.
/// Implement this for your game's controls.
pub trait InputMapper<G: ArcadeGame> {
    fn capture(&self) -> Option<G::Input>;
}

/// Renders the game state to the screen.
/// Implement this for your game's visuals.
pub trait GameRenderer<G: ArcadeGame> {
    /// Draw the game world (aliens, player, bullets, etc).
    fn draw(&self, game: &G);

    /// Draw the HUD overlay (score, lives, wave, etc).
    fn draw_hud(&self, game: &G);

    /// Draw the title screen. Default provided.
    fn draw_title(&self, game_name: &str) {
        clear_background(Color::new(0.02, 0.02, 0.06, 1.0));
        let cx = screen_width() / 2.0;
        let cy = screen_height() / 2.0;
        draw_text(game_name, cx - 100.0, cy - 40.0, 40.0, GREEN);
        let pulse = (get_time() * 2.0).sin() as f32 * 0.5 + 0.5;
        draw_text(
            "PRESS SPACE TO START",
            cx - 110.0,
            cy + 30.0,
            22.0,
            Color::new(0.5, 0.8, 1.0, pulse),
        );
    }

    /// Draw the game over screen. Default provided.
    fn draw_game_over(&self, game: &G, recording: &GameRecording<G::Input>) {
        clear_background(Color::new(0.02, 0.02, 0.06, 1.0));
        let cx = screen_width() / 2.0;
        let cy = screen_height() / 2.0;

        draw_text("GAME OVER", cx - 100.0, cy - 60.0, 40.0, RED);
        draw_text(
            &format!("FINAL SCORE: {}", game.score()),
            cx - 120.0,
            cy - 10.0,
            30.0,
            WHITE,
        );

        let json_size = serde_json::to_string(recording)
            .map(|s| s.len())
            .unwrap_or(0);
        draw_text(
            &format!(
                "RECORDING: {} transitions, {} ticks ({:.1} KB)",
                recording.transitions.len(),
                recording.total_ticks,
                json_size as f64 / 1024.0,
            ),
            cx - 160.0,
            cy + 40.0,
            18.0,
            Color::new(0.5, 0.5, 0.5, 1.0),
        );

        let result = arcade_core::verify_recording::<G>(recording);
        let color = if result.valid { GREEN } else { RED };
        draw_text(
            &format!(
                "VERIFY: {} (computed: {})",
                if result.valid { "PASS" } else { "FAIL" },
                result.computed_score,
            ),
            cx - 160.0,
            cy + 70.0,
            18.0,
            color,
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

/// Configuration for the game harness.
pub struct HarnessConfig {
    /// Game title shown on title screen.
    pub title: String,
    /// Starting seed (increments each game).
    pub initial_seed: u64,
    /// Ticks per second (game logic rate).
    pub tick_rate: f64,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            title: "ADACADE GAME".into(),
            initial_seed: 42,
            tick_rate: 60.0,
        }
    }
}

/// Run a game with the full harness: title → play → game over → replay.
///
/// This is the main entry point. Call from your `#[macroquad::main]` function.
pub async fn run_game<G: ArcadeGame + Clone>(
    renderer: impl GameRenderer<G>,
    input_mapper: impl InputMapper<G>,
    config: HarnessConfig,
) {
    let mut seed = config.initial_seed;
    let tick_dt = 1.0 / config.tick_rate;

    enum State<G: ArcadeGame> {
        Title,
        Playing {
            game: G,
            recorder: InputRecorder<G::Input>,
        },
        GameOver {
            game: G,
            recording: GameRecording<G::Input>,
        },
    }

    let mut state: State<G> = State::Title;
    let mut accumulator = 0.0;

    loop {
        let dt = get_frame_time() as f64;
        accumulator += dt;

        match &mut state {
            State::Title => {
                renderer.draw_title(&config.title);
                if is_key_pressed(KeyCode::Space) {
                    let recorder = if G::held_input() {
                        InputRecorder::new()
                    } else {
                        InputRecorder::instant()
                    };
                    state = State::Playing {
                        game: G::new(seed),
                        recorder,
                    };
                    accumulator = 0.0;
                }
            }

            State::Playing { game, recorder } => {
                let input = input_mapper.capture();

                while accumulator >= tick_dt {
                    let tick = game.current_tick();
                    recorder.record(tick, input.as_ref(), get_time());
                    game.tick(input.as_ref());
                    accumulator -= tick_dt;

                    if game.is_over() {
                        let recording = GameRecording {
                            game_id: G::game_id().to_string(),
                            seed,
                            transitions: std::mem::take(recorder).finish(),
                            total_ticks: game.current_tick(),
                            claimed_score: game.score(),
                        };
                        state = State::GameOver {
                            game: game.clone(),
                            recording,
                        };
                        break;
                    }
                }

                if let State::Playing { game, .. } = &state {
                    renderer.draw(game);
                    renderer.draw_hud(game);
                }
            }

            State::GameOver { game, recording } => {
                renderer.draw_game_over(game, recording);
                if is_key_pressed(KeyCode::Space) {
                    seed += 1;
                    let recorder = if G::held_input() {
                        InputRecorder::new()
                    } else {
                        InputRecorder::instant()
                    };
                    state = State::Playing {
                        game: G::new(seed),
                        recorder,
                    };
                    accumulator = 0.0;
                }
            }
        }

        next_frame().await;
    }
}
