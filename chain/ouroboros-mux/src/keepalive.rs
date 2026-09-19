//! Node-to-node keep-alive (mini-protocol 8).
//!
//! The client sends `[0, cookie]` and the server echoes `[1, cookie]`. It is
//! the only traffic on a connection whose chain is quiet, so an unanswered
//! ping is how a follower tells a dead socket from a slow chain.

use minicbor::{Decoder, Encoder};

use crate::cbor_message;

#[derive(Debug, thiserror::Error)]
pub enum KeepAliveError {
    #[error("cbor decode error: {0}")]
    Cbor(String),
    #[error("unexpected keep-alive message tag {0}")]
    UnexpectedTag(u64),
}

impl From<minicbor::decode::Error> for KeepAliveError {
    fn from(e: minicbor::decode::Error) -> Self {
        Self::Cbor(cbor_message(e))
    }
}

/// `[0, cookie]`
pub fn encode_keep_alive(cookie: u16) -> Vec<u8> {
    let mut buf = Vec::with_capacity(6);
    Encoder::new(&mut buf)
        .array(2)
        .and_then(|e| e.u8(0))
        .and_then(|e| e.u16(cookie))
        .expect("vec write");
    buf
}

/// Decode `[1, cookie]` and return the cookie.
pub fn decode_response(data: &[u8]) -> Result<u16, KeepAliveError> {
    let mut d = Decoder::new(data);
    d.array()?;
    match d.u64()? {
        1 => Ok(d.u16()?),
        other => Err(KeepAliveError::UnexpectedTag(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_and_response() {
        assert_eq!(encode_keep_alive(7), vec![0x82, 0x00, 0x07]);
        assert_eq!(decode_response(&[0x82, 0x01, 0x07]).unwrap(), 7);
        assert!(matches!(
            decode_response(&[0x82, 0x00, 0x07]),
            Err(KeepAliveError::UnexpectedTag(0))
        ));
    }
}
