//! Node-to-node block-fetch (mini-protocol 3).
//!
//! ```text
//! Idle      --RequestRange--> Busy
//! Busy      --StartBatch-->   Streaming | --NoBlocks--> Idle
//! Streaming --Block-->        Streaming | --BatchDone--> Idle
//! ```

use futures_io::{AsyncRead, AsyncWrite};
use minicbor::{Decoder, Encoder};

use crate::codec::Point;
use crate::mux::{Mux, MuxError};
use crate::{cbor_message, protocol};

#[derive(Debug, thiserror::Error)]
pub enum BlockFetchError {
    #[error(transparent)]
    Mux(#[from] MuxError),
    #[error("cbor decode error: {0}")]
    Cbor(String),
    #[error("peer has no block at the requested point")]
    NoBlocks,
    #[error("unexpected block-fetch message tag {0}")]
    UnexpectedTag(u64),
}

impl From<minicbor::decode::Error> for BlockFetchError {
    fn from(e: minicbor::decode::Error) -> Self {
        Self::Cbor(cbor_message(e))
    }
}

/// Fetch the one block at `point`.
///
/// Returns the era-wrapped block bytes, `[era_tag, block]`, where the tag
/// counts Byron's two block kinds separately: 0 epoch-boundary, 1 Byron,
/// 2 Shelley, 3 Allegra, 4 Mary, 5 Alonzo, 6 Babbage, 7 Conway. This is the
/// same shape `pallas_traverse::MultiEraBlock::decode` takes.
pub async fn fetch_single<S: AsyncRead + AsyncWrite + Unpin>(
    mux: &mut Mux<S>,
    point: &Point,
) -> Result<Vec<u8>, BlockFetchError> {
    mux.send(protocol::BLOCK_FETCH, &encode_request_range(point, point))
        .await?;

    match message_tag(&mux.recv(protocol::BLOCK_FETCH).await?)? {
        2 => {}
        3 => return Err(BlockFetchError::NoBlocks),
        other => return Err(BlockFetchError::UnexpectedTag(other)),
    }

    let block = decode_block(&mux.recv(protocol::BLOCK_FETCH).await?)?;

    match message_tag(&mux.recv(protocol::BLOCK_FETCH).await?)? {
        5 => Ok(block),
        other => Err(BlockFetchError::UnexpectedTag(other)),
    }
}

/// `[0, from, to]`
pub fn encode_request_range(from: &Point, to: &Point) -> Vec<u8> {
    let mut buf = Vec::with_capacity(96);
    Encoder::new(&mut buf)
        .array(3)
        .and_then(|e| e.u8(0))
        .and_then(|e| e.encode(from))
        .and_then(|e| e.encode(to))
        .expect("vec write");
    buf
}

fn message_tag(data: &[u8]) -> Result<u64, BlockFetchError> {
    let mut d = Decoder::new(data);
    d.array()?;
    Ok(d.u64()?)
}

/// `[4, #6.24(bytes)]`
fn decode_block(data: &[u8]) -> Result<Vec<u8>, BlockFetchError> {
    let mut d = Decoder::new(data);
    d.array()?;
    let tag = d.u64()?;
    if tag != 4 {
        return Err(BlockFetchError::UnexpectedTag(tag));
    }
    let wrapper = d.tag()?;
    if wrapper != minicbor::data::Tag::new(24) {
        return Err(BlockFetchError::Cbor(format!(
            "expected CBOR tag 24 around the block, got {wrapper:?}"
        )));
    }
    Ok(d.bytes()?.to_vec())
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::mux::test_support::{ScriptedPeer, Step, segment};

    #[test]
    fn fetches_one_block_and_writes_the_request() {
        let block = vec![0x82, 0x07, 0x80];
        let mut wrapped = Vec::new();
        Encoder::new(&mut wrapped)
            .array(2)
            .and_then(|e| e.u8(4))
            .and_then(|e| e.tag(minicbor::data::Tag::new(24)))
            .and_then(|e| e.bytes(&block))
            .unwrap();

        let mut stream = segment(3, &[0x81, 0x02]);
        stream.extend(segment(3, &wrapped));
        stream.extend(segment(3, &[0x81, 0x05]));

        let point = Point::Specific {
            slot: 42,
            hash: [3; 32],
        };
        let mut mux = Mux::new(ScriptedPeer::new([Step::Bytes(stream)]));
        assert_eq!(block_on(fetch_single(&mut mux, &point)).unwrap(), block);

        let written = mux.into_inner().written;
        assert_eq!(&written[8..], &encode_request_range(&point, &point)[..]);
    }

    #[test]
    fn no_blocks_is_its_own_error() {
        let stream = segment(3, &[0x81, 0x03]);
        let mut mux = Mux::new(ScriptedPeer::new([Step::Bytes(stream)]));
        let point = Point::Origin;
        assert!(matches!(
            block_on(fetch_single(&mut mux, &point)),
            Err(BlockFetchError::NoBlocks)
        ));
    }
}
