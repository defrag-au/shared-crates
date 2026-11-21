//! Per-request CIP-8 verification — the Slice B (CLI) auth track.
//!
//! Each API call carries `X-Stake-*` headers; the caller signs a canonical
//! payload `v1\n{method}\n{path}\n{timestamp}\n{sha256_hex(body)}` per request.
//! Stateless on the worker side — no challenge round-trip, no session token.
//!
//! See `docs/design/PLATFORM_ADMIN_AUTH.md` for the full model. The wire
//! format is documented under "Per-request signing".
//!
//! ## Layered on top of [`super::cip8`]
//!
//! The COSE_Sign1 + COSE_Key parsing and Ed25519 verification live in
//! [`super::cip8`] (also used by the Slice C challenge/verify track). This
//! module adds:
//!
//! - Canonical-payload construction
//! - Timestamp window validation (±60s)
//! - Stake-address derivation from the public key + network byte
//! - Claimed-vs-derived address sanity check (defence-in-depth)

use blake2::Blake2b;
use ed25519_dalek::{Signer as _, SigningKey};
use pallas_addresses::{Network, StakeAddress, StakePayload};
use sha2::{Digest as Sha2Digest, Sha256};

use crate::cip8;

/// Worker-clock skew tolerance for `X-Stake-Timestamp`.
pub const TIMESTAMP_WINDOW_SECS: u64 = 60;

/// Successful per-request verification.
#[derive(Debug, Clone)]
pub struct VerifiedRequest {
    /// Bech32 stake address derived from the embedded public key + the
    /// worker's `NETWORK` env var. This is what the allowlist matches against.
    pub stake_address: String,
}

/// Errors from per-request verification.
///
/// All variants should be mapped to 401/403 by the worker — none expose
/// internal state worth differentiating to the client.
#[derive(Debug, thiserror::Error)]
pub enum PerRequestError {
    #[error(
        "X-Stake-Timestamp outside ±{TIMESTAMP_WINDOW_SECS}s window (got {timestamp}, now {now_unix})"
    )]
    TimestampOutOfWindow { timestamp: u64, now_unix: u64 },
    #[error("CIP-8 signature verification failed: {0}")]
    SignatureInvalid(String),
    #[error(
        "derived stake address does not match X-Stake-Address claim (derived {derived}, claimed {claimed})"
    )]
    AddressMismatch { derived: String, claimed: String },
    #[error("could not derive stake address from public key: {0}")]
    AddressDerive(String),
}

/// Construct the canonical bytes that the per-request signature covers.
///
/// Exposed publicly so the CLI can produce identical bytes when signing.
/// The format is intentionally a plaintext newline-delimited block — easy
/// to debug, no CBOR / JSON ambiguity, version-tagged so future formats can
/// coexist via the leading `v<N>` marker.
pub fn canonical_payload(method: &str, path: &str, timestamp: u64, body: &[u8]) -> Vec<u8> {
    let body_hash = <Sha256 as Sha2Digest>::digest(body);
    let body_hash_hex = hex::encode(body_hash);
    format!("v1\n{method}\n{path}\n{timestamp}\n{body_hash_hex}").into_bytes()
}

/// Output of [`sign_per_request`] — the three hex blobs and the derived stake
/// address that the CLI ships as request headers.
#[derive(Debug, Clone)]
pub struct SignedRequest {
    /// `X-Stake-CoseSig` header value (hex of COSE_Sign1 CBOR).
    pub cose_sig_hex: String,
    /// `X-Stake-CoseKey` header value (hex of COSE_Key CBOR).
    pub cose_key_hex: String,
    /// `X-Stake-Address` header value (bech32 stake address).
    pub stake_address: String,
}

/// Sign a per-request payload from a raw Ed25519 secret key. Produces the
/// three hex blobs the worker expects in the `X-Stake-*` headers.
///
/// Used by `tools/platformadm` to sign each API call; the wire format is
/// identical to what CIP-30 wallets produce via `signData()`, which keeps the
/// worker's verifier single-codepath across CLI and browser tracks.
pub fn sign_per_request(
    method: &str,
    path: &str,
    body: &[u8],
    timestamp: u64,
    secret_key_bytes: &[u8; 32],
    network: Network,
) -> Result<SignedRequest, SignError> {
    let payload = canonical_payload(method, path, timestamp, body);
    sign_payload(&payload, secret_key_bytes, network)
}

