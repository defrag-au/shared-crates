//! IPFS CID extraction and normalisation.
//!
//! [`AssetMetadata::extract_cids`] collects every IPFS CID a piece of
//! asset metadata references — the headline `image` plus every
//! `files[].src` — and returns them normalised to CIDv1 so they sort and
//! dedupe stably regardless of how they were originally encoded on-chain.
//!
//! This module is pure (no chain dependencies) and is always compiled; the
//! CIP-68 datum decoder that feeds it lives behind the `cip68` feature.

use crate::{Asset, AssetFile, AssetMetadata, AssetMetadata68, PrimitiveOrList};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Where in the metadata a CID was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CidRole {
    /// The headline `image` field.
    Image,
    /// An entry in the `files[]` array (`files[].src`).
    File,
}

/// A single IPFS CID recovered from asset metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ExtractedCid {
    /// The CID, normalised to CIDv1 (base32).
    pub cid: String,
    /// Where it was referenced from.
    pub role: CidRole,
    /// The `mediaType` of the referencing entry, when known (`files[]` only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

        if let Some(cid) = cid_from_url(&image.dechunked()) {
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
    fn dechunked_resolves_cip25_string_chunking() {
        // A plain string is returned as-is; a chunked value (CIP-25's
        // 64-byte split) is concatenated back into the whole.
        let plain = PrimitiveOrList::Primitive("ipfs://bafyone".to_string());
        assert_eq!(plain.dechunked(), "ipfs://bafyone");
        let chunked = PrimitiveOrList::List(vec!["ipfs://bafy".to_string(), "rest".to_string()]);
        assert_eq!(chunked.dechunked(), "ipfs://bafyrest");
    }

    /// One real on-chain CIP-25 fixture and the CIDs `extract_cids`
    /// should recover from it.
    struct CidCase {
        asset: &'static str,
        metadata_json: &'static str,
        expected: &'static [(CidRole, &'static str)],
    }

    #[test]
    fn extracts_cids_from_real_collections() {
        // Each fixture is real on-chain metadata, chosen to span the
        // shapes the resolver must handle: both metadata variants,
        // CIDv0 + CIDv1, single + chunked `files[].src`, and a file
        // that repeats the headline image (which dedupes). Add a case
        // here when a new on-chain shape needs covering.
        let cases = [
            // Attributed variant; 3 files, files[0] repeats the image;
            // all CIDv0 (exercises CIDv0 -> CIDv1 normalisation).
            CidCase {
                asset: "Derp Bird #07490",
                metadata_json: include_str!("../resources/test/derpbird07490.json"),
                expected: &[
                    (
                        CidRole::Image,
                        "bafybeihdcqqgawor7rqnucl3fvhbmxtp7bspkf2i6k2ev6wqaihjujrqny",
                    ),
                    (
                        CidRole::File,
                        "bafybeifxjcrr6ahvauwym6qz6wqxjcxoilu7lkzeyyikzsn6a6rzlwzq34",
                    ),
                    (
                        CidRole::File,
                        "bafybeih4b5tk5zrfgrzpwogn6voq6lutg2rb3z4753zg4p2hrjof2odnem",
                    ),
                ],
            },
            // Attributed variant with a numeric trait + `swaps` array
            // + a `type` (not `mediaType`) field; 2 files, files[0]
            // repeats the image.
            CidCase {
                asset: "Pred #09193",
                metadata_json: include_str!("../resources/test/pred09193.json"),
                expected: &[
                    (
                        CidRole::Image,
                        "bafybeicuxkjhqrktemdvjqmb7ppep2capgreu76tdqaxya2h5gr2y35luq",
                    ),
                    (
                        CidRole::File,
                        "bafybeicju6krfmaqgdjczkj6pnyulmo3dlaaxrpqcys7vjgotv2tyjjb3m",
                    ),
                ],
            },
            // Flattened variant; a chunked `files[].src` (string array)
            // pointing at an mp4; CIDv1 image (normalisation passthrough).
            CidCase {
                asset: "Bankopoly #01",
                metadata_json: include_str!("../resources/test/bankcard2500.json"),
                expected: &[
                    (
                        CidRole::Image,
                        "bafybeibakqunty3xjq6ljfyfi3lde27xf3edm2tz6rm2kyb6n4aczgcjee",
                    ),
                    (
                        CidRole::File,
                        "bafybeiflyytsm445wlbv4fsayvdlnhnz34rzrxntzp74notjtrqi2jozay",
                    ),
                ],
            },
        ];

        for case in &cases {
            let metadata: AssetMetadata = serde_json::from_str(case.metadata_json)
                .unwrap_or_else(|e| panic!("{}: metadata did not decode: {e}", case.asset));
            let cids = metadata.extract_cids();
            let got: Vec<(CidRole, &str)> = cids.iter().map(|c| (c.role, c.cid.as_str())).collect();
            assert_eq!(got.as_slice(), case.expected, "{}", case.asset);
        }
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
