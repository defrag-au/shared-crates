use thiserror::Error;

use crate::clip::Track;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("unknown track '{0}'")]
    UnknownTrack(String),
    #[error("unknown interp '{0}'")]
    UnknownInterp(String),
    #[error("unknown wrap '{0}'")]
    UnknownWrap(String),
    #[error("unknown ease '{0}'")]
    UnknownEase(String),
    #[error("unknown key '{0}'")]
    UnknownKey(String),
    #[error("dump parse: {0}")]
    Parse(String),
    #[error("unknown target '{0}'")]
    UnknownTarget(String),
    #[error("track {1} does not apply to '{0}'")]
    BadTrack(String, Track),
    #[error("empty clip")]
    EmptyClip,
    #[error(transparent)]
    Scene(#[from] scenegraph::Error),
}
