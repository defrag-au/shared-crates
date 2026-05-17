//! IPFS CID extraction and normalisation.
//!
//! [`AssetMetadata::extract_cids`] collects every IPFS CID a piece of
//! asset metadata references — the headline `image` plus every
//! `files[].src` — and returns them normalised to CIDv1 so they sort and
//! dedupe stably regardless of how they were originally encoded on-chain.
//!
//! This module is pure (no chain dependencies) and is always compiled; the
//! CIP-68 datum decoder that feeds it lives behind the `cip68` feature.

use crate::{get_image_url, Asset, AssetFile, AssetMetadata, AssetMetadata68, PrimitiveOrList};
use std::collections::HashSet;

/// Where in the metadata a CID was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CidRole {
    /// The headline `image` field.
    Image,
    /// An entry in the `files[]` array (`files[].src`).
    File,
}

/// A single IPFS CID recovered from asset metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedCid {
    /// The CID, normalised to CIDv1 (base32).
    pub cid: String,
    /// Where it was referenced from.
    pub role: CidRole,
    /// The `mediaType` of the referencing entry, when known (`files[]` only).
    pub media_type: Option<String>,
}

impl AssetMetadata {
    /// Collect every IPFS CID this metadata references, normalised to
    /// CIDv1 and deduped (first occurrence wins). HTTP and other
    /// non-IPFS URLs are skipped — this is an IPFS preservation index.
    #[must_use]
    pub fn extract_cids(&self) -> Vec<ExtractedCid> {
        let (image, files) = self.image_and_files();
        let mut seen = HashSet::new();
        let mut out = Vec::new();

        if let Some(cid) = cid_from_url(&get_image_url(image.clone())) {
            if seen.insert(cid.clone()) {
                out.push(ExtractedCid {
                    cid,
                    role: CidRole::Image,
                    media_type: None,
                });
            }
        }

        if let Some(files) = files {
            for file in files {
                if let Some(cid) = cid_from_url(&file.get_src()) {
                    if seen.insert(cid.clone()) {
                        out.push(ExtractedCid {
                            cid,
                            role: CidRole::File,
                            media_type: Some(file.media_type().to_owned()),
                        });
                    }
                }
            }
        }

        out
    }

    /// The `image` and `files` fields, which every variant carries.
    fn image_and_files(&self) -> (&PrimitiveOrList<String>, &Option<Vec<AssetFile>>) {
        match self {
            AssetMetadata::Attributed { image, files, .. }
            | AssetMetadata::Flattened { image, files, .. }
            | AssetMetadata::FlattenedMixed { image, files, .. }
            | AssetMetadata::CodifiedTraits { image, files, .. }
            | AssetMetadata::ColonDelimitedAttributes { image, files, .. }
            | AssetMetadata::AttributeArray { image, files, .. }
            | AssetMetadata::UnsignedAlgorithms { image, files, .. } => (image, files),
        }
    }
}

impl AssetMetadata68 {
    /// Collect every IPFS CID this CIP-68 metadata references.
    /// See [`AssetMetadata::extract_cids`].
    #[must_use]
    pub fn extract_cids(&self) -> Vec<ExtractedCid> {
        self.metadata.extract_cids()
    }
}

/// Pull a CID out of a metadata URL string and normalise it to CIDv1.
///
/// Handles `ipfs://<cid>`, `ipfs://ipfs/<cid>`, gateway URLs containing
/// `/ipfs/<cid>`, and bare CIDs. Returns `None` for anything that is not
/// a recognisable IPFS CID (e.g. plain `https://` image URLs).
fn cid_from_url(url: &str) -> Option<String> {
    let url = url.trim();
    let candidate = if let Some(rest) = url.strip_prefix("ipfs://") {
        // Tolerate the occasional `ipfs://ipfs/<cid>` double prefix.
        let rest = rest.strip_prefix("ipfs/").unwrap_or(rest);
        rest.split('/').next().unwrap_or("")
    } else if let Some(idx) = url.find("/ipfs/") {
        url[idx + "/ipfs/".len()..].split('/').next().unwrap_or("")
    } else {
        url
    };
    normalize_cid(candidate.trim())
}

/// Validate `s` as an IPFS CID and return it normalised to CIDv1.
///
/// CIDv0 (`Qm…`) values are converted to their canonical CIDv1 form so
/// the index sorts and dedupes by a single representation. Returns
/// `None` if `s` is not a recognisable CID.
#[must_use]
pub fn normalize_cid(s: &str) -> Option<String> {
    if s.len() == 46 && s.starts_with("Qm") {
        return cid_v0_to_v1(s);
    }
    if Asset::is_valid_cid(s) {
        // Already a CIDv1.
        return Some(s.to_owned());
    }
    None
}

/// Convert a CIDv0 (`Qm…`, base58btc-encoded dag-pb / sha2-256 multihash)
/// to its canonical CIDv1 (`bafybei…`, base32) form.
///
/// Returns `None` if `s` is not a structurally valid CIDv0.
#[must_use]
pub fn cid_v0_to_v1(s: &str) -> Option<String> {
    let multihash = base58btc_decode(s)?;
    // A CIDv0 is always a sha2-256 multihash: 0x12 (hash code) followed
    // by 0x20 (32-byte digest length) and the 32-byte digest.
    if multihash.len() != 34 || multihash[0] != 0x12 || multihash[1] != 0x20 {
        return None;
    }
    // CIDv1 = <version 0x01> <codec 0x70 = dag-pb> <multihash>, then
    // base32-lower with a `b` multibase prefix.
    let mut payload = Vec::with_capacity(2 + multihash.len());
    payload.push(0x01);
    payload.push(0x70);
    payload.extend_from_slice(&multihash);
    Some(format!("b{}", base32_lower_encode(&payload)))
}

