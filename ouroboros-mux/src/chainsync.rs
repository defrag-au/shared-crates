//! Node-to-node chain-sync (mini-protocol 2), headers variant.
//!
//! ```text
//! Idle      --RequestNext-->   CanAwait
//! CanAwait  --AwaitReply-->    MustReply
//! CanAwait / MustReply --RollForward | RollBackward--> Idle
//! Idle      --FindIntersect--> Intersect --IntersectFound | IntersectNotFound--> Idle
//! ```
//!
//! The encode and decode functions are sans-IO so a follower can drive the
//! protocol itself (for example racing the reply against a keep-alive timer).
//! [`find_intersect`] and [`request_next`] are the simple async forms.
//!
//! After a successful intersect the peer's first reply to `RequestNext` is a
//! `RollBackward` to the intersection point. That is protocol, not a reorg.

use futures_io::{AsyncRead, AsyncWrite};
use minicbor::{Decoder, Encoder};

use crate::codec::{Point, Tip};
use crate::mux::{Mux, MuxError};
use crate::{cbor_message, protocol};

#[derive(Debug, thiserror::Error)]
pub enum ChainSyncError {
    #[error(transparent)]
    Mux(#[from] MuxError),
    #[error("cbor decode error: {0}")]
    Cbor(String),
    #[error("unexpected chain-sync message tag {0}")]
    UnexpectedTag(u64),
    #[error("peer ended chain-sync")]
    Done,
}

impl From<minicbor::decode::Error> for ChainSyncError {
    fn from(e: minicbor::decode::Error) -> Self {
        Self::Cbor(cbor_message(e))
    }
}

/// A header delivered by `RollForward`, still era-wrapped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderContent {
    /// Hard-fork-combinator era index: 0 Byron, 1 Shelley, 2 Allegra, 3 Mary,
    /// 4 Alonzo, 5 Babbage, 6 Conway. Note block-fetch bodies number eras
    /// differently (one higher from Shelley on, because Byron has two block
    /// kinds).
    pub variant: u8,
    /// Byron only: 0 for an epoch-boundary block, 1 for a main block.
    pub byron_subtag: Option<u8>,
    /// The header CBOR itself (the payload of the tag-24 wrapper).
    pub cbor: Vec<u8>,
}

/// A reply to `RequestNext`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    RollForward(HeaderContent, Tip),
    RollBackward(Point, Tip),
    /// The peer is at its tip; the real reply follows when a block arrives.
    Await,
}

/// A reply to `FindIntersect`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intersect {
    Found(Point, Tip),
    NotFound(Tip),
}

impl Intersect {
    pub fn tip(&self) -> &Tip {
        match self {
            Self::Found(_, tip) | Self::NotFound(tip) => tip,
        }
    }
}

/// `FindIntersect` with `points`, most preferred first. An empty list always
/// yields `NotFound` carrying the peer's tip, which is the cheapest way to ask
/// for it.
pub async fn find_intersect<S: AsyncRead + AsyncWrite + Unpin>(
    mux: &mut Mux<S>,
    points: &[Point],
) -> Result<Intersect, ChainSyncError> {
    mux.send(protocol::CHAIN_SYNC, &encode_find_intersect(points))
        .await?;
    let reply = mux.recv(protocol::CHAIN_SYNC).await?;
    decode_intersect(&reply)
}

/// `RequestNext` and its first reply (which may be [`Next::Await`]).
pub async fn request_next<S: AsyncRead + AsyncWrite + Unpin>(
    mux: &mut Mux<S>,
) -> Result<Next, ChainSyncError> {
    mux.send(protocol::CHAIN_SYNC, &encode_request_next()).await?;
    let reply = mux.recv(protocol::CHAIN_SYNC).await?;
    decode_next(&reply)
}

/// `[0]`
pub fn encode_request_next() -> Vec<u8> {
    vec![0x81, 0x00]
}

/// `[4, [point, ...]]`
pub fn encode_find_intersect(points: &[Point]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(16 + points.len() * 44);
    let mut e = Encoder::new(&mut buf);
    e.array(2)
        .and_then(|e| e.u8(4))
        .and_then(|e| e.array(points.len() as u64))
        .expect("vec write");
    for point in points {
        e.encode(point).expect("vec write");
    }
    buf
}

