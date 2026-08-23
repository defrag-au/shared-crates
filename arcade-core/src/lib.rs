//! Replay verification for deterministic games.
//!
//! Any game implementing [`ArcadeGame`] is automatically replay-verifiable:
//! the same seed + same inputs always produces the same score. A client does
//! not claim a number — it submits the seed and the input log, and the server
//! runs the identical logic headlessly and computes the score itself.
//!
//! Pure serde by design: no rendering, no I/O, no wasm-bindgen. The same crate
//! compiles into a macroquad client, a Cloudflare Worker doing the replay, and
//! a native test. The macroquad play-loop glue lives in `arcade-harness`.
//!
//! Two game models:
//!   - **Session games** (space invaders): short play, input log, final score
//!   - **Sim games** (colony): long-running, decision log, periodic score snapshots

use serde::{Deserialize, Serialize};

pub mod api;

/// The core trait every replay-verified game must implement.
///
/// The game must be fully deterministic: `new(seed)` + sequence of
/// `tick(input)` calls must always produce the same `score()`.
pub trait ArcadeGame: Sized {
    /// Input type — what the player can do each tick.
    /// Must be serializable and comparable (for delta encoding).
    type Input: Clone + PartialEq + Serialize + for<'de> Deserialize<'de>;

    /// Create a new game from a seed. All RNG derives from this.
    fn new(seed: u64) -> Self;

    /// Advance the game by one tick with optional player input.
    fn tick(&mut self, input: Option<&Self::Input>);

    /// Whether the game session has ended.
    /// For continuous games (colony sim), return false — the server
    /// decides when to stop at epoch boundaries.
    fn is_over(&self) -> bool;

    /// Current score. Must be deterministic from seed + inputs.
    fn score(&self) -> u64;

    /// Current tick number.
    fn current_tick(&self) -> u64;

    /// Game metadata for the leaderboard.
    fn game_id() -> &'static str;

    /// Whether inputs are "held" (like a key) or "instant" (like a decision).
    ///
    /// - `true` (default): input persists until changed. Good for action games
    ///   where the player holds directions. Delta recorder stores transitions.
    /// - `false`: input is consumed on the tick it's applied. Good for sim games
    ///   where decisions are one-shot events.
    fn held_input() -> bool {
        true
    }
}

/// A recorded game session — compact delta-encoded format.
///
/// Records input *transitions*: when the player starts doing something
/// new, and when they stop. A typical session might have ~500 transitions
/// vs 16,000+ individual ticks.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameRecording<I> {
    pub game_id: String,
    pub seed: u64,
    /// Delta-encoded input transitions.
    /// Each entry means "from this tick, the input is this (or None)".
    pub transitions: Vec<InputTransition<I>>,
    pub total_ticks: u64,
    pub claimed_score: u64,
}

/// An input transition — the player's input changed at this tick.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputTransition<I> {
    pub tick: u64,
    /// None = player released all keys. Some = new input state.
    pub input: Option<I>,
    /// Wall-clock timestamp in seconds since session start.
    /// Not used for replay — used for play-pattern analysis.
    #[serde(default)]
    pub timestamp: f64,
}

/// Result of server-side replay verification.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerifyResult {
    pub valid: bool,
    pub computed_score: u64,
    pub claimed_score: u64,
    pub total_ticks: u64,
    pub reason: Option<String>,
}

/// Replay a game recording headlessly and verify the score.
pub fn verify_recording<G: ArcadeGame>(recording: &GameRecording<G::Input>) -> VerifyResult {
    let mut game = G::new(recording.seed);
    let mut transition_idx = 0;
    let mut current_input: Option<G::Input> = None;
    let held = G::held_input();

    for tick in 0..recording.total_ticks {
        if game.is_over() {
            break;
        }

        // Check for input transition at this tick
        let mut had_input_this_tick = false;
        while transition_idx < recording.transitions.len()
            && recording.transitions[transition_idx].tick == tick
        {
            current_input = recording.transitions[transition_idx].input.clone();
            transition_idx += 1;
            had_input_this_tick = true;
        }

        game.tick(current_input.as_ref());

        // For instant-input games, revert to None after each tick
        if !held && had_input_this_tick {
            current_input = None;
        }
    }

    let computed = game.score();
    let valid = computed == recording.claimed_score;

    VerifyResult {
        valid,
        computed_score: computed,
        claimed_score: recording.claimed_score,
        total_ticks: recording.total_ticks,
        reason: if valid {
            None
        } else {
            Some(format!(
                "score mismatch: computed {computed}, claimed {}",
                recording.claimed_score
            ))
        },
    }
}

/// Helper for recording inputs during play.
///
/// Two modes based on `held`:
/// - `held=true` (action games): tracks state, only records transitions
/// - `held=false` (sim/turn games): records every input as a discrete event
pub struct InputRecorder<I: Clone + PartialEq> {
    transitions: Vec<InputTransition<I>>,
    current: Option<I>,
    held: bool,
}

