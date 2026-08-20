//! The score-API wire contract: seed issuance, submission, leaderboards.
//!
//! Defined here rather than in the worker because both ends of every one of
//! these types is Rust — the client builds it, the worker parses it — and a
//! type defined twice is a type that drifts. Serde's failure mode makes that
//! drift silent: a renamed or dropped field deserialises to `None`/absent
//! rather than erroring, so a hand-mirrored copy doesn't break the build, it
//! quietly stops carrying a value. This codebase has already paid that bill
//! twice (the ownership bundle format, which cost every asset its rarity rank).
//!
//! `arcade-core` is the right home because it is already the crate both sides
//! share — the client records with it, the worker replays with it — and it is
//! pure serde, so nothing here drags a runtime into either.
//!
//! # Identity is not in these types
//!
//! No player id, no display name, no wallet. Identity rides in the
//! `Authorization: Bearer` header as the platform's widget token, and the
//! server reads the player from its claims. That is deliberate: a player id in
//! the body is a player id the client chose, and a leaderboard whose names are
//! client-supplied is a leaderboard of whatever people felt like typing.

use serde::{Deserialize, Serialize};

/// `POST` — ask for a seed to play with.
pub const SEED_PATH: &str = "/api/seed";

/// `POST` — submit a finished recording for verification.
pub const SUBMIT_PATH: &str = "/api/submit";

/// `GET` — the leaderboard for one game.
pub fn leaderboard_path(game_id: &str) -> String {
    format!("/api/leaderboard/{game_id}")
}

/// Ask the server to start a session.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeedRequest {
    pub game_id: String,
}

/// The server's answer: play *this* seed, and quote this challenge back.
///
/// The challenge is what makes the seed the server's rather than the client's.
/// It is stored server-side against the issuing player and game with a short
/// expiry, and consumed on submit — so a recording can only be submitted
/// against a seed the server actually handed out, once.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeedGrant {
    pub game_id: String,

    /// The seed the game must be constructed with. The server checks the
    /// submitted recording's seed against this, so playing a different one is
    /// not a way to pick an easier board.
    ///
    /// Plain `u64` rather than a string: this JSON is produced and consumed by
    /// Rust at both ends. It transits JS (the miniquad bridge, Discord's proxy)
    /// only as an opaque string, so it never touches JavaScript's number type
    /// and needs none of `wasm_safe_serde`'s string encoding.
    pub seed: u64,

    /// Opaque nonce to quote back in [`ScoreSubmission::challenge`].
    pub challenge: String,

    /// Unix ms after which the challenge is refused. Clients show it; the
    /// server enforces it.
    pub expires_at_ms: u64,
}

/// A finished session, submitted for verification.
///
/// Generic over the recording so one definition serves both ends: the client
/// sends `ScoreSubmission<GameRecording<MyInput>>`, while the server — which
/// dispatches on `game_id` and so cannot name the input type — parses
/// `ScoreSubmission<serde_json::Value>` and hands the inner value to that
/// game's verifier. Deliberately no default type parameter, so `arcade-core`
/// does not take a `serde_json` dependency for the server's convenience.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScoreSubmission<R> {
    pub game_id: String,

    /// The challenge from [`SeedGrant`]. Single use.
    pub challenge: String,

    /// The [`crate::GameRecording`] for this session.
    pub recording: R,
}

/// What the server decided.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubmitOutcome {
    /// Whether the replay reproduced the claimed score.
    pub verified: bool,

    /// The score the *server* computed by replaying. Not the claim — on a
    /// mismatch this is the honest number, which is what a client should show.
    pub score: u64,

    /// Rank within the current epoch, when the score was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,

    /// Whether this player's score counts toward rewards, per the guild's
    /// eligibility rules. Resolved server-side from Discord roles: a client
    /// cannot ask to be eligible. `false` is not a rejection — the score still
    /// lands on the board, it just isn't playing for anything.
    #[serde(default)]
    pub reward_eligible: bool,

    /// Why, when something went wrong. Present on `verified: false` and on a
    /// rejected challenge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One row of a leaderboard.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub rank: u32,

    /// Stable player identity (a Discord user id). For joining, not display.
    pub player_id: String,

    /// What to render. Captured at submit time from the verified token rather
    /// than looked up when the board is drawn — a leaderboard must not need to
    /// call Discord once per row to have names on it.
    pub display_name: String,

    pub score: u64,
    pub epoch: u64,

    /// Whether this entry was reward-eligible when it was set. Recorded per
    /// entry, not per player: roles change, and a board that silently
    /// re-decides old rows is a board that can take a prize back.
    #[serde(default)]
    pub reward_eligible: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_omits_absent_optionals() {
        let outcome = SubmitOutcome {
            verified: true,
            score: 1234,
            rank: Some(3),
            reward_eligible: true,
            reason: None,
        };
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(!json.contains("reason"), "absent reason should be omitted");
        assert!(json.contains("\"rank\":3"));
    }

    /// The server may add fields; an older client must not break on them.
    #[test]
    fn unknown_fields_are_ignored() {
        let json = r#"{"verified":true,"score":10,"reward_eligible":false,"season":"s2"}"#;
        let outcome: SubmitOutcome = serde_json::from_str(json).unwrap();
        assert!(outcome.verified);
        assert_eq!(outcome.rank, None);
    }

    /// The generic is the point: one definition, both ends.
    #[test]
    fn submission_is_generic_over_the_recording() {
        let client = ScoreSubmission {
            game_id: "xeno-invaders".into(),
            challenge: "abc".into(),
            recording: crate::GameRecording::<u8> {
                game_id: "xeno-invaders".into(),
                seed: 7,
                transitions: vec![],
                total_ticks: 3,
                claimed_score: 0,
            },
        };
        let json = serde_json::to_string(&client).unwrap();

        // What the server sees: same bytes, recording left opaque.
        let server: ScoreSubmission<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(server.game_id, "xeno-invaders");
        assert_eq!(server.recording["seed"], 7);
    }
}
