//! EIP-191 (`personal_sign`) verification — the EVM sibling of `cip8`.
//!
//! An EVM wallet proves control of an address by signing a message with the
//! secp256k1 key behind it. There is no public key on the wire, so verification
//! is **recovery**: derive the signing key from `(message, signature)` and take
//! the address that key controls as ground truth. The client's claimed address
//! is a sanity check against that derivation, never an input to it — the same
//! ordering `cip8` enforces for a stake address.
//!
//! ## The two bindings, and how they differ from CIP-8
//!
//! - **Message binding is intrinsic here.** CIP-8 verification takes the public
//!   key and the expected message as separate inputs, which is why that path
//!   needs an explicit "the envelope payload equals the expected message" check
//!   (see `cip8::extract_sig_structure`). Recovery has no such gap: the message
//!   is one of the inputs that derives the key, so a signature over a different
//!   message recovers a different address and cannot pass the compare.
//! - **Address binding** is the caller comparing [`recover_address`]'s return
//!   value against the claimed address. [`verify_signature`] does both in one
//!   call; a caller that needs the derived address (to compare, store or log
//!   it) should call [`recover_address`] and keep the claim out of the
//!   derivation.
//!
//! ## What the wallet signs
//!
//! `personal_sign` over UTF-8 bytes, hashed the way EIP-191 says:
//!
//! ```text
//! keccak256("\x19Ethereum Signed Message:\n" + <byte length, decimal> + <message bytes>)
//! ```
//!
//! Note where the length comes from: **bytes, not characters.** A client that
//! wants the wallet to sign a challenge verbatim should pass the message
//! hex-encoded (`0x…` of the UTF-8 bytes): `personal_sign` hex-decodes a string
//! param that looks like hex, and the challenge messages this estate mints are
//! ASCII, which is exactly where that ambiguity would bite.
//!
//! ## Signature encoding
//!
//! 65 bytes, `r || s || v`. `v` is accepted as `0`/`1` (some wallets) or
//! `27`/`28` (MetaMask and most of the ecosystem); EIP-155 values (`>= 35`) are
//! refused rather than guessed at, because `personal_sign` has no chain
//! component — a `v` that carries one is not the signature this verifies.
//!
//! ## High-s signatures
//!
//! Ethereum's `ecrecover` accepts high-s; RustCrypto's recovery rejects it
//! (EIP-2 forbids high-s in *transaction* signatures only, so a wallet may
//! legitimately emit one for `personal_sign`). [`recover`] retries with the
//! low-s normalisation and the matching recovery-id flip — the same
//! transformation a signer makes when normalising — so both forms recover the
//! same key instead of one of them failing.

use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use sha3::{Digest as _, Keccak256};

/// Recover the address that signed `message`, per EIP-191 `personal_sign`.
///
/// Returns the canonical lower-case `0x…` form. EIP-55's mixed case is a
/// checksum, not identity — the ownership data this estate keeps stores the
/// lower-case form, so every downstream comparison is a string compare.
pub fn recover_address(signature_hex: &str, message: &str) -> Result<String, String> {
    let key = recover(signature_hex, message)?;
    Ok(address_of(&key))
}

/// Compare the recovered address against a claim.
///
/// `Ok(false)` is "a valid signature, over this message, by a key that is not
/// the claimed one" — a different person, not a malformed request. A claim that
/// is not an address at all is an `Err`, because that is a client bug and
/// should read like one.
pub fn verify_signature(
    signature_hex: &str,
    message: &str,
    expected_address: &str,
) -> Result<bool, String> {
    let expected = normalise_address(expected_address)?;
    Ok(recover_address(signature_hex, message)? == expected)
}

/// Validate and lower-case an EVM address.
///
/// Strict on shape: `0x` plus exactly 40 hex digits. An address is an
/// identifier here, and accepting a mistyped or truncated one would turn "not
/// you" into a silent no-op.
pub fn normalise_address(address: &str) -> Result<String, String> {
    let digits = address
        .strip_prefix("0x")
        .or_else(|| address.strip_prefix("0X"))
        .ok_or_else(|| format!("EVM address must be 0x-prefixed, got: {address}"))?;
    if digits.len() != 40 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!(
            "EVM address must be 0x + 40 hex digits, got: {address}"
        ));
    }
    Ok(format!("0x{}", digits.to_ascii_lowercase()))
}