impl<I: Clone + PartialEq> InputRecorder<I> {
    /// Create a recorder for held-input games (action games).
    pub fn new() -> Self {
        Self {
            transitions: Vec::new(),
            current: None,
            held: true,
        }
    }

    /// Create a recorder for instant-input games (sim/turn games).
    pub fn instant() -> Self {
        Self {
            transitions: Vec::new(),
            current: None,
            held: false,
        }
    }

    /// Record the input state for this tick with a wall-clock timestamp.
    /// In held mode: only stores if input changed from last tick.
    /// In instant mode: stores every non-None input as a discrete event.
    pub fn record(&mut self, tick: u64, input: Option<&I>, timestamp: f64) {
        if self.held {
            let changed = match (&self.current, input) {
                (None, None) => false,
                (Some(a), Some(b)) => a != b,
                _ => true,
            };
            if changed {
                self.current = input.cloned();
                self.transitions.push(InputTransition {
                    tick,
                    input: input.cloned(),
                    timestamp,
                });
            }
        } else {
            if let Some(i) = input {
                self.transitions.push(InputTransition {
                    tick,
                    input: Some(i.clone()),
                    timestamp,
                });
            }
        }
    }

    /// Consume the recorder and return the transitions.
    pub fn finish(self) -> Vec<InputTransition<I>> {
        self.transitions
    }

    /// Number of transitions recorded so far.
    pub fn len(&self) -> usize {
        self.transitions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.transitions.is_empty()
    }
}

impl<I: Clone + PartialEq> Default for InputRecorder<I> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trivial test game: score = number of ticks with input.
    #[derive(Clone)]
    struct CountGame {
        tick: u64,
        count: u64,
        over_at: u64,
    }

    #[derive(Clone, PartialEq, Serialize, Deserialize)]
    struct CountInput;

    impl ArcadeGame for CountGame {
        type Input = CountInput;

        fn new(seed: u64) -> Self {
            Self { tick: 0, count: 0, over_at: seed % 100 + 10 }
        }

        fn tick(&mut self, input: Option<&CountInput>) {
            if input.is_some() {
                self.count += 1;
            }
            self.tick += 1;
        }

        fn is_over(&self) -> bool { self.tick >= self.over_at }
        fn score(&self) -> u64 { self.count }
        fn current_tick(&self) -> u64 { self.tick }
        fn game_id() -> &'static str { "count-game" }
    }

    #[test]
    fn verify_valid_recording() {
        let recording = GameRecording {
            game_id: "count-game".into(),
            seed: 42,
            transitions: vec![
                InputTransition { tick: 5, input: Some(CountInput), timestamp: 0.08 },
                InputTransition { tick: 15, input: None, timestamp: 0.25 },
            ],
            total_ticks: 52,
            claimed_score: 10,
        };

        let result = verify_recording::<CountGame>(&recording);
        assert!(result.valid, "{result:?}");
    }

    #[test]
    fn reject_inflated_score() {
        let recording = GameRecording {
            game_id: "count-game".into(),
            seed: 42,
            transitions: vec![
                InputTransition { tick: 0, input: Some(CountInput), timestamp: 0.0 },
                InputTransition { tick: 5, input: None, timestamp: 0.08 },
            ],
            total_ticks: 52,
            claimed_score: 999,
        };

        let result = verify_recording::<CountGame>(&recording);
        assert!(!result.valid);
        assert_eq!(result.computed_score, 5);
    }

    #[test]
    fn recorder_only_stores_transitions() {
        let mut rec = InputRecorder::new();

        for tick in 0..100 {
            rec.record(tick, Some(&CountInput), tick as f64 / 60.0);
        }
        for tick in 100..150 {
            rec.record(tick, None, tick as f64 / 60.0);
        }
        for tick in 150..200 {
            rec.record(tick, Some(&CountInput), tick as f64 / 60.0);
        }

        let transitions = rec.finish();
        // Should only have 3 transitions: start, stop, start
        assert_eq!(transitions.len(), 3);
        assert_eq!(transitions[0].tick, 0);
        assert!(transitions[0].input.is_some());
        assert_eq!(transitions[1].tick, 100);
        assert!(transitions[1].input.is_none());
        assert_eq!(transitions[2].tick, 150);
        assert!(transitions[2].input.is_some());
    }

    #[test]
    fn deterministic_across_runs() {
        let transitions = vec![
            InputTransition { tick: 5, input: Some(CountInput), timestamp: 0.08 },
            InputTransition { tick: 15, input: None, timestamp: 0.25 },
        ];

        let r1 = verify_recording::<CountGame>(&GameRecording {
            game_id: "count-game".into(),
            seed: 123,
            transitions: transitions.clone(),
            total_ticks: 50,
            claimed_score: 10,
        });
        let r2 = verify_recording::<CountGame>(&GameRecording {
            game_id: "count-game".into(),
            seed: 123,
            transitions,
            total_ticks: 50,
            claimed_score: 10,
        });

        assert_eq!(r1.computed_score, r2.computed_score);
        assert!(r1.valid);
        assert!(r2.valid);
    }
}
