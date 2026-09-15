//! Node-to-node handshake (mini-protocol 0).
//!
//! The client proposes a version table; the server accepts one version or
//! refuses. Versions 13 and 14 match what `pallas-network` 1.1.1 proposes, and
//! that is what mitos syncs mainnet with every day.

use futures_io::{AsyncRead, AsyncWrite};
use minicbor::{Decoder, Encoder};

use crate::mux::{Mux, MuxError};
use crate::{cbor_message, protocol};

/// Versions proposed, ascending (the version table is a CBOR map keyed by
/// version, and nodes expect canonical key order).
pub const PROPOSED_VERSIONS: &[u64] = &[13, 14];

/// Whether this end will also answer mini-protocols the peer starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffusionMode {
    /// Client only. The peer never starts a protocol towards us. This is the
    /// right mode for anything that only follows or submits.
    InitiatorOnly,
    /// Duplex. Only meaningful for a node that runs responders.
    InitiatorAndResponder,
}

/// A successful handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    pub version: u64,
    pub network_magic: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum HandshakeError {
    #[error(transparent)]
    Mux(#[from] MuxError),
    #[error("cbor decode error: {0}")]
    Cbor(String),
    #[error("no common version; the peer supports {0:?}")]
    VersionMismatch(Vec<u64>),
    #[error("peer could not decode version {version}: {reason}")]
    DecodeError { version: u64, reason: String },
    #[error("peer refused version {version}: {reason}")]
    Refused { version: u64, reason: String },
    #[error("peer accepted magic {actual}, expected {expected}")]
    MagicMismatch { expected: u64, actual: u64 },
    #[error("unexpected handshake message tag {0}")]
    UnexpectedTag(u64),
}

/// Propose [`PROPOSED_VERSIONS`] and wait for the answer.
pub async fn handshake<S: AsyncRead + AsyncWrite + Unpin>(
    mux: &mut Mux<S>,
    network_magic: u64,
    mode: DiffusionMode,
) -> Result<Accepted, HandshakeError> {
    mux.send(protocol::HANDSHAKE, &encode_propose(network_magic, mode))
        .await?;
    let reply = mux.recv(protocol::HANDSHAKE).await?;
    let accepted = decode_reply(&reply)?;
    if accepted.network_magic != network_magic {
        return Err(HandshakeError::MagicMismatch {
            expected: network_magic,
            actual: accepted.network_magic,
        });
    }
    Ok(accepted)
}

/// `[0, { version => [magic, initiator_only, peer_sharing, query] }]`
pub fn encode_propose(network_magic: u64, mode: DiffusionMode) -> Vec<u8> {
    let initiator_only = matches!(mode, DiffusionMode::InitiatorOnly);
    let mut buf = Vec::with_capacity(64);
    let mut e = Encoder::new(&mut buf);
    e.array(2).and_then(|e| e.u8(0)).expect("vec write");
    e.map(PROPOSED_VERSIONS.len() as u64).expect("vec write");
    for version in PROPOSED_VERSIONS {
        e.u64(*version)
            .and_then(|e| e.array(4))
            .and_then(|e| e.u64(network_magic))
            .and_then(|e| e.bool(initiator_only))
            // peer sharing disabled
            .and_then(|e| e.u8(0))
            // not a query handshake
            .and_then(|e| e.bool(false))
            .expect("vec write");
    }
    buf
}

pub fn decode_reply(data: &[u8]) -> Result<Accepted, HandshakeError> {
    let cbor = |e: minicbor::decode::Error| HandshakeError::Cbor(cbor_message(e));
    let mut d = Decoder::new(data);
    d.array().map_err(cbor)?;
    match d.u64().map_err(cbor)? {
        // AcceptVersion = [1, version, version_data]
        1 => {
            let version = d.u64().map_err(cbor)?;
            // Version data is [magic, initiator_only] or
            // [magic, initiator_only, peer_sharing, query]; only the magic matters.
            d.array().map_err(cbor)?;
            let network_magic = d.u64().map_err(cbor)?;
            Ok(Accepted {
                version,
                network_magic,
            })
        }
        // Refuse = [2, reason]
        2 => {
            d.array().map_err(cbor)?;
            match d.u64().map_err(cbor)? {
                0 => {
                    let count = d.array().map_err(cbor)?.unwrap_or(0);
                    let versions = (0..count)
                        .map(|_| d.u64().map_err(cbor))
                        .collect::<Result<Vec<_>, _>>()?;
                    Err(HandshakeError::VersionMismatch(versions))
                }
                1 => {
                    let version = d.u64().map_err(cbor)?;
                    let reason = d.str().map_err(cbor)?.to_string();
                    Err(HandshakeError::DecodeError { version, reason })
                }
                2 => {
                    let version = d.u64().map_err(cbor)?;
                    let reason = d.str().map_err(cbor)?.to_string();
                    Err(HandshakeError::Refused { version, reason })
                }
                other => Err(HandshakeError::UnexpectedTag(other)),
            }
        }
        other => Err(HandshakeError::UnexpectedTag(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn propose_declares_initiator_only_for_every_version() {
        let bytes = encode_propose(crate::magic::MAINNET, DiffusionMode::InitiatorOnly);
        let mut d = Decoder::new(&bytes);
        assert_eq!(d.array().unwrap(), Some(2));
        assert_eq!(d.u8().unwrap(), 0);
        assert_eq!(d.map().unwrap(), Some(2));
        for expected in PROPOSED_VERSIONS {
            assert_eq!(d.u64().unwrap(), *expected);
            assert_eq!(d.array().unwrap(), Some(4));
            assert_eq!(d.u64().unwrap(), crate::magic::MAINNET);
            assert!(d.bool().unwrap(), "initiator-only flag");
            assert_eq!(d.u8().unwrap(), 0);
            assert!(!d.bool().unwrap());
        }
    }

    #[test]
    fn decodes_accept_and_each_refusal() {
        let mut accept = Vec::new();
        Encoder::new(&mut accept)
            .array(3)
            .and_then(|e| e.u8(1))
            .and_then(|e| e.u64(14))
            .and_then(|e| e.array(4))
            .and_then(|e| e.u64(1))
            .and_then(|e| e.bool(true))
            .and_then(|e| e.u8(0))
            .and_then(|e| e.bool(false))
            .unwrap();
        assert_eq!(
            decode_reply(&accept).unwrap(),
            Accepted {
                version: 14,
                network_magic: 1
            }
        );

        let mut mismatch = Vec::new();
        Encoder::new(&mut mismatch)
            .array(2)
            .and_then(|e| e.u8(2))
            .and_then(|e| e.array(2))
            .and_then(|e| e.u8(0))
            .and_then(|e| e.array(2))
            .and_then(|e| e.u64(15))
            .and_then(|e| e.u64(16))
            .unwrap();
        assert!(matches!(
            decode_reply(&mismatch),
            Err(HandshakeError::VersionMismatch(v)) if v == vec![15, 16]
        ));

        let mut refused = Vec::new();
        Encoder::new(&mut refused)
            .array(2)
            .and_then(|e| e.u8(2))
            .and_then(|e| e.array(3))
            .and_then(|e| e.u8(2))
            .and_then(|e| e.u64(14))
            .and_then(|e| e.str("no"))
            .unwrap();
        assert!(matches!(
            decode_reply(&refused),
            Err(HandshakeError::Refused { version: 14, .. })
        ));
    }
}