/// `address` in its EIP-55 mixed-case checksum form: what a Sign-In with Ethereum (EIP-4361)
/// message must carry, and what a wallet shows. A letter is upper-cased where the matching
/// nibble of `keccak256(lower-case hex)` is 8 or more.
pub fn checksum_address(address: &str) -> Result<String, String> {
    let lower = normalise_address(address)?;
    let digits = &lower[2..];
    let hash = Keccak256::digest(digits.as_bytes());
    let mixed: String = digits
        .chars()
        .enumerate()
        .map(|(i, c)| {
            let nibble = (hash[i / 2] >> if i % 2 == 0 { 4 } else { 0 }) & 0x0f;
            match nibble >= 8 {
                true => c.to_ascii_uppercase(),
                false => c,
            }
        })
        .collect();
    Ok(format!("0x{mixed}"))
}

/// Recover the signing key. Private because the key is an implementation
/// detail: an address is the identity this estate carries across boundaries.
fn recover(signature_hex: &str, message: &str) -> Result<VerifyingKey, String> {
    let bytes = hex::decode(signature_hex).map_err(|e| format!("Invalid signature hex: {e}"))?;
    if bytes.len() != 65 {
        return Err(format!(
            "Signature must be 65 bytes (r||s||v), got {}",
            bytes.len()
        ));
    }

    let signature =
        Signature::from_slice(&bytes[..64]).map_err(|e| format!("Invalid ECDSA signature: {e}"))?;
    let recovery_id = recovery_id(bytes[64])?;
    let prehash = personal_message_hash(message);

    match VerifyingKey::recover_from_prehash(&prehash, &signature, recovery_id) {
        Ok(key) => Ok(key),
        Err(first) => {
            // See "High-s signatures" in the module header. `normalize_s` is a
            // no-op on a low-s signature, and retrying with a flipped recovery
            // id there would recover a different key — so only retry when s
            // actually changed.
            let low = signature.normalize_s();
            if low == signature {
                return Err(format!("Cannot recover signer: {first}"));
            }
            let flipped = RecoveryId::new(!recovery_id.is_y_odd(), recovery_id.is_x_reduced());
            VerifyingKey::recover_from_prehash(&prehash, &low, flipped)
                .map_err(|e| format!("Cannot recover signer: {e}"))
        }
    }
}

/// The `v` byte → recovery id. Typed refusals — see the module header.
fn recovery_id(v: u8) -> Result<RecoveryId, String> {
    let is_y_odd = match v {
        0 | 27 => false,
        1 | 28 => true,
        other => {
            return Err(format!(
                "Signature v must be 0/1/27/28, got {other}; an EIP-155 v is not a personal_sign value"
            ));
        }
    };
    Ok(RecoveryId::new(is_y_odd, false))
}

/// `keccak256("\x19Ethereum Signed Message:\n" + <byte length> + <message>)`.
fn personal_message_hash(message: &str) -> [u8; 32] {
    let bytes = message.as_bytes();
    let mut hasher = Keccak256::new();
    hasher.update(b"\x19Ethereum Signed Message:\n");
    hasher.update(bytes.len().to_string().as_bytes());
    hasher.update(bytes);
    hasher.finalize().into()
}