/// Decode a base58btc (Bitcoin alphabet) string to bytes.
fn base58btc_decode(s: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut bytes: Vec<u8> = Vec::with_capacity(s.len());
    for ch in s.bytes() {
        let mut carry = ALPHABET.iter().position(|&a| a == ch)? as u32;
        for b in &mut bytes {
            carry += u32::from(*b) * 58;
            *b = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            bytes.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    // Each leading '1' encodes one leading zero byte.
    for ch in s.bytes() {
        if ch == b'1' {
            bytes.push(0);
        } else {
            break;
        }
    }
    bytes.reverse();
    Some(bytes)
}

/// Encode bytes as lowercase RFC 4648 base32, no padding.
fn base32_lower_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::with_capacity(data.len() * 8 / 5 + 1);
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in data {
        acc = (acc << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 0x1f) as usize] as char);
        }
        acc &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 0x1f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Golden CIDv0 -> CIDv1 vectors. The first is the canonical example
    // from the IPFS documentation; the second is the Nikeverse1501
    // pre-update image. Both were cross-checked with an independent
    // base58/base32 implementation.
    const CIDV0_VECTORS: &[(&str, &str)] = &[
        (
            "QmbWqxBEKC3P8tqsKc98xmWNzrzDtRLMiMPL8wBuTGsMnR",
            "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
        ),
        (
            "QmQVcekGM1VRMHZnDjNPvYiEQ3mYYRaXiEAPs2qwHHk8kv",
            "bafybeibaanddhnz7v7quubnv3dlngzkl2x56zxllbyhtfck3kybrepaee4",
        ),
    ];

    #[test]
    fn converts_cidv0_to_cidv1() {
        for (v0, v1) in CIDV0_VECTORS {
            assert_eq!(cid_v0_to_v1(v0).as_deref(), Some(*v1), "for {v0}");
        }
    }

    #[test]
    fn rejects_invalid_cidv0() {
        assert!(cid_v0_to_v1("QmNotARealCidValueHereJustGarbage1234567890xx").is_none());
        assert!(cid_v0_to_v1("not-a-cid").is_none());
    }

    #[test]
    fn normalize_passes_through_cidv1() {
        let v1 = "bafybeidw54qa6bcbbjnztbbj6cd7qzazr33instef33ql4lws45mp6uw3e";
        assert_eq!(normalize_cid(v1).as_deref(), Some(v1));
    }

    #[test]
    fn normalize_upgrades_cidv0() {
        assert_eq!(
            normalize_cid("QmbWqxBEKC3P8tqsKc98xmWNzrzDtRLMiMPL8wBuTGsMnR").as_deref(),
            Some("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"),
        );
    }

    #[test]
    fn normalize_rejects_non_cid() {
        assert!(normalize_cid("https://example.com/x.png").is_none());
        assert!(normalize_cid("").is_none());
    }

    #[test]
    fn extracts_image_and_file_cids() {
        let json = r#"{
            "name": "Guthix",
            "image": "ipfs://QmbWqxBEKC3P8tqsKc98xmWNzrzDtRLMiMPL8wBuTGsMnR",
            "mediaType": "image/png",
            "files": [
                {
                    "name": "hi-res",
                    "mediaType": "image/webp",
                    "src": "ipfs://bafybeidw54qa6bcbbjnztbbj6cd7qzazr33instef33ql4lws45mp6uw3e"
                }
            ],
            "tier": "Elder"
        }"#;
        let metadata: AssetMetadata = serde_json::from_str(json).unwrap();
        let cids = metadata.extract_cids();
        assert_eq!(cids.len(), 2);
        assert_eq!(cids[0].role, CidRole::Image);
        assert_eq!(
            cids[0].cid,
            "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"
        );
        assert_eq!(cids[1].role, CidRole::File);
        assert_eq!(cids[1].media_type.as_deref(), Some("image/webp"));
        assert_eq!(
            cids[1].cid,
            "bafybeidw54qa6bcbbjnztbbj6cd7qzazr33instef33ql4lws45mp6uw3e"
        );
    }

    #[test]
    fn skips_http_image_urls() {
        let json = r#"{"name": "x", "image": "https://example.com/x.png"}"#;
        let metadata: AssetMetadata = serde_json::from_str(json).unwrap();
        assert!(metadata.extract_cids().is_empty());
    }

    #[test]
    fn dedupes_repeated_cids() {
        let cid = "bafybeidw54qa6bcbbjnztbbj6cd7qzazr33instef33ql4lws45mp6uw3e";
        let json = format!(
            r#"{{
                "name": "x",
                "image": "ipfs://{cid}",
                "files": [{{"mediaType": "image/png", "src": "ipfs://{cid}"}}]
            }}"#
        );
        let metadata: AssetMetadata = serde_json::from_str(&json).unwrap();
        let cids = metadata.extract_cids();
        assert_eq!(cids.len(), 1, "the same CID across image + files dedupes");
        assert_eq!(cids[0].role, CidRole::Image);
    }
}
