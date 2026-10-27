use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("need scene.sg1 or world.sw1")]
    MissingRest,
    #[error("{path}: {source}")]
    Scene {
        path: String,
        #[source]
        source: scenegraph::Error,
    },
    #[error("{path}: {source}")]
    Anim {
        path: String,
        #[source]
        source: sceneanim::Error,
    },
    #[error("{path}: {source}")]
    Input {
        path: String,
        #[source]
        source: sceneinput::Error,
    },
    #[error("{path}: {source}")]
    Drive {
        path: String,
        #[source]
        source: scenedrv::Error,
    },
    #[error("{path}: {source}")]
    World {
        path: String,
        #[source]
        source: sceneworld::Error,
    },
    #[error("{0}")]
    Io(String),
    #[error("round-trip {0}")]
    RoundTrip(String),
    /// Two documents that must agree do not. `detail` names each differing
    /// node or line on both sides (`want:` / `got:`).
    #[error("{what}\n{detail}")]
    Mismatch { what: String, detail: String },
    #[error("hit expected {want}, got {got}")]
    HitMismatch { want: String, got: String },
    #[error("play wrote rest; add after.sg1 (scene-gauntlet DIR --emit writes it)")]
    MissingAfter,
    #[error("play wrote rest; add delta.sgd1 (scene-gauntlet DIR --emit writes it)")]
    MissingDelta,
    #[error("act target expected {want}, got {got}")]
    ActTarget { want: String, got: String },
    #[error("dump parse: {0}")]
    ActParse(String),
    #[error("unknown op '{0}'")]
    UnknownOp(String),
    #[error("unknown key '{0}'")]
    UnknownKey(String),
    #[error("mark needs node=")]
    MarkNeedsNode,
    #[error("mark needs text=")]
    MarkNeedsText,
    #[error("set needs node=")]
    SetNeedsNode,
    #[error("set needs text=")]
    SetNeedsText,
    #[error("remove needs node=")]
    RemoveNeedsNode,
    #[error("to needs node=")]
    ToNeedsNode,
    #[error("to needs t=, r=, or s=")]
    ToNeedsDest,
    #[error("unknown node '{0}'")]
    UnknownActNode(String),
    /// The whole gauntlet dump, led by its `problems N` section.
    #[error("{0}")]
    Problems(String),
    #[error("game: {0}")]
    Game(String),
    #[error("replay.rp1: {0}")]
    ReplayParse(String),
    #[error("replay.rp1 line {line}: {what} mismatch\nwant:\n{want}\ngot:\n{got}")]
    ReplayMismatch {
        line: usize,
        what: String,
        want: String,
        got: String,
    },
    #[error("replay.rp1 line {line}: only a pick runs without a game, and it is never a move")]
    ReplayNeedsGame { line: usize },
}

impl Error {
    /// Two scene dumps that must be equal: per-node `want:` / `got:`.
    pub fn scenes(what: impl Into<String>, want: &str, got: &str) -> Self {
        Error::Mismatch {
            what: what.into(),
            detail: crate::mismatch::scenes(want, got),
        }
    }

    /// Two line dumps (a delta) that must be equal.
    pub fn lines(what: impl Into<String>, want: &str, got: &str) -> Self {
        Error::Mismatch {
            what: what.into(),
            detail: crate::mismatch::lines(want, got),
        }
    }

    pub fn io(e: impl std::fmt::Display) -> Self {
        Error::Io(e.to_string())
    }
}