/// Sign arbitrary bytes via CIP-8 / COSE_Sign1, producing the same hex
/// blob shape as [`sign_per_request`]. Used by the challenge/verify flow
/// (CLI counterpart of the browser portal's `wallet.signData()`).
///
/// The caller supplies the exact bytes to sign — same bytes the verifier
/// will check against the envelope's payload field.
pub fn sign_payload(
    payload: &[u8],
    secret_key_bytes: &[u8; 32],
    network: Network,
) -> Result<SignedRequest, SignError> {
    // Materialise the Ed25519 keypair + derive the stake credential.
    let signing_key = SigningKey::from_bytes(secret_key_bytes);
    let public_key_bytes = signing_key.verifying_key().to_bytes();

    let mut hasher = Blake2b::<blake2::digest::consts::U28>::new();
    hasher.update(public_key_bytes);
    let credential_array: [u8; 28] = hasher.finalize().into();
    let credential_hash = pallas_crypto::hash::Hash::<28>::from(credential_array);
    let stake_addr = StakeAddress::new(network, StakePayload::Stake(credential_hash));
    let stake_address_bech32 = stake_addr
        .to_bech32()
        .map_err(|e| SignError::Address(e.to_string()))?;
    let stake_address_bytes = stake_addr.to_vec();

    // Build protected_header CBOR map: {1: -8 (EdDSA), "address": <addr bytes>}
    // CIP-8 requires the address in the protected header. Verifier treats the
    // bytes as opaque (uses them in Sig_Structure reconstruction), so this is
    // both compatibility ballast and a binding from sig to address.
    let protected_map = ciborium::Value::Map(vec![
        (
            ciborium::Value::Integer(1.into()),
            ciborium::Value::Integer((-8).into()),
        ),
        (
            ciborium::Value::Text("address".into()),
            ciborium::Value::Bytes(stake_address_bytes),
        ),
    ]);
    let mut protected_bytes = Vec::new();
    ciborium::into_writer(&protected_map, &mut protected_bytes)
        .map_err(|e| SignError::Cbor(format!("protected header: {e}")))?;

    // Sig_Structure = ["Signature1", protected, b"", payload]
    let sig_structure = ciborium::Value::Array(vec![
        ciborium::Value::Text("Signature1".into()),
        ciborium::Value::Bytes(protected_bytes.clone()),
        ciborium::Value::Bytes(vec![]),
        ciborium::Value::Bytes(payload.to_vec()),
    ]);
    let mut sig_structure_bytes = Vec::new();
    ciborium::into_writer(&sig_structure, &mut sig_structure_bytes)
        .map_err(|e| SignError::Cbor(format!("sig_structure: {e}")))?;

    // Ed25519 sign the Sig_Structure bytes.
    let signature = signing_key.sign(&sig_structure_bytes);

    // COSE_Sign1 = [protected_bytes, {}, payload, signature]
    let cose_sign1 = ciborium::Value::Array(vec![
        ciborium::Value::Bytes(protected_bytes),
        ciborium::Value::Map(vec![]),
        ciborium::Value::Bytes(payload.to_vec()),
        ciborium::Value::Bytes(signature.to_bytes().to_vec()),
    ]);
    let mut cose_sign1_bytes = Vec::new();
    ciborium::into_writer(&cose_sign1, &mut cose_sign1_bytes)
        .map_err(|e| SignError::Cbor(format!("cose_sign1: {e}")))?;

    // COSE_Key = {1: 1 (OKP), 3: -8 (EdDSA), -1: 6 (Ed25519), -2: <pubkey bytes>}
    let cose_key = ciborium::Value::Map(vec![
        (
            ciborium::Value::Integer(1.into()),
            ciborium::Value::Integer(1.into()),
        ),
        (
            ciborium::Value::Integer(3.into()),
            ciborium::Value::Integer((-8).into()),
        ),
        (
            ciborium::Value::Integer((-1).into()),
            ciborium::Value::Integer(6.into()),
        ),
        (
            ciborium::Value::Integer((-2).into()),
            ciborium::Value::Bytes(public_key_bytes.to_vec()),
        ),
    ]);
    let mut cose_key_bytes = Vec::new();
    ciborium::into_writer(&cose_key, &mut cose_key_bytes)
        .map_err(|e| SignError::Cbor(format!("cose_key: {e}")))?;

    Ok(SignedRequest {
        cose_sig_hex: hex::encode(cose_sign1_bytes),
        cose_key_hex: hex::encode(cose_key_bytes),
        stake_address: stake_address_bech32,
    })
}

