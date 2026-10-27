use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("unknown op '{0}'")]
    UnknownOp(String),
    #[error("unknown wrap '{0}'")]
    UnknownWrap(String),
    #[error("unknown key '{0}'")]
    UnknownKey(String),
    #[error("dump parse: {0}")]
    Parse(String),
    #[error("fan needs tag=")]
    FanNeedsTag,
    #[error("no nodes tagged '{0}'")]
    EmptyTag(String),
    #[error("orbit needs cam=")]
    OrbitNeedsCam,
    #[error("unknown camera '{0}'")]
    UnknownCamera(String),
    #[error("'{0}' is not a Camera")]
    NotCamera(String),
    #[error("to needs node=")]
    ToNeedsNode,
    #[error("to needs t=, s=, r=, or a=")]
    ToNeedsDest,
    #[error("unknown node '{0}'")]
    UnknownNode(String),
    #[error("driver has no ops")]
    EmptyDriver,
    #[error(transparent)]
    Anim(#[from] sceneanim::Error),
    #[error(transparent)]
    Scene(#[from] scenegraph::Error),
}
