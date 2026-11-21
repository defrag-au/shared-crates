//! CIP-8 COSE_Sign1 signature verification and PKH derivation.
//!
//! Handles both software wallets (proper Sig_Structure) and hardware wallets
//! (raw message fallback). Uses `ciborium` for CBOR and `ed25519-dalek` for Ed25519.

use blake2::{Blake2b, Digest as _};
use ed25519_dalek::{Signature, VerifyingKey};
use pallas_addresses::{Network, StakeAddress, StakePayload};

/// Verify a CIP-8 COSE_Sign1 signature **against a specific message**.
///
/// Tries the proper Sig_Structure path first (any wallet that builds one), then
/// falls back to raw verification for a signer that attests to bytes directly —
/// a bare 64-byte signature, or a hardware wallet's, which is over
/// `Blake2b-224(payload)` rather than the payload itself (CIP-8's E1/E3: device
/// memory and device display).
///
/// - `signature_hex`: hex-encoded COSE_Sign1 or raw 64-byte signature
/// - `public_key_hex`: hex-encoded COSE_Key or raw 32-byte Ed25519 public key
/// - `expected_message`: the plaintext message that must have been signed
///
/// `expected_message` is **binding on every path**: each form below is the
/// message itself or a collision-resistant, deterministic function of it, so a
/// `true` return still means "this key signed exactly this message" — which is
/// what makes a challenge nonce meaningful. The hashed form is not a fallback
/// *guess*: it is CIP-8's own payload encoding (see [`signed_payload`]), and
/// where an envelope declares it, the declaration is what selects it.
///
/// It does **not** bind the signature to a claimed address — the key is the
/// only identity here. Callers that accept an address from the client must
/// derive it from `public_key_hex` (see [`derive_payment_pkh`]) and compare;
/// `session::consume_challenge` and `per_request::verify_per_request` both do.
pub fn verify_signature(
    signature_hex: &str,
    public_key_hex: &str,
    expected_message: &str,
) -> Result<bool, String> {
    let signature_bytes =
        hex::decode(signature_hex).map_err(|e| format!("Invalid signature hex: {e}"))?;
    let public_key_bytes =
        hex::decode(public_key_hex).map_err(|e| format!("Invalid public key hex: {e}"))?;

    // Extract raw 32-byte Ed25519 key from COSE_Key envelope or use directly
    let key_bytes: Vec<u8> = if public_key_bytes.len() == 32 {
        public_key_bytes
    } else {
        extract_key_from_cose_key(&public_key_bytes)
            .map_err(|e| format!("Failed to parse COSE_Key: {e}"))?
    };

    let key_array: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| "Public key must be 32 bytes".to_string())?;
    let verifying_key =
        VerifyingKey::from_bytes(&key_array).map_err(|e| format!("Invalid Ed25519 key: {e}"))?;

    // Try CIP-8 Sig_Structure verification (preferred — software wallets)
    if signature_bytes.len() > 64
        && let Ok((sig_structure, sig_bytes)) =
            extract_sig_structure(&signature_bytes, expected_message.as_bytes())
        && let Ok(array) = <[u8; 64]>::try_from(sig_bytes.as_slice())
    {
        let sig = Signature::from_bytes(&array);
        if verifying_key.verify_strict(&sig_structure, &sig).is_ok() {
            return Ok(true);
        }
    }

    // Fallback: try to extract raw 64-byte signature and verify against message bytes
    let raw_sig_bytes: Vec<u8> = if signature_bytes.len() == 64 {
        signature_bytes.clone()
    } else {
        extract_sig_from_cose_sign1(&signature_bytes)
            .map_err(|e| format!("Failed to parse COSE_Sign1: {e}"))?
    };

    if let Ok(array) = <[u8; 64]>::try_from(raw_sig_bytes.as_slice()) {
        let sig = Signature::from_bytes(&array);
        if verifying_key
            .verify_strict(expected_message.as_bytes(), &sig)
            .is_ok()
        {
            return Ok(true);
        }
        // A hardware wallet cannot build a Sig_Structure, so it signs the
        // payload's `Blake2b-224` on its own and the wallet hands that
        // signature over inside the envelope. Still a function of the expected
        // message, so the binding holds; see this function's docs.
        let hashed = blake2b224(expected_message.as_bytes());
        if verifying_key.verify_strict(hashed.as_slice(), &sig).is_ok() {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Derive the bech32 **stake address** that a hex-encoded COSE_Key belongs to.
///
/// This is the binding [`verify_signature`] deliberately doesn't do. A verified
/// signature proves "this key signed this message"; it says nothing about the
/// address a client *claims* to be. Deriving the address from the key and
/// comparing makes the derived value the ground truth and the client's claim a
/// sanity check — which is the only ordering that can't be spoofed.
///
/// The credential is `blake2b224(pubkey)`, wrapped with a network byte. Note
/// that `derive_payment_pkh`'s name is a historical misnomer: the same hash IS
/// the stake credential when the key is a stake key, which is what CIP-30
/// `signData` over `getRewardAddresses()` produces.
pub fn derive_stake_address(cose_key_hex: &str, network: Network) -> Result<String, String> {
    let credential_hex = derive_payment_pkh(cose_key_hex)?;
    let credential_bytes =
        hex::decode(&credential_hex).map_err(|e| format!("credential hex: {e}"))?;

    let credential_array: [u8; 28] = credential_bytes.as_slice().try_into().map_err(|_| {
        format!(
            "credential must be 28 bytes, got {}",
            credential_bytes.len()
        )
    })?;

    let credential_hash = pallas_crypto::hash::Hash::<28>::from(credential_array);
    StakeAddress::new(network, StakePayload::Stake(credential_hash))
        .to_bech32()
        .map_err(|e| format!("bech32: {e}"))
}

/// Derive the payment key hash from a hex-encoded Ed25519 public key.
///
/// PKH = Blake2b-224(raw_ed25519_key_bytes)
pub fn derive_payment_pkh(public_key_hex: &str) -> Result<String, String> {
    let key_bytes = hex::decode(public_key_hex).map_err(|e| format!("Invalid hex: {e}"))?;

    let raw_key: Vec<u8> = if key_bytes.len() == 32 {
        key_bytes
    } else {
        extract_key_from_cose_key(&key_bytes)?
    };

    let mut hasher = Blake2b::<blake2::digest::consts::U28>::new();
    hasher.update(&raw_key);
    let hash = hasher.finalize();

    Ok(hex::encode(hash))
}

// ─── COSE parsing internals ──────────────────────────────────────────────────

/// Extract Ed25519 public key bytes from a COSE_Key structure.
///
/// COSE_Key for Ed25519: `{1: 1 (OKP), 3: -8 (EdDSA), -1: 6 (Ed25519), -2: key_bytes}`
fn extract_key_from_cose_key(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let value: ciborium::Value =
        ciborium::from_reader(bytes).map_err(|e| format!("CBOR decode failed: {e}"))?;

    let map = match value {
        ciborium::Value::Map(m) => m,
        _ => return Err("COSE_Key must be a CBOR map".into()),
    };

    for (k, v) in &map {
        let is_minus_two = matches!(k, ciborium::Value::Integer(i) if i128::from(*i) == -2);
        if is_minus_two && let ciborium::Value::Bytes(b) = v {
            return Ok(b.clone());
        }
    }

    Err("Key -2 not found in COSE_Key map".into())
}

/// Extract (Sig_Structure bytes, signature bytes) from a COSE_Sign1 envelope.
///
/// Reconstructs the Sig_Structure for verification: `["Signature1", prot_hdr, b"", payload]`
///
/// The envelope's payload is **bound to `expected_payload`**: it must be that
/// message, or the `Blake2b-224` hash of it that a hardware wallet signs in its
/// place (see [`signed_payload`]). Anything else is an error rather than a
/// signature over the attacker's choice of message. Without that check, a
/// signature that genuinely attests to message A verifies when the caller asks
/// whether it attests to B — which makes a challenge nonce decorative and lets
/// any CIP-8 signature harvested from another dapp satisfy our flow. A `Null`
/// payload is the detached case, where the caller's expected bytes are the only
/// source.
fn extract_sig_structure(
    cose_sign1_bytes: &[u8],
    expected_payload: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let value: ciborium::Value =
        ciborium::from_reader(cose_sign1_bytes).map_err(|e| format!("CBOR decode: {e}"))?;

    let array = match value {
        ciborium::Value::Array(a) => a,
        _ => return Err("COSE_Sign1 must be a CBOR array".into()),
    };

    if array.len() != 4 {
        return Err(format!(
            "COSE_Sign1 must have 4 elements, got {}",
            array.len()
        ));
    }

    let protected_header = match &array[0] {
        ciborium::Value::Bytes(b) => b.clone(),
        _ => return Err("Protected header must be bytes".into()),
    };

    let attached = match &array[2] {
        ciborium::Value::Bytes(b) => Some(b.as_slice()),
        ciborium::Value::Null => None,
        _ => return Err("Payload must be bytes or null".into()),
    };

    let sig = match &array[3] {
        ciborium::Value::Bytes(b) => b.clone(),
        _ => return Err("Signature must be bytes".into()),
    };

    let payload = signed_payload(expected_payload, attached, &protected_header, &array[1])?;

    let sig_structure = ciborium::Value::Array(vec![
        ciborium::Value::Text("Signature1".into()),
        ciborium::Value::Bytes(protected_header),
        ciborium::Value::Bytes(vec![]),
        ciborium::Value::Bytes(payload),
    ]);

    let mut sig_structure_bytes = Vec::new();
    ciborium::into_writer(&sig_structure, &mut sig_structure_bytes)
        .map_err(|e| format!("CBOR encode Sig_Structure: {e}"))?;

    Ok((sig_structure_bytes, sig))
}

/// `Blake2b-224`, the hash CIP-8 names for a payload a signer cannot attest to
/// directly. Same construction as [`derive_payment_pkh`]'s.
fn blake2b224(bytes: &[u8]) -> [u8; 28] {
    let mut hasher = Blake2b::<blake2::digest::consts::U28>::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Whether the envelope declares that the signer attested to
/// `Blake2b-224(payload)` rather than the payload itself.
///
/// CIP-8 puts the `hashed` flag in the **unprotected** header; the wallets in
/// this ecosystem have also been seen to put it in the protected one, which is
/// the copy the signature covers. Both are honoured. Trusting the unprotected
/// copy costs nothing: it only selects which bytes the signature is checked
/// against, and it still has to verify there — it can never widen *which
/// message* a signature is good for.
fn declares_hashed(protected_header: &[u8], unprotected: &ciborium::Value) -> bool {
    fn flagged(map: &ciborium::Value) -> bool {
        match map {
            ciborium::Value::Map(entries) => entries.iter().any(|(k, v)| {
                matches!(k, ciborium::Value::Text(t) if t == "hashed")
                    && matches!(v, ciborium::Value::Bool(true))
            }),
            _ => false,
        }
    }

    let in_protected = ciborium::from_reader::<ciborium::Value, _>(protected_header)
        .map(|value| flagged(&value))
        .unwrap_or(false);

    in_protected || flagged(unprotected)
}

/// The bytes the signer put in the Sig_Structure's payload slot.
///
/// Three shapes reach us, each a function of the message the caller expects —
/// which is what keeps the binding intact:
///
/// - the payload itself: every software wallet;
/// - the payload's `Blake2b-224`, declared by the `hashed` header: how a
///   hardware wallet attests, since it signs a fixed-size hash rather than an
///   arbitrary-length message (CIP-8's E1/E3);
/// - the hash **attached as the payload**, flag or no flag, which is how the
///   wallets in this ecosystem have been seen to present it.
///
/// Anything else is a binding failure: the signature would be over bytes the
/// caller never asked about.
fn signed_payload(
    expected: &[u8],
    attached: Option<&[u8]>,
    protected_header: &[u8],
    unprotected: &ciborium::Value,
) -> Result<Vec<u8>, String> {
    let hashed = blake2b224(expected);

    match attached {
        Some(payload) if payload == expected => {
            Ok(if declares_hashed(protected_header, unprotected) {
                hashed.to_vec()
            } else {
                payload.to_vec()
            })
        }
        Some(payload) if payload == hashed.as_slice() => Ok(payload.to_vec()),
        None => Ok(if declares_hashed(protected_header, unprotected) {
            hashed.to_vec()
        } else {
            expected.to_vec()
        }),
        Some(_) => Err(
            "envelope payload does not match the expected message — the signature \
             attests to different bytes than the caller is checking"
                .into(),
        ),
    }
}

/// Extract the raw 64-byte signature from a COSE_Sign1 structure.
fn extract_sig_from_cose_sign1(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let value: ciborium::Value =
        ciborium::from_reader(bytes).map_err(|e| format!("CBOR decode: {e}"))?;

    let array = match value {
        ciborium::Value::Array(a) => a,
        _ => return Err("COSE_Sign1 must be a CBOR array".into()),
    };

    if array.len() != 4 {
        return Err("COSE_Sign1 must have 4 elements".into());
    }

    match &array[3] {
        ciborium::Value::Bytes(b) => Ok(b.clone()),
        _ => Err("Signature element must be bytes".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::per_request::sign_payload;
    use pallas_addresses::Network;

    /// Deterministic keypairs — matches the `[42u8; 32]` convention in
    /// `per_request`'s round-trip tests.
    const SECRET_A: [u8; 32] = [42u8; 32];
    const SECRET_B: [u8; 32] = [7u8; 32];

    fn sign(secret: &[u8; 32], message: &str) -> (String, String, String) {
        let signed = sign_payload(message.as_bytes(), secret, Network::Mainnet)
            .expect("sign_payload should not fail on a well-formed payload");
        (
            signed.cose_sig_hex,
            signed.cose_key_hex,
            signed.stake_address,
        )
    }

    // ── Baseline: the happy path works ──────────────────────────────────────

    #[test]
    fn valid_signature_over_expected_message_verifies() {
        let (sig, key, _) = sign(&SECRET_A, "Link wallet\n\nNonce: abc123");
        assert!(
            verify_signature(&sig, &key, "Link wallet\n\nNonce: abc123").unwrap(),
            "a signature over exactly the expected message must verify"
        );
    }

    #[test]
    fn signature_from_a_different_key_is_rejected() {
        let (sig, _, _) = sign(&SECRET_A, "Link wallet\n\nNonce: abc123");
        let (_, key_b, _) = sign(&SECRET_B, "Link wallet\n\nNonce: abc123");
        assert!(
            !verify_signature(&sig, &key_b, "Link wallet\n\nNonce: abc123").unwrap(),
            "a signature must not verify against an unrelated public key"
        );
    }

    // ── The payload-binding gap (§4.4 of SOCIAL_WALLET_LINK_DESIGN) ─────────

    /// **This is the replay vulnerability.**
    ///
    /// `extract_sig_structure` rebuilds the Sig_Structure from the payload
    /// embedded in the *envelope*, and only falls back to `expected_message`
    /// when that payload is CBOR `Null`. So a signature that genuinely attests
    /// to message A verifies when the caller asks "does this attest to B?".
    ///
    /// Concretely: an attacker holding any valid CIP-8 signature from a
    /// victim's wallet — obtained from any other dapp's sign-in flow — passes
    /// our nonce challenge without ever seeing our nonce.
    #[test]
    fn signature_over_a_different_message_is_rejected() {
        let (sig, key, _) = sign(&SECRET_A, "Sign in to Some Other Dapp");

        assert!(
            !verify_signature(&sig, &key, "Link wallet\n\nNonce: abc123").unwrap(),
            "a signature over a DIFFERENT message must not verify against our \
             expected message — the nonce is never actually checked otherwise"
        );
    }

    /// The same defect stated as the property we actually depend on: the nonce
    /// in the expected message has to be load-bearing.
    #[test]
    fn signature_bound_to_one_nonce_does_not_verify_against_another() {
        let (sig, key, _) = sign(&SECRET_A, "Link wallet\n\nNonce: aaaaaaaa");

        assert!(
            !verify_signature(&sig, &key, "Link wallet\n\nNonce: bbbbbbbb").unwrap(),
            "swapping only the nonce must invalidate the signature"
        );
    }

    // ── Hardware wallets: CIP-8's hashed payload form ────────────────────────

    /// A COSE_Key for an Ed25519 public key — the shape a wallet returns beside
    /// the signature.
    fn cose_key_hex(public_key: &[u8; 32]) -> String {
        let key = ciborium::Value::Map(vec![
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
                ciborium::Value::Bytes(public_key.to_vec()),
            ),
        ]);
        let mut bytes = Vec::new();
        ciborium::into_writer(&key, &mut bytes).unwrap();
        hex::encode(bytes)
    }

    /// A `signData` result shaped like a hardware wallet's: the signer attested
    /// to `Blake2b-224(payload)` and said so with the `hashed` header, while the
    /// envelope still carries the payload itself.
    ///
    /// Built here rather than captured from a device so the shape is pinned by
    /// the test; a real capture (an Eternl account behind a Ledger) joins it
    /// once one is in hand.
    fn sign_hashed(secret: &[u8; 32], payload: &[u8], flag_in_protected: bool) -> (String, String) {
        use ed25519_dalek::{Signer as _, SigningKey};

        let signing_key = SigningKey::from_bytes(secret);
        let flag = ciborium::Value::Map(vec![(
            ciborium::Value::Text("hashed".into()),
            ciborium::Value::Bool(true),
        )]);
        let empty = ciborium::Value::Map(vec![]);
        let (protected, unprotected) = if flag_in_protected {
            (&flag, empty.clone())
        } else {
            (&empty, flag.clone())
        };
        let mut protected_bytes = Vec::new();
        ciborium::into_writer(protected, &mut protected_bytes).unwrap();

        // Sig_Structure over the HASH, not the payload — what the device signs.
        let sig_structure = ciborium::Value::Array(vec![
            ciborium::Value::Text("Signature1".into()),
            ciborium::Value::Bytes(protected_bytes.clone()),
            ciborium::Value::Bytes(vec![]),
            ciborium::Value::Bytes(blake2b224(payload).to_vec()),
        ]);
        let mut sig_structure_bytes = Vec::new();
        ciborium::into_writer(&sig_structure, &mut sig_structure_bytes).unwrap();
        let signature = signing_key.sign(&sig_structure_bytes);

        let cose_sign1 = ciborium::Value::Array(vec![
            ciborium::Value::Bytes(protected_bytes),
            unprotected.clone(),
            ciborium::Value::Bytes(payload.to_vec()),
            ciborium::Value::Bytes(signature.to_bytes().to_vec()),
        ]);
        let mut envelope = Vec::new();
        ciborium::into_writer(&cose_sign1, &mut envelope).unwrap();

        (
            hex::encode(envelope),
            cose_key_hex(&signing_key.verifying_key().to_bytes()),
        )
    }

    /// The one Damon's Ledger produces: the flag is set and the signature is
    /// over `Blake2b-224(payload)`.
    #[test]
    fn a_hardware_wallet_hash_over_the_message_verifies() {
        let message = "Link this wallet to your Discord account.\n\nDiscord ID: 1\nNonce: abc";
        let (sig, key) = sign_hashed(&SECRET_A, message.as_bytes(), true);

        assert!(
            verify_signature(&sig, &key, message).unwrap(),
            "a signature over the payload's Blake2b-224, declared by the hashed header, \
             must verify — this is the hardware wallet path"
        );
    }

    #[test]
    fn the_hashed_flag_in_the_unprotected_header_is_honoured() {
        let message = "Link this wallet to your Discord account.\n\nDiscord ID: 1\nNonce: abc";
        let (sig, key) = sign_hashed(&SECRET_A, message.as_bytes(), false);

        assert!(
            verify_signature(&sig, &key, message).unwrap(),
            "CIP-8 puts the hashed flag in the unprotected header; honouring only the \
             protected copy would refuse a spec-conforming signer"
        );
    }

    /// The other presentation seen in the wild: the hash in the payload slot
    /// itself, with no flag anywhere.
    #[test]
    fn an_attached_payload_hash_verifies() {
        use ed25519_dalek::{Signer as _, SigningKey};

        let message = "Link this wallet to your Discord account.\n\nDiscord ID: 1\nNonce: abc";
        let message_hash = blake2b224(message.as_bytes());
        let signing_key = SigningKey::from_bytes(&SECRET_A);

        let sig_structure = ciborium::Value::Array(vec![
            ciborium::Value::Text("Signature1".into()),
            ciborium::Value::Bytes(vec![0xa0]),
            ciborium::Value::Bytes(vec![]),
            ciborium::Value::Bytes(message_hash.to_vec()),
        ]);
        let mut sig_structure_bytes = Vec::new();
        ciborium::into_writer(&sig_structure, &mut sig_structure_bytes).unwrap();
        let signature = signing_key.sign(&sig_structure_bytes);

        let envelope = ciborium::Value::Array(vec![
            ciborium::Value::Bytes(vec![0xa0]),
            ciborium::Value::Map(vec![]),
            ciborium::Value::Bytes(message_hash.to_vec()),
            ciborium::Value::Bytes(signature.to_bytes().to_vec()),
        ]);
        let mut bytes = Vec::new();
        ciborium::into_writer(&envelope, &mut bytes).unwrap();

        assert!(
            verify_signature(
                &hex::encode(bytes),
                &cose_key_hex(&signing_key.verifying_key().to_bytes()),
                message,
            )
            .unwrap(),
            "a signature over the payload's hash, attached as the payload, must verify"
        );
    }

    #[test]
    fn a_bare_signature_over_the_payload_hash_verifies() {
        use ed25519_dalek::{Signer as _, SigningKey};

        let message = "Link this wallet to your Discord account.\n\nDiscord ID: 1\nNonce: abc";
        let signing_key = SigningKey::from_bytes(&SECRET_A);
        // No envelope at all: the device signed the hash and the wallet passed
        // the 64 bytes straight through.
        let signature = signing_key.sign(&blake2b224(message.as_bytes()));

        assert!(
            verify_signature(
                &hex::encode(signature.to_bytes()),
                &cose_key_hex(&signing_key.verifying_key().to_bytes()),
                message,
            )
            .unwrap(),
            "a bare 64-byte signature over the payload hash must verify"
        );
    }

    #[test]
    fn a_hash_of_a_different_message_is_rejected() {
        let (sig, key) = sign_hashed(&SECRET_A, b"a different message", true);

        assert!(
            !verify_signature(&sig, &key, "Link wallet\n\nNonce: abc123").unwrap(),
            "honouring the hashed form must not let a signature over another message through"
        );
    }

    /// A **real** Eternl-behind-a-Ledger `signData`, captured 2026-10-15 with the
    /// probe in `.tmp/wallet-probe` — the genuine device fixture
    /// `SOCIAL_WALLET_LINK_DESIGN.md` §4.5 asks for, rather than the synthetic
    /// shape above.
    ///
    /// It settles three things the synthetic fixtures could only assume:
    ///
    /// - `hashed: true` arrives in the **unprotected** header, which is where
    ///   CIP-8 puts it — *not* the protected header the design doc expected. Had
    ///   [`declares_hashed`] read only the protected copy, this would still fail.
    /// - the envelope attaches the payload itself anyway, so the caller's message
    ///   is checkable against it and the flag is the only signal of the hash form.
    /// - the protected header carries an `"address"` field (the reward address,
    ///   `e14331…`) beside `{1: -8}`, and it is the signing key's own — see
    ///   [`the_captured_envelopes_address_header_is_the_signers_own`].
    ///
    /// Two more payloads were signed in the same session — a pure-ASCII 45-byte
    /// sibling, and the real ~145-byte link message — and **Eternl asked for the
    /// hashed form every time**, so the device showed a hash rather than the
    /// message even well inside the app's ~160-byte one-APDU display window. That
    /// is the wallet's choice (the app's spec makes raw-or-hashed a host input),
    /// not a length or a character rule: a Ledger user approves a hash here, and
    /// the readable text only appears in Eternl's own dialog, never on the device.
    ///
    /// The payload contains a non-ASCII `—` (U+2014); it is reproduced verbatim
    /// because it is part of the bytes that were signed.
    #[test]
    fn a_captured_ledger_signature_verifies() {
        let message = "Link $djo to Discord — nonce 3f9a1c0b7d2e4856";
        let signature = "84582aa201276761646472657373581de14331030364d7a9cba0b64c8fd80bc59661b715fafdb6ae06ea6426a6a166686173686564f4582f4c696e6b2024646a6f20746f20446973636f726420e28094206e6f6e6365203366396131633062376432653438353658401260e426d265f605a1fe996bcbf5316016bcd075545a69cac3b9334c075ca4b889f622040eee35ed0f613e7bb73c74d259c7f4425f1bf0b1f65be7e850c1c803";
        let key =
            "a40101032720062158202668902cf792a30612cddff017a5f338e68e6f464111670b25ca45421f0e1d75";

        // The literal above is UTF-8; comparing through the hex the device saw
        // is what proves the two are the same bytes.
        assert_eq!(
            hex::encode(message.as_bytes()),
            "4c696e6b2024646a6f20746f20446973636f726420e28094206e6f6e63652033663961316330623764326534383536",
            "the message literal must be byte-identical to the captured payload"
        );
        assert!(
            verify_signature(signature, key, message).unwrap(),
            "the captured envelope attests to that message, over its Blake2b-224"
        );
        assert!(
            !verify_signature(
                signature,
                key,
                "Link $djo to Discord — nonce 0000000000000000"
            )
            .unwrap(),
            "…and to nothing else: another nonce must not verify"
        );
    }

    /// The same payload signed by the same wallet on a **software** account —
    /// captured 2026-10-15 from the probe.
    ///
    /// It corrects what the Ledger capture suggested: the hashed form is
    /// **Eternl's policy, not the Ledger's**. A software account, with no device
    /// to work around, emitted the identical envelope — `hashed: true` in the
    /// unprotected header, payload attached, signature over `Blake2b-224`. So
    /// §4.5's non-hashed real fixture has to come from another wallet entirely,
    /// and "the device shows a hash" is a consequence of the wallet's choice
    /// rather than of anything the hardware requires.
    #[test]
    fn a_captured_software_wallet_signature_verifies() {
        let message = "Link $djo to Discord - nonce 3f9a1c0b7d2e4856";
        let signature = "84582aa201276761646472657373581de1cba51a2e5b5b802a0402a87b762d2ecc6b1a9b6d0dd708daf07b82ada166686173686564f4582d4c696e6b2024646a6f20746f20446973636f7264202d206e6f6e636520336639613163306237643265343835365840598a00310358962c47ab7cf66abd5beeeeb18f94f811f93960f2f348563856379da6b8bc58f763c3ea9212c73b46b37eaf2e464140183676261a7674b8b20005";
        let key =
            "a4010103272006215820810e338829eace800ae0493066b73c1c52cc67e1c597541ebe59d9bc8f2b704b";

        assert!(
            verify_signature(signature, key, message).unwrap(),
            "the software capture attests to that message, over its Blake2b-224"
        );
        assert!(
            !verify_signature(
                signature,
                key,
                "Link $djo to Discord - nonce 0000000000000000"
            )
            .unwrap(),
            "…and to nothing else: another nonce must not verify"
        );
    }

    /// The `"address"` field the captured envelopes carry in their *protected*
    /// headers is the signing key's own address, not decoration.
    ///
    /// Worth pinning because `app-cardano`'s message-signing spec warns it "might
    /// be unrelated to the key used for signing", and the design's §4.2 step 6
    /// reads it as a check. This verifier does not, and does not need to (the
    /// derived address is ground truth and is compared to the claim); this test
    /// is the empirical answer for Eternl: on both captures, the field and the key
    /// agree.
    #[test]
    fn the_captured_envelopes_address_header_is_the_signers_own() {
        // One pair per capture: the Ledger account (`2668902c…` → `e14331…`) and
        // the software one (`810e3388…` → `e1cba51a…`).
        let captures = [
            (
                "a40101032720062158202668902cf792a30612cddff017a5f338e68e6f464111670b25ca45421f0e1d75",
                "e14331030364d7a9cba0b64c8fd80bc59661b715fafdb6ae06ea6426a6",
            ),
            (
                "a4010103272006215820810e338829eace800ae0493066b73c1c52cc67e1c597541ebe59d9bc8f2b704b",
                "e1cba51a2e5b5b802a0402a87b762d2ecc6b1a9b6d0dd708daf07b82ad",
            ),
        ];

        for (key, header_address) in captures {
            let derived = derive_stake_address(key, Network::Mainnet).unwrap();
            let derived_hex = hex::encode(
                pallas_addresses::Address::from_bech32(&derived)
                    .expect("derive_stake_address answers in bech32")
                    .to_vec(),
            );
            assert_eq!(
                derived_hex, header_address,
                "the signing key must be the address in the protected header"
            );
        }
    }

    // ── Structural rejections (these already hold) ──────────────────────────

    #[test]
    fn tampered_envelope_payload_is_rejected() {
        let (sig, key, _) = sign(&SECRET_A, "Link wallet\n\nNonce: abc123");
        let mut bytes = hex::decode(&sig).unwrap();
        // Flip a byte inside the envelope's payload region.
        let len = bytes.len();
        bytes[len - 70] ^= 0xff;
        let tampered = hex::encode(bytes);

        let verified =
            verify_signature(&tampered, &key, "Link wallet\n\nNonce: abc123").unwrap_or(false);
        assert!(
            !verified,
            "mutating the envelope must not produce a verifying signature"
        );
    }

    #[test]
    fn garbage_cbor_does_not_verify() {
        let (_, key, _) = sign(&SECRET_A, "irrelevant");
        let verified = verify_signature("deadbeefdeadbeef", &key, "irrelevant").unwrap_or(false);
        assert!(!verified, "non-COSE input must not verify");
    }

    #[test]
    fn non_hex_input_is_an_error() {
        assert!(verify_signature("zzzz", "zzzz", "msg").is_err());
    }

    // ── Address binding (§4.2 step 6) ───────────────────────────────────────

    /// `derive_payment_pkh` is `blake2b224(pubkey)`, which IS the stake
    /// credential when the key is a stake key. Different wallets must produce
    /// different credentials — the basis for binding a signature to an address.
    #[test]
    fn distinct_keys_derive_distinct_credentials() {
        let (_, key_a, addr_a) = sign(&SECRET_A, "msg");
        let (_, key_b, addr_b) = sign(&SECRET_B, "msg");

        assert_ne!(
            derive_payment_pkh(&key_a).unwrap(),
            derive_payment_pkh(&key_b).unwrap()
        );
        assert_ne!(addr_a, addr_b);
    }

    /// The primitive takes no address and therefore cannot bind one. Callers
    /// that accept a client-supplied address must derive it from the key and
    /// compare — `session::consume_challenge` and `per_request` both do;
    /// `challenge::ChallengeManager` does not.
    ///
    /// The property under test: the *key* is the sole identity in a verified
    /// signature, so anything the client claims about which wallet signed has
    /// to be checked against the key, never taken on trust.
    #[test]
    fn identity_comes_from_the_key_not_from_any_claim() {
        let msg = "Link wallet\n\nNonce: abc123";
        let (sig_a, key_a, _) = sign(&SECRET_A, msg);
        let (_, key_b, _) = sign(&SECRET_B, msg);

        // A's signature verifies under A's key and nothing about an address
        // was supplied, or could have been.
        assert!(verify_signature(&sig_a, &key_a, msg).unwrap());

        // The only thing distinguishing the two wallets is the credential
        // derived from the key — which is what a caller must compare against.
        let cred_a = derive_payment_pkh(&key_a).unwrap();
        let cred_b = derive_payment_pkh(&key_b).unwrap();
        assert_ne!(cred_a, cred_b);
        assert_eq!(
            cred_a.len(),
            56,
            "blake2b-224 credential is 28 bytes of hex"
        );
    }

    /// `derive_stake_address` must reproduce the address `sign_payload`
    /// advertises — if these ever disagree, every binding check silently
    /// rejects every honest wallet.
    #[test]
    fn derived_address_matches_the_signers_address() {
        let (_, key, addr) = sign(&SECRET_A, "msg");
        assert_eq!(derive_stake_address(&key, Network::Mainnet).unwrap(), addr);
    }

    /// The attack the binding exists to stop: sign our challenge with wallet B
    /// while claiming wallet A's address.
    #[test]
    fn attackers_key_derives_a_different_address_than_the_victims() {
        let msg = "Link wallet\n\nNonce: abc123";
        let (_, _, victim_addr) = sign(&SECRET_A, msg);
        let (attacker_sig, attacker_key, _) = sign(&SECRET_B, msg);

        // The attacker's signature over our real nonce is genuinely valid...
        assert!(verify_signature(&attacker_sig, &attacker_key, msg).unwrap());

        // ...but the address it derives to is not the one being claimed, which
        // is the only thing standing between them and the victim's identity.
        let derived = derive_stake_address(&attacker_key, Network::Mainnet).unwrap();
        assert_ne!(
            derived, victim_addr,
            "binding must reject a signature whose key derives elsewhere"
        );
    }

    #[test]
    fn network_byte_changes_the_derived_address() {
        let (_, key, mainnet_addr) = sign(&SECRET_A, "msg");
        let testnet_addr = derive_stake_address(&key, Network::Testnet).unwrap();

        assert_ne!(
            mainnet_addr, testnet_addr,
            "a mainnet claim must not be satisfiable by a testnet address"
        );
        assert!(mainnet_addr.starts_with("stake1"));
        assert!(testnet_addr.starts_with("stake_test1"));
    }
}