/// Errors from [`sign_per_request`].
#[derive(Debug, thiserror::Error)]
pub enum SignError {
    #[error("address derive: {0}")]
    Address(String),
    #[error("CBOR encode: {0}")]
    Cbor(String),
}

/// Pull the embedded payload bytes from a COSE_Sign1 envelope (hex-encoded).
///
/// Used by [`verify_per_request`] to check that what the caller said they
/// signed is what they actually signed — defending against the
/// inlined-payload case in `cip8::verify_signature`.
fn extract_cose_sign1_payload(cose_sig_hex: &str) -> Result<Vec<u8>, String> {
    let bytes = hex::decode(cose_sig_hex).map_err(|e| format!("hex: {e}"))?;
    let value: ciborium::Value =
        ciborium::from_reader(bytes.as_slice()).map_err(|e| format!("CBOR: {e}"))?;
    let array = match value {
        ciborium::Value::Array(a) => a,
        _ => return Err("COSE_Sign1 must be a CBOR array".into()),
    };
    if array.len() != 4 {
        return Err(format!(
            "COSE_Sign1 expects 4 elements, got {}",
            array.len()
        ));
    }
    match &array[2] {
        ciborium::Value::Bytes(b) => Ok(b.clone()),
        ciborium::Value::Null => {
            Err("COSE_Sign1 payload is null (detached); per-request requires inline payload".into())
        }
        _ => Err("COSE_Sign1 payload field must be bytes".into()),
    }
}

