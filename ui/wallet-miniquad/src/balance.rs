//! CIP-30 balance CBOR → $handle extraction.
//!
//! `wallet.getBalance()` returns a hex-encoded CBOR `Value`:
//!
//!   Value := coin                          ; uint only (lovelace, no assets)
//!          / [coin, multiasset]            ; lovelace + multiasset map
//!   multiasset := { policy_id => { asset_name => quantity } }
//!
//! Handle extraction walks the multiasset map looking for the ADA Handle
//! policy (`f0ff48bb…b9a` on mainnet); each asset name under that policy
//! is a CIP-67-prefixed (CIP-68 user/ref token) or unprefixed (legacy
//! CIP-25) UTF-8 handle string. We strip the 4-byte CIP-67 label if
//! present and decode the rest.
//!
//! Returns the first valid handle found (sorted by map iteration order;
//! pallas-codec uses BTreeMap so it's deterministic). `None` for wallets
//! without a handle, malformed CBOR, or testnet wallets (the policy ID
//! differs / handles rarely exist there).

use pallas_codec::minicbor::{self, decode::Decoder};

/// Mainnet ADA Handle policy ID — 28 raw bytes.
const HANDLE_POLICY: [u8; 28] = [
    0xf0, 0xff, 0x48, 0xbb, 0xb7, 0xbb, 0xe9, 0xd5, 0x9a, 0x40, 0xf1, 0xce, 0x90, 0xe9, 0xe9, 0xd0,
    0xff, 0x50, 0x02, 0xec, 0x48, 0xf2, 0x32, 0xb4, 0x9c, 0xa0, 0xfb, 0x9a,
];

pub fn extract_handle(balance_hex: &str) -> Option<String> {
    let bytes = hex_decode(balance_hex)?;
    let mut d = Decoder::new(&bytes);

    // Branch on top-level shape. minicbor's `datatype()` peeks without
    // consuming, so we can decide between "just a uint" and "array".
    use minicbor::data::Type;
    match d.datatype().ok()? {
        Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::Int => {
            // No multiasset → no handle possible.
            None
        }
        Type::Array | Type::ArrayIndef => {
            d.array().ok()?;
            // First element is lovelace — skip it without parsing.
            d.skip().ok()?;
            // Second element is the multiasset map.
            scan_multiasset(&mut d)
        }
        _ => None,
    }
}

fn scan_multiasset(d: &mut Decoder<'_>) -> Option<String> {
    let len = d.map().ok()?;
    let count = len.unwrap_or(0);
    for _ in 0..count {
        let policy_id = d.bytes().ok()?;
        if policy_id == HANDLE_POLICY {
            return scan_assets(d);
        }
        // Skip the inner asset map without parsing.
        skip_one(d)?;
    }
    None
}

fn scan_assets(d: &mut Decoder<'_>) -> Option<String> {
    let len = d.map().ok()?;
    let count = len.unwrap_or(0);
    for _ in 0..count {
        let asset_name = d.bytes().ok()?;
        d.skip().ok()?; // quantity — we don't care

        let cleaned = strip_cip67_label(asset_name);
        if let Ok(s) = std::str::from_utf8(cleaned) {
            if !s.is_empty() && s.chars().all(|c| !c.is_control()) {
                return Some(format!("${s}"));
            }
        }
    }
    None
}

/// CIP-67 label: 4 bytes laid out as
///   `[0x00, label_high(8b), label_low(4b)|crc_high(4b), crc_low(4b)|0x0]`
/// — so a leading full 0x00 byte and a trailing low nibble of zero
/// envelope the label/CRC payload. Strip if present; covers both
/// CIP-68 user tokens (label 222 = `0x000de140`) and ref tokens
/// (label 100 = `0x000643b0`). Legacy CIP-25 handles have no prefix
/// and pass through unchanged.
fn strip_cip67_label(name: &[u8]) -> &[u8] {
    if name.len() >= 4 && name[0] == 0x00 && (name[3] & 0x0F) == 0x00 {
        &name[4..]
    } else {
        name
    }
}

fn skip_one(d: &mut Decoder<'_>) -> Option<()> {
    d.skip().ok()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for chunk in bytes.chunks(2) {
        let high = hex_nibble(chunk[0])?;
        let low = hex_nibble(chunk[1])?;
        out.push((high << 4) | low);
    }
    Some(out)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(10 + b - b'a'),
        b'A'..=b'F' => Some(10 + b - b'A'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lovelace_only_returns_none() {
        // CBOR for just `5000000` (5 ADA) — a single uint.
        let hex = "1a004c4b40";
        assert_eq!(extract_handle(hex), None);
    }

    #[test]
    fn empty_or_malformed_returns_none() {
        assert_eq!(extract_handle(""), None);
        assert_eq!(extract_handle("zzz"), None);
        assert_eq!(extract_handle("00"), None); // not a valid Value
    }

    #[test]
    fn strip_cip67_handles_prefix() {
        // 0x000643b0 + "alien" — CIP-68 user token "alien"
        let raw = [0x00, 0x06, 0x43, 0xb0, b'a', b'l', b'i', b'e', b'n'];
        assert_eq!(strip_cip67_label(&raw), b"alien");
    }

    #[test]
    fn strip_cip67_passes_legacy_through() {
        // Legacy CIP-25 handle: no prefix
        let raw = b"alien";
        assert_eq!(strip_cip67_label(raw), b"alien");
    }

    #[test]
    fn strip_cip67_passes_non_label_prefix_through() {
        // Looks similar but doesn't match the 0x00 ... 0x00 envelope
        let raw = [0xab, 0x06, 0x43, 0xb0, b'x'];
        assert_eq!(strip_cip67_label(&raw), &raw[..]);
    }

    #[test]
    fn extracts_handle_from_balance_with_assets() {
        // Hand-crafted CBOR:
        //   [5000000, { f0ff…b9a: { "alien": 1 } }]
        // Asset name is legacy CIP-25 (no prefix), 5 bytes "alien"
        let mut bytes = vec![
            0x82, // array of 2
            0x1a, 0x00, 0x4c, 0x4b, 0x40, // u32 5000000
            0xa1, // map of 1
            0x58, 0x1c, // bytes of length 28 (policy id)
        ];
        bytes.extend_from_slice(&HANDLE_POLICY);
        bytes.extend_from_slice(&[
            0xa1, // inner map of 1
            0x45, b'a', b'l', b'i', b'e', b'n', // bytes "alien"
            0x01, // u8 quantity 1
        ]);
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(extract_handle(&hex), Some("$alien".to_string()));
    }

    #[test]
    fn skips_unrelated_policies() {
        // [coin, { other_policy: {...}, handle_policy: { "found": 1 } }]
        // Use a 28-byte all-zeros policy as the "other"
        let mut bytes = vec![0x82, 0x1a, 0x00, 0x4c, 0x4b, 0x40, 0xa2, 0x58, 0x1c];
        bytes.extend_from_slice(&[0u8; 28]); // other policy
        bytes.extend_from_slice(&[0xa1, 0x44, b't', b'o', b'k', b'n', 0x05, 0x58, 0x1c]);
        bytes.extend_from_slice(&HANDLE_POLICY);
        bytes.extend_from_slice(&[0xa1, 0x45, b'f', b'o', b'u', b'n', b'd', 0x01]);
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(extract_handle(&hex), Some("$found".to_string()));
    }
}
