use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("unknown tool '{0}'")]
    UnknownTool(String),
    #[error("unknown face '{0}'")]
    UnknownFace(String),
    #[error("unknown key '{0}'")]
    UnknownKey(String),
    #[error("dump parse: {0}")]
    Parse(String),
    #[error(transparent)]
    Scene(#[from] scenegraph::Error),
}