/// Verify a per-request CIP-8 signature end-to-end.
///
/// On success returns the verified bech32 stake address — the caller hands
/// this to the allowlist (`api_auth::allowlist::tier_for`) to
/// determine tier and gate access. On failure, all `PerRequestError` variants
/// should be treated as 401/403 — don't leak distinguishers to the client.
#[allow(clippy::too_many_arguments)]
pub fn verify_per_request(
    method: &str,
    path: &str,
    body: &[u8],
    timestamp: u64,
    now_unix: u64,
    cose_sig_hex: &str,
    cose_key_hex: &str,
    claimed_stake_address: &str,
    network: Network,
) -> Result<VerifiedRequest, PerRequestError> {
    // 1. Timestamp window check — prevents replaying a captured signature
    //    indefinitely.
    let drift = now_unix.abs_diff(timestamp);
    if drift > TIMESTAMP_WINDOW_SECS {
        return Err(PerRequestError::TimestampOutOfWindow {
            timestamp,
            now_unix,
        });
    }

    // 2. Reconstruct the canonical payload. The CLI signed identical bytes.
    let payload = canonical_payload(method, path, timestamp, body);
    let payload_str =
        std::str::from_utf8(&payload).expect("canonical_payload produces ASCII-only bytes");

    // 3. Defence-in-depth: the COSE_Sign1 envelope embeds the actual signed
    //    payload. `cip8::verify_signature` uses that embedded payload when
    //    reconstructing Sig_Structure (good for the challenge/verify track,
    //    where the envelope's payload is the nonce-bearing message). For
    //    per-request, we need the EMBEDDED payload to EQUAL the reconstructed
    //    canonical payload — otherwise a tampered (method, path, body, ts)
    //    on the caller side gives an envelope whose signature still verifies
    //    against the original payload but doesn't bind to what we received.
    //    Extract the envelope payload and compare verbatim before trusting
    //    the crypto.
    let envelope_payload =
        extract_cose_sign1_payload(cose_sig_hex).map_err(PerRequestError::SignatureInvalid)?;
    if envelope_payload != payload {
        return Err(PerRequestError::SignatureInvalid(
            "envelope payload does not match request's canonical payload".into(),
        ));
    }

    // 4. CIP-8 verification (low-level, shared with Slice C).
    let ok = cip8::verify_signature(cose_sig_hex, cose_key_hex, payload_str)
        .map_err(PerRequestError::SignatureInvalid)?;
    if !ok {
        return Err(PerRequestError::SignatureInvalid(
            "signature did not verify".into(),
        ));
    }

    // 4. Derive the stake address from the public key + network byte.
    //    Shared with the challenge and session tracks — see
    //    `cip8::derive_stake_address`.
    let derived = cip8::derive_stake_address(cose_key_hex, network)
        .map_err(PerRequestError::AddressDerive)?;

    // 5. Sanity check the claim. Catches header mismatches, copy-paste errors,
    //    and mistakenly-targeted requests against the wrong network. The
    //    cryptographic ground truth is the derived address; the claim is just
    //    a "did the operator intend what they signed?" check.
    if derived != claimed_stake_address {
        return Err(PerRequestError::AddressMismatch {
            derived,
            claimed: claimed_stake_address.to_string(),
        });
    }

    Ok(VerifiedRequest {
        stake_address: derived,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_payload_is_stable() {
        let p1 = canonical_payload("POST", "/api/clients", 1717059600, b"{\"slug\":\"foo\"}");
        let p2 = canonical_payload("POST", "/api/clients", 1717059600, b"{\"slug\":\"foo\"}");
        assert_eq!(p1, p2);
    }

    #[test]
    fn canonical_payload_differs_on_method() {
        let p1 = canonical_payload("POST", "/api/clients", 1717059600, b"");
        let p2 = canonical_payload("GET", "/api/clients", 1717059600, b"");
        assert_ne!(p1, p2);
    }

    #[test]
    fn canonical_payload_differs_on_path() {
        let p1 = canonical_payload("POST", "/api/clients", 1717059600, b"");
        let p2 = canonical_payload("POST", "/api/wallets", 1717059600, b"");
        assert_ne!(p1, p2);
    }

    #[test]
    fn canonical_payload_differs_on_body() {
        let p1 = canonical_payload("POST", "/api/clients", 1717059600, b"{\"slug\":\"foo\"}");
        let p2 = canonical_payload("POST", "/api/clients", 1717059600, b"{\"slug\":\"bar\"}");
        assert_ne!(p1, p2);
    }

    #[test]
    fn canonical_payload_format_is_v1() {
        let p = canonical_payload("GET", "/health", 1234, b"");
        let s = std::str::from_utf8(&p).unwrap();
        assert!(s.starts_with("v1\n"));
        assert!(s.contains("\nGET\n"));
        assert!(s.contains("\n/health\n"));
        assert!(s.contains("\n1234\n"));
        // Empty body's sha256 is e3b0c4...
        assert!(s.ends_with("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));
    }

    #[test]
    fn timestamp_window_rejects_old_signature() {
        let err = verify_per_request(
            "GET",
            "/health",
            b"",
            1717059600,      // signed 60s + 1 ago
            1717059600 + 61, // now
            "00",
            "00",
            "stake1u...",
            Network::Mainnet,
        )
        .unwrap_err();
        assert!(matches!(err, PerRequestError::TimestampOutOfWindow { .. }));
    }

    /// The gold-standard test: sign + verify round-trip with a deterministic
    /// keypair, on both networks. If this passes we know the wire format is
    /// internally consistent — any breakage between sign and verify shows up
    /// here before it bites in production.
    #[test]
    fn sign_then_verify_roundtrip_testnet() {
        let secret = [42u8; 32];
        let now = 1717059600u64;

        let signed = sign_per_request(
            "POST",
            "/api/clients",
            b"{\"slug\":\"yepple\"}",
            now,
            &secret,
            Network::Testnet,
        )
        .unwrap();

        let verified = verify_per_request(
            "POST",
            "/api/clients",
            b"{\"slug\":\"yepple\"}",
            now,
            now, // same time = no drift
            &signed.cose_sig_hex,
            &signed.cose_key_hex,
            &signed.stake_address,
            Network::Testnet,
        )
        .unwrap();

        assert_eq!(verified.stake_address, signed.stake_address);
        assert!(signed.stake_address.starts_with("stake_test"));
    }

    #[test]
    fn sign_then_verify_roundtrip_mainnet() {
        let secret = [7u8; 32];
        let now = 1717059600u64;

        let signed =
            sign_per_request("GET", "/api/whoami", b"", now, &secret, Network::Mainnet).unwrap();
        let verified = verify_per_request(
            "GET",
            "/api/whoami",
            b"",
            now,
            now + 30, // 30s drift — within window
            &signed.cose_sig_hex,
            &signed.cose_key_hex,
            &signed.stake_address,
            Network::Mainnet,
        )
        .unwrap();

        assert_eq!(verified.stake_address, signed.stake_address);
        assert!(verified.stake_address.starts_with("stake1"));
        // Mainnet should NOT match preprod prefix even on the same key.
        assert!(!verified.stake_address.starts_with("stake_test"));
    }

    #[test]
    fn cross_network_rejected_by_address_mismatch() {
        // Sign on mainnet, verify against testnet using the same key — the
        // derived testnet address won't match the mainnet `claimed`.
        let secret = [99u8; 32];
        let now = 1717059600u64;
        let signed = sign_per_request("GET", "/h", b"", now, &secret, Network::Mainnet).unwrap();

        let err = verify_per_request(
            "GET",
            "/h",
            b"",
            now,
            now,
            &signed.cose_sig_hex,
            &signed.cose_key_hex,
            &signed.stake_address, // mainnet bech32 claim
            Network::Testnet,      // worker is on preprod
        )
        .unwrap_err();
        assert!(matches!(err, PerRequestError::AddressMismatch { .. }));
    }

    #[test]
    fn tampered_body_fails_verification() {
        let secret = [11u8; 32];
        let now = 1717059600u64;
        let signed = sign_per_request(
            "POST",
            "/api/clients",
            b"{\"slug\":\"foo\"}",
            now,
            &secret,
            Network::Testnet,
        )
        .unwrap();

        // Verify with DIFFERENT body — canonical_payload no longer matches.
        let err = verify_per_request(
            "POST",
            "/api/clients",
            b"{\"slug\":\"bar\"}", // tampered
            now,
            now,
            &signed.cose_sig_hex,
            &signed.cose_key_hex,
            &signed.stake_address,
            Network::Testnet,
        )
        .unwrap_err();
        assert!(matches!(err, PerRequestError::SignatureInvalid(_)));
    }

    #[test]
    fn tampered_path_fails_verification() {
        let secret = [22u8; 32];
        let now = 1717059600u64;
        let signed =
            sign_per_request("GET", "/api/whoami", b"", now, &secret, Network::Testnet).unwrap();
        let err = verify_per_request(
            "GET",
            "/api/clients", // different path
            b"",
            now,
            now,
            &signed.cose_sig_hex,
            &signed.cose_key_hex,
            &signed.stake_address,
            Network::Testnet,
        )
        .unwrap_err();
        assert!(matches!(err, PerRequestError::SignatureInvalid(_)));
    }

    #[test]
    fn timestamp_window_rejects_future_signature() {
        let err = verify_per_request(
            "GET",
            "/health",
            b"",
            1717059600 + 61, // signed in the future
            1717059600,      // now
            "00",
            "00",
            "stake1u...",
            Network::Mainnet,
        )
        .unwrap_err();
        assert!(matches!(err, PerRequestError::TimestampOutOfWindow { .. }));
    }
}
