use thiserror::Error;

use crate::world::Wall;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("unknown key '{0}'")]
    UnknownKey(String),
    #[error("unknown wall '{0}'")]
    UnknownWall(String),
    #[error("dump parse: {0}")]
    Parse(String),
    #[error("hang needs c=")]
    HangNeedsCell,
    #[error("hang needs wall=")]
    HangNeedsWall,
    #[error("hang needs name=")]
    HangNeedsName,
    #[error("hang needs m=")]
    HangNeedsImage,
    #[error("duplicate hang '{0}'")]
    DuplicateHang(String),
    #[error("duplicate hang on {wall} at {x},{z}")]
    DuplicateSlot { wall: Wall, x: i32, z: i32 },
    #[error("hilbert order {0} exceeds 8")]
    HilbertOrder(u32),
    #[error("hilbert order {order} has no slot {i}")]
    HilbertFull { order: u32, i: u32 },
    #[error(transparent)]
    Scene(#[from] scenegraph::Error),
}