pub fn decode_intersect(data: &[u8]) -> Result<Intersect, ChainSyncError> {
    let mut d = Decoder::new(data);
    d.array()?;
    match d.u64()? {
        5 => Ok(Intersect::Found(d.decode()?, d.decode()?)),
        6 => Ok(Intersect::NotFound(d.decode()?)),
        7 => Err(ChainSyncError::Done),
        other => Err(ChainSyncError::UnexpectedTag(other)),
    }
}

pub fn decode_next(data: &[u8]) -> Result<Next, ChainSyncError> {
    let mut d = Decoder::new(data);
    d.array()?;
    match d.u64()? {
        1 => Ok(Next::Await),
        2 => {
            let header = decode_header_content(&mut d)?;
            Ok(Next::RollForward(header, d.decode()?))
        }
        3 => Ok(Next::RollBackward(d.decode()?, d.decode()?)),
        7 => Err(ChainSyncError::Done),
        other => Err(ChainSyncError::UnexpectedTag(other)),
    }
}

/// Byron:    `[0, [[subtag, size], #6.24(bytes)]]`
/// Shelley+: `[variant, #6.24(bytes)]`
fn decode_header_content(d: &mut Decoder<'_>) -> Result<HeaderContent, ChainSyncError> {
    d.array()?;
    let variant = d.u8()?;
    let byron_subtag = if variant == 0 {
        d.array()?;
        d.array()?;
        let subtag = d.u8()?;
        d.skip()?;
        Some(subtag)
    } else {
        None
    };
    let tag = d.tag()?;
    if tag != minicbor::data::Tag::new(24) {
        return Err(ChainSyncError::Cbor(format!(
            "expected CBOR tag 24 around the header, got {tag:?}"
        )));
    }
    let cbor = d.bytes()?.to_vec();
    Ok(HeaderContent {
        variant,
        byron_subtag,
        cbor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tip() -> Tip {
        Tip {
            point: Point::Specific {
                slot: 10,
                hash: [1; 32],
            },
            block_number: 5,
        }
    }

    #[test]
    fn find_intersect_encodes_its_points() {
        let bytes = encode_find_intersect(&[Point::Origin]);
        assert_eq!(bytes, vec![0x82, 0x04, 0x81, 0x80]);
    }

    #[test]
    fn decodes_roll_forward_with_a_shelley_era_header() {
        let mut bytes = Vec::new();
        let mut e = Encoder::new(&mut bytes);
        e.array(3)
            .and_then(|e| e.u8(2))
            .and_then(|e| e.array(2))
            .and_then(|e| e.u8(6))
            .and_then(|e| e.tag(minicbor::data::Tag::new(24)))
            .and_then(|e| e.bytes(&[0xAA, 0xBB]))
            .and_then(|e| e.encode(tip()))
            .unwrap();

        assert_eq!(
            decode_next(&bytes).unwrap(),
            Next::RollForward(
                HeaderContent {
                    variant: 6,
                    byron_subtag: None,
                    cbor: vec![0xAA, 0xBB]
                },
                tip()
            )
        );
    }

    #[test]
    fn decodes_await_rollback_and_intersect_replies() {
        assert_eq!(decode_next(&[0x81, 0x01]).unwrap(), Next::Await);

        let mut rollback = Vec::new();
        Encoder::new(&mut rollback)
            .array(3)
            .and_then(|e| e.u8(3))
            .and_then(|e| e.encode(Point::Origin))
            .and_then(|e| e.encode(tip()))
            .unwrap();
        assert_eq!(
            decode_next(&rollback).unwrap(),
            Next::RollBackward(Point::Origin, tip())
        );

        let mut not_found = Vec::new();
        Encoder::new(&mut not_found)
            .array(2)
            .and_then(|e| e.u8(6))
            .and_then(|e| e.encode(tip()))
            .unwrap();
        assert_eq!(
            decode_intersect(&not_found).unwrap(),
            Intersect::NotFound(tip())
        );
    }
}