/// The EVM address an uncompressed public key controls:
/// `keccak256(pubkey[1..])[12..]`, lower-cased.
fn address_of(key: &VerifyingKey) -> String {
    let point = key.to_sec1_point(false);
    let hash = Keccak256::digest(&point.as_bytes()[1..]);
    format!("0x{}", hex::encode(&hash[12..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::SigningKey;
    use k256::elliptic_curve::PrimeField as _;
    use k256::elliptic_curve::scalar::IsHigh as _;

    /// The four all-mixed-case vectors EIP-55 itself lists.
    #[test]
    fn a_checksummed_address_matches_the_eip_55_vectors() {
        for expected in [
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
            "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359",
            "0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB",
            "0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb",
        ] {
            assert_eq!(
                checksum_address(&expected.to_ascii_lowercase()).unwrap(),
                expected
            );
        }
    }

    /// A deterministic key — `seed` is the last byte of the scalar, so any
    /// non-zero value is a valid secp256k1 key.
    fn key(seed: u8) -> SigningKey {
        let mut bytes = [0u8; 32];
        bytes[31] = seed;
        SigningKey::from_slice(&bytes).expect("seed is a valid scalar")
    }

    fn address(signing: &SigningKey) -> String {
        address_of(signing.verifying_key())
    }

    fn preimage(message: &str) -> Vec<u8> {
        let mut bytes = b"\x19Ethereum Signed Message:\n".to_vec();
        bytes.extend_from_slice(message.len().to_string().as_bytes());
        bytes.extend_from_slice(message.as_bytes());
        bytes
    }

    /// Sign the way a wallet does for `personal_sign`: the EIP-191 digest,
    /// low-s, and the `v` byte in its `27`/`28` form.
    fn sign(signing: &SigningKey, message: &str) -> String {
        sign_digest(signing, Keccak256::new_with_prefix(preimage(message)))
    }

    /// Sign a digest the way Ethereum libraries do: low-s with the matching
    /// recovery id, encoded as `r || s || v`.
    fn sign_digest(signing: &SigningKey, digest: Keccak256) -> String {
        let (signature, recovery_id) =
            signing.sign_prehash_recoverable(digest.finalize().as_slice());
        encode(&signature, recovery_id, 27)
    }

    fn encode(signature: &Signature, recovery_id: RecoveryId, v_base: u8) -> String {
        let mut bytes = signature.to_bytes().to_vec();
        bytes.push(v_base + recovery_id.to_byte());
        hex::encode(bytes)
    }

    #[test]
    fn recovery_returns_the_signer() {
        let signing = key(7);
        let signed = sign(&signing, "Link this wallet to your Discord account.");
        assert_eq!(
            recover_address(&signed, "Link this wallet to your Discord account.").unwrap(),
            address(&signing)
        );
    }

    #[test]
    fn address_is_well_formed_and_stable() {
        let signing = key(9);
        let first = address(&signing);
        assert!(first.starts_with("0x"), "lower-case 0x form: {first}");
        assert_eq!(first.len(), 42);
        assert!(
            first[2..].bytes().all(|b| b.is_ascii_hexdigit()),
            "hex digits only: {first}"
        );
        assert_eq!(address(&signing), first, "derivation is deterministic");
        assert_ne!(address(&signing), address(&key(10)));
    }

    #[test]
    fn a_message_of_a_different_length_recovers_a_different_key() {
        // The byte length is inside the signed preimage, so a message that is a
        // prefix of another must not verify as it.
        let signing = key(11);
        let signed = sign(&signing, "Sign in");
        assert_ne!(
            recover_address(&signed, "Sign in").unwrap(),
            recover_address(&signed, "Sign in to the platform").unwrap(),
        );
        assert!(!verify_signature(&signed, "Sign in to the platform", &address(&signing)).unwrap());
    }

    #[test]
    fn a_signature_over_the_raw_message_without_the_eip191_prefix_is_rejected() {
        // What a wallet would produce for a signing scheme that skipped the
        // EIP-191 envelope. It must not pass, or every signature harvested from
        // any other EVM flow would satisfy ours.
        let signing = key(12);
        let message = "Link this wallet to your Discord account.";
        let raw = sign_digest(&signing, Keccak256::new_with_prefix(message.as_bytes()));
        assert!(!verify_signature(&raw, message, &address(&signing)).unwrap());
    }

    #[test]
    fn another_keys_signature_never_passes_the_claim() {
        let victim = key(13);
        let attacker = key(14);
        let signed = sign(&attacker, "Link this wallet to your Discord account.");
        assert_eq!(
            recover_address(&signed, "Link this wallet to your Discord account.").unwrap(),
            address(&attacker),
        );
        assert!(
            !verify_signature(
                &signed,
                "Link this wallet to your Discord account.",
                &address(&victim),
            )
            .unwrap(),
            "the victim's address must not verify against the attacker's key"
        );
    }

    #[test]
    fn v_added_to_zero_and_to_twenty_seven_recover_alike() {
        let signing = key(15);
        let message = "v byte forms";
        let (signature, recovery_id) = signing.sign_prehash_recoverable(
            Keccak256::new_with_prefix(preimage(message))
                .finalize()
                .as_slice(),
        );
        let as_0_1 = encode(&signature, recovery_id, 0);
        let as_27_28 = encode(&signature, recovery_id, 27);
        assert_eq!(
            recover_address(&as_0_1, message).unwrap(),
            recover_address(&as_27_28, message).unwrap(),
        );
        assert_eq!(
            recover_address(&as_0_1, message).unwrap(),
            address(&signing)
        );
    }

    #[test]
    fn a_high_s_signature_recovers_the_signer() {
        // Ethereum's ecrecover accepts high-s; RustCrypto's does not. Flip s to
        // its high form and the recovery-id parity with it — the same key must
        // come back, not an error.
        let signing = key(16);
        let message = "high-s";
        let (signature, recovery_id) = signing.sign_prehash_recoverable(
            Keccak256::new_with_prefix(preimage(message))
                .finalize()
                .as_slice(),
        );
        let low_s = *signature.s();
        assert!(
            !bool::from(low_s.is_high()),
            "sign_digest_recoverable normalises s"
        );
        let high_s = -low_s;
        let high = Signature::from_scalars(signature.r().to_repr(), high_s.to_repr())
            .expect("both scalars are in range");
        let flipped = RecoveryId::new(!recovery_id.is_y_odd(), recovery_id.is_x_reduced());
        let signed = encode(&high, flipped, 27);
        assert_eq!(
            recover_address(&signed, message).unwrap(),
            address(&signing)
        );
    }

    #[test]
    fn an_eip155_v_byte_is_refused_rather_than_guessed_at() {
        let signing = key(17);
        let message = "chain-tagged";
        let (signature, _) = signing.sign_prehash_recoverable(
            Keccak256::new_with_prefix(preimage(message))
                .finalize()
                .as_slice(),
        );
        let mut bytes = signature.to_bytes().to_vec();
        bytes.push(35); // 0x23: chain 0, recovery bit 0, EIP-155 form
        let err = recover_address(&hex::encode(bytes), message).unwrap_err();
        assert!(err.contains("EIP-155"), "names the reason: {err}");
    }

    #[test]
    fn a_signature_of_the_wrong_length_is_an_error_not_a_guess() {
        let signing = key(18);
        let message = "length";
        let (signature, _) = signing.sign_prehash_recoverable(
            Keccak256::new_with_prefix(preimage(message))
                .finalize()
                .as_slice(),
        );
        let truncated = hex::encode(&signature.to_bytes()[..63]);
        let err = recover_address(&truncated, message).unwrap_err();
        assert!(err.contains("65"), "names the expected size: {err}");
    }

    #[test]
    fn non_hex_input_is_an_error() {
        let err = recover_address("not-hex", "message").unwrap_err();
        assert!(err.contains("hex"), "names the encoding: {err}");
    }

    #[test]
    fn a_multi_byte_message_hashes_its_bytes_not_its_characters() {
        // EIP-191 puts the byte length in the preimage. If either side counted
        // characters, the two-byte characters below would break the round trip.
        let signing = key(19);
        let message = "héllo → 日本語";
        let signed = sign(&signing, message);
        assert_eq!(
            recover_address(&signed, message).unwrap(),
            address(&signing)
        );
    }

    #[test]
    fn addresses_are_compared_without_checksum_case() {
        let signing = key(20);
        let mut mixed = address(&signing);
        // Deliberately not a valid EIP-55 checksum; the estate does not keep one.
        mixed.replace_range(2..3, &mixed[2..3].to_ascii_uppercase());
        assert!(
            verify_signature(&sign(&signing, "case"), "case", &mixed).unwrap(),
            "EIP-55 case is not identity"
        );
    }

    #[test]
    fn normalising_rejects_anything_that_is_not_an_address() {
        assert_eq!(
            normalise_address("0xAbCdEf0000000000000000000000000000000000").unwrap(),
            "0xabcdef0000000000000000000000000000000000",
        );
        assert_eq!(
            normalise_address("0XABCDEF0000000000000000000000000000000000").unwrap(),
            "0xabcdef0000000000000000000000000000000000",
        );
        assert!(normalise_address("abcdef0000000000000000000000000000000000").is_err());
        assert!(normalise_address("0xabcdef").is_err());
        // 39 and 41 hex digits — off-by-one on either side is not an address.
        assert!(normalise_address("0xabcdef000000000000000000000000000000000").is_err());
        assert!(normalise_address("0xabcdef00000000000000000000000000000000000").is_err());
        assert!(normalise_address("0xgggggggggggggggggggggggggggggggggggggggg").is_err());
    }

    #[test]
    fn a_malformed_claim_is_an_error_not_a_false() {
        let signing = key(21);
        let signed = sign(&signing, "claim");
        // "Not an address" and "not your address" are different answers.
        assert!(verify_signature(&signed, "claim", "stake1u9notanaddress").is_err());
        assert!(
            !verify_signature(
                &signed,
                "claim",
                "0x0000000000000000000000000000000000000001"
            )
            .unwrap()
        );
    }
}
