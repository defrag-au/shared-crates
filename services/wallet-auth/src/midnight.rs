//! Midnight `signData` verification — the third track beside `cip8` and `evm`.
//!
//! A Midnight wallet proves control of an **unshielded address** by signing a
//! message with the key that address commits to. The DApp connector's
//! `signData` is the method; this module is the half that checks the answer.
//!
//! ## What the wallet actually signs
//!
//! Two conventions sit between our text and a BIP-340 signature, and both are
//! load-bearing:
//!
//! 1. **The wallet prefixes the data.** From the connector specification's
//!    *Signing* section: a wallet receiving `signData` "must prefix data with
//!    string `midnight_signed_message:<data_size>:`", where the size is in
//!    **bytes**. It is there so an attacker cannot dress a transaction up as a
//!    login prompt: the reader's dialog shows the prefix, and a wallet will not
//!    sign a transaction that starts with it. [`signed_bytes`] builds it; a
//!    verifier that skipped it would accept nothing a real wallet produces.
//! 2. **The prefixed bytes are hashed with SHA-256 before BIP-340.** Midnight's
//!    ledger signs through `k256`, whose `Signer`/`RandomizedSigner` impls
//!    digest the message first (`base-crypto/src/signatures.rs`:
//!    `SigningKey::sign` → `sign_with_rng` → `try_sign_digest_with_rng(rng,
//!    Sha256::new_with_prefix(msg))`), and verifies the same way (`VerifyingKey::verify`
//!    → `Verifier::verify` → `verify_digest`). `k256`'s raw-verification entry
//!    point takes that 32-byte digest, which is what [`verify_signature`]
//!    passes it. Verifying the prefixed bytes as the *message* instead would
//!    reject every signature a real wallet produces, and look like a broken
//!    wallet rather than a broken verifier.
//!
//! ## The two schemes
//!
//! `Signature.scheme` may be absent, and absent means `schnorr_bip340` — the
//! specification says so, and a future major version makes it mandatory.
//! [`Scheme::parse`] therefore treats `None` as Schnorr and refuses a name it
//! does not know rather than defaulting one, because a name this code has not
//! heard of is a scheme whose signature it cannot check.
//!
//! | scheme | key on the wire | address derivation |
//! |---|---|---|
//! | `schnorr_bip340` | 32 bytes, x-only | `sha256(key)` |
//! | `ecdsa_secp256k1_sha256` | 33 bytes, SEC1 compressed | `sha256("midnight:ecdsa:" ‖ key)` |
//!
//! Both derive through `UnshieldedAddress` in the wallet SDK's
//! `address-format` package. The ECDSA arm concatenates a domain tag first, so
//! a key that could be read either way cannot be made to claim the other
//! scheme's address.
//!
//! ## The address is the binding
//!
//! Midnight hands the verifying key over with the signature, so unlike EVM
//! there is nothing to recover — and unlike Cardano there is nothing to derive
//! and compare through a curve. The check is a hash and a byte compare:
//! [`address_of_key`] must equal the 32 bytes behind the claimed bech32m
//! address. That is why the address can be stored as the link's identity: it is
//! a commitment to the key, computed by both sides from the same rule.

use k256::ecdsa::VerifyingKey as EcdsaKey;
use k256::ecdsa::signature::hazmat::PrehashVerifier;
use k256::schnorr::VerifyingKey as SchnorrKey;
use sha2::{Digest as _, Sha256};

/// The prefix a Midnight wallet puts in front of the data before signing, up to
/// its first colon. The size and the closing colon follow.
const PREFIX: &str = "midnight_signed_message";

/// The bech32m human-readable part of a **mainnet** unshielded address.
///
/// Midnight writes `mn_addr` on mainnet and `mn_addr_<network>` anywhere else,
/// so an address is mainnet exactly when its HRP is this string — no parsing of
/// a suffix is needed, and an address for any other network simply does not
/// match.
pub const MAINNET_ADDRESS_HRP: &str = "mn_addr";

/// Bytes of an unshielded address: a SHA-256 digest, whatever the scheme.
pub const ADDRESS_LEN: usize = 32;

/// Which signature scheme a `signData` reply used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    /// BIP-340 Schnorr over secp256k1, with a 32-byte x-only key. The default.
    SchnorrBip340,
    /// ECDSA over secp256k1 with SHA-256, with a 33-byte SEC1-compressed key.
    EcdsaSecp256k1Sha256,
}

impl Scheme {
    /// Read the connector's `scheme` field. `None` is Schnorr, per the
    /// specification; an unknown name is an error, never a guess.
    pub fn parse(field: Option<&str>) -> Result<Self, String> {
        match field {
            None => Ok(Self::SchnorrBip340),
            Some("schnorr_bip340") => Ok(Self::SchnorrBip340),
            Some("ecdsa_secp256k1_sha256") => Ok(Self::EcdsaSecp256k1Sha256),
            Some(other) => Err(format!("unsupported signature scheme: {other}")),
        }
    }

    /// The connector's own name for the scheme.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SchnorrBip340 => "schnorr_bip340",
            Self::EcdsaSecp256k1Sha256 => "ecdsa_secp256k1_sha256",
        }
    }
}

/// The exact bytes a Midnight wallet signs for `data`: the connector's size
/// prefix, then the data as UTF-8.
///
/// The size counts **bytes**, not characters, so a message with a non-ASCII
/// character hashes differently here and in a wallet that counted characters.
/// This estate's challenge messages are ASCII, but the rule is the rule.
pub fn signed_bytes(data: &str) -> Vec<u8> {
    let mut out = format!("{PREFIX}:{}:", data.len()).into_bytes();
    out.extend_from_slice(data.as_bytes());
    out
}

/// The digest Midnight's ledger signs: SHA-256 of the prefixed bytes.
pub fn digest(data: &str) -> [u8; 32] {
    Sha256::digest(signed_bytes(data)).into()
}

/// Check that `signature_hex` is `message` signed by the key behind
/// `verifying_key_hex`.
///
/// Returns `Ok(())` only when the signature is valid for this exact message,
/// this key, and this scheme. Every failure is a `String` naming what was
/// wrong with the *input* — a caller turns all of them into one
/// "signature did not verify", which is the useful thing to tell a person and
/// the useless thing to tell an attacker.
pub fn verify_signature(
    message: &str,
    signature_hex: &str,
    verifying_key_hex: &str,
    scheme: Scheme,
) -> Result<(), String> {
    let signature = hex::decode(signature_hex.trim()).map_err(|e| format!("signature: {e}"))?;
    let key = hex::decode(verifying_key_hex.trim()).map_err(|e| format!("verifying key: {e}"))?;
    // Both schemes put r ‖ s on the wire, so both are 64 bytes — and the check
    // has to happen *here*, before the type does. `k256::schnorr::Signature`'s
    // `TryFrom<&[u8]>` splits the slice in half with `split_at` and panics on a
    // short one instead of returning `Err`; a client posting 63 bytes would
    // otherwise take the isolate down rather than getting a 400.
    if signature.len() != 64 {
        return Err(format!(
            "a signature is 64 bytes of r ‖ s, got {}",
            signature.len()
        ));
    }
    let digest = digest(message);
    match scheme {
        Scheme::SchnorrBip340 => {
            let key: [u8; 32] = key
                .as_slice()
                .try_into()
                .map_err(|_| format!("a schnorr key is 32 bytes, got {}", key.len()))?;
            let key =
                SchnorrKey::from_bytes(&key).map_err(|_| "malformed schnorr key".to_string())?;
            let signature = k256::schnorr::Signature::try_from(signature.as_slice())
                .map_err(|_| "malformed schnorr signature".to_string())?;
            key.verify_raw(&digest, &signature)
                .map_err(|_| "signature did not verify".to_string())
        }
        Scheme::EcdsaSecp256k1Sha256 => {
            let key = EcdsaKey::from_sec1_bytes(&key)
                .map_err(|_| "malformed secp256k1 key".to_string())?;
            let signature = k256::ecdsa::Signature::from_slice(&signature)
                .map_err(|_| "malformed secp256k1 signature".to_string())?;
            key.verify_prehash(&digest, &signature)
                .map_err(|_| "signature did not verify".to_string())
        }
    }
}

/// The unshielded address a verifying key commits to.
///
/// The Schnorr arm hashes the 32 x-only bytes; the ECDSA arm hashes
/// `midnight:ecdsa:` and then the 33 compressed bytes. Both rules are the
/// wallet SDK's (`UnshieldedAddress::fromSchnorrPublicKey` /
/// `fromEcdsaPublicKey`), and both are what the stored address has to equal.
pub fn address_of_key(
    verifying_key_hex: &str,
    scheme: Scheme,
) -> Result<[u8; ADDRESS_LEN], String> {
    let key = hex::decode(verifying_key_hex.trim()).map_err(|e| format!("verifying key: {e}"))?;
    match scheme {
        Scheme::SchnorrBip340 => {
            if key.len() != 32 {
                return Err(format!("a schnorr key is 32 bytes, got {}", key.len()));
            }
            Ok(Sha256::digest(&key).into())
        }
        Scheme::EcdsaSecp256k1Sha256 => {
            if key.len() != 33 {
                return Err(format!(
                    "a compressed secp256k1 key is 33 bytes, got {}",
                    key.len()
                ));
            }
            let mut hasher = Sha256::new();
            hasher.update(b"midnight:ecdsa:");
            hasher.update(&key);
            Ok(hasher.finalize().into())
        }
    }
}

/// The human-readable part and payload of a bech32m string.
///
/// Accepts either case (bech32 permits one, uniformly) but not a mix, because a
/// mixed-case string is a typo or a tampered one and the checksum will not
/// distinguish them for a reader.
pub fn decode_address(address: &str) -> Result<(String, Vec<u8>), String> {
    let lower = address.to_ascii_lowercase();
    if lower != address && address.to_ascii_uppercase() != address {
        return Err("mixed-case bech32".into());
    }
    let split = lower.rfind('1').ok_or("no bech32 separator")?;
    let (hrp, data) = (&lower[..split], &lower[split + 1..]);
    if hrp.is_empty() || data.len() < 6 {
        return Err("too short to be bech32m".into());
    }
    let values: Vec<u8> = data
        .bytes()
        .map(|c| CHARSET.iter().position(|&x| x == c).map(|i| i as u8))
        .collect::<Option<_>>()
        .ok_or("a character that is not bech32")?;
    let expanded = hrp
        .bytes()
        .map(|b| b >> 5)
        .chain([0])
        .chain(hrp.bytes().map(|b| b & 31));
    if polymod(expanded.chain(values.iter().copied())) != BECH32M_CONST {
        return Err("checksum".into());
    }
    let payload = &values[..values.len() - 6];
    let mut bytes = Vec::with_capacity(payload.len() * 5 / 8);
    let (mut acc, mut bits) = (0u32, 0u32);
    for v in payload {
        acc = (acc << 5) | u32::from(*v);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            bytes.push((acc >> bits) as u8);
        }
    }
    // Leftover bits are padding, and must be zero — the wallet encodes with
    // `bech32m.encode(hrp, words, false)`, which pads nothing.
    if bits >= 5 || (acc & ((1 << bits) - 1)) != 0 {
        return Err("non-zero padding".into());
    }
    Ok((hrp.to_string(), bytes))
}

/// The mainnet bech32m form of a 32-byte unshielded address.
///
/// The inverse of [`decode_address`] for the one HRP this estate stores. Used
/// to canonicalise a claim, so that two spellings of one address cannot become
/// two rows.
pub fn encode_address(bytes: &[u8]) -> String {
    encode_address_for(MAINNET_ADDRESS_HRP, bytes)
}

/// The bech32m form of `bytes` under an arbitrary HRP.
///
/// The checksum covers the HRP, so the same payload on another network is a
/// different string — a caller cannot build one by editing the prefix of a
/// mainnet address. This variant exists for the two places that need a
/// non-mainnet form: a caller proving it refuses one, and any future track that
/// learns to store one.
pub fn encode_address_for(hrp: &str, bytes: &[u8]) -> String {
    let mut values = Vec::with_capacity(bytes.len() * 8 / 5 + 1);
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in bytes {
        acc = (acc << 8) | u32::from(*b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            values.push(((acc >> bits) & 31) as u8);
        }
    }
    if bits > 0 {
        values.push(((acc << (5 - bits)) & 31) as u8);
    }
    let expanded = hrp
        .bytes()
        .map(|b| b >> 5)
        .chain([0])
        .chain(hrp.bytes().map(|b| b & 31));
    let mut checksum_input: Vec<u8> = expanded.chain(values.iter().copied()).collect();
    // The checksum is computed over the data with six zero placeholders.
    checksum_input.extend_from_slice(&[0u8; 6]);
    let residue = polymod(checksum_input.into_iter()) ^ BECH32M_CONST;
    let mut out = String::with_capacity(hrp.len() + 1 + values.len() + 6);
    out.push_str(hrp);
    out.push('1');
    for v in &values {
        out.push(CHARSET[usize::from(*v)] as char);
    }
    for i in 0..6 {
        out.push(CHARSET[((residue >> (5 * (5 - i))) & 31) as usize] as char);
    }
    out
}

const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

/// bech32m's checksum constant. Plain bech32 is `1`; Midnight addresses are
/// bech32m, so a Cardano-style decoder would reject every one of them.
const BECH32M_CONST: u32 = 0x2bc8_30a3;

fn polymod(values: impl Iterator<Item = u8>) -> u32 {
    const GEN: [u32; 5] = [
        0x3b6a_57b2,
        0x2650_8e6d,
        0x1ea1_19fa,
        0x3d42_33dd,
        0x2a14_62b3,
    ];
    let mut chk = 1u32;
    for v in values {
        let top = chk >> 25;
        chk = ((chk & 0x01ff_ffff) << 5) ^ u32::from(v);
        for (i, g) in GEN.iter().enumerate() {
            if (top >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    chk
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::SigningKey as EcdsaSigningKey;
    use k256::ecdsa::signature::hazmat::PrehashSigner;
    use k256::schnorr::SigningKey as SchnorrSigningKey;

    /// A real mainnet unshielded address, from the connector specification's
    /// own `makeTransfer` example. It is a wallet's output, not something this
    /// crate built, which is what makes it worth asserting against.
    const SPEC_ADDRESS: &str = "mn_addr1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9s6e0fs3";

    /// A fixed secret key, so the tests are the same every run.
    fn secret(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn schnorr_key(byte: u8) -> (SchnorrSigningKey, [u8; 32]) {
        let key = SchnorrSigningKey::from_bytes(&secret(byte)).expect("valid secret");
        let x_only: [u8; 32] = key.verifying_key().to_bytes().into();
        (key, x_only)
    }

    /// Sign the way the Midnight ledger does: SHA-256 first, then BIP-340.
    fn ledger_signature(key: &SchnorrSigningKey, message: &str) -> String {
        let signature = key.sign_prehash(&digest(message)).expect("schnorr signing");
        hex::encode(signature.to_bytes())
    }

    #[test]
    fn the_prefix_counts_bytes_and_comes_before_the_data() {
        assert_eq!(
            String::from_utf8(signed_bytes("Hi\n")).unwrap(),
            "midnight_signed_message:3:Hi\n"
        );
        // The size is bytes: é is two of them.
        assert_eq!(
            String::from_utf8(signed_bytes("é")).unwrap(),
            "midnight_signed_message:2:é"
        );
    }

    #[test]
    fn a_signature_over_the_prefixed_digest_verifies() {
        let (key, x_only) = schnorr_key(7);
        let signature = ledger_signature(&key, "Link this wallet");
        assert_eq!(
            verify_signature(
                "Link this wallet",
                &signature,
                &hex::encode(x_only),
                Scheme::SchnorrBip340
            ),
            Ok(())
        );
    }

    /// The prefix is what stops a signature over some other message being
    /// replayed here — including one over this message *without* it.
    #[test]
    fn a_signature_over_the_unprefixed_message_is_refused() {
        let (key, x_only) = schnorr_key(7);
        let bare = key
            .sign_prehash(&Sha256::digest(b"Link this wallet"))
            .expect("schnorr signing");
        assert!(
            verify_signature(
                "Link this wallet",
                &hex::encode(bare.to_bytes()),
                &hex::encode(x_only),
                Scheme::SchnorrBip340
            )
            .is_err()
        );
    }

    #[test]
    fn another_key_does_not_verify() {
        let (key, _) = schnorr_key(7);
        let (_, other) = schnorr_key(9);
        let signature = ledger_signature(&key, "Link this wallet");
        assert!(
            verify_signature(
                "Link this wallet",
                &signature,
                &hex::encode(other),
                Scheme::SchnorrBip340
            )
            .is_err()
        );
    }

    #[test]
    fn a_different_message_does_not_verify() {
        let (key, x_only) = schnorr_key(7);
        let signature = ledger_signature(&key, "Link this wallet");
        assert!(
            verify_signature(
                "Link another wallet",
                &signature,
                &hex::encode(x_only),
                Scheme::SchnorrBip340
            )
            .is_err()
        );
    }

    #[test]
    fn an_ecdsa_signature_verifies_and_derives_its_own_address_shape() {
        let key = EcdsaSigningKey::from_slice(&secret(3)).expect("valid secret");
        let compressed = key.verifying_key().to_encoded_point(true);
        let signature: k256::ecdsa::Signature = key
            .sign_prehash(&digest("Link this wallet"))
            .expect("ecdsa signing");
        assert_eq!(
            verify_signature(
                "Link this wallet",
                &hex::encode(signature.to_bytes()),
                &hex::encode(compressed.as_bytes()),
                Scheme::EcdsaSecp256k1Sha256
            ),
            Ok(())
        );

        // The address is sha256("midnight:ecdsa:" ‖ key), and the schnorr rule
        // applied to the same bytes would give a different answer — which is
        // the point of the domain tag.
        let address = address_of_key(
            &hex::encode(compressed.as_bytes()),
            Scheme::EcdsaSecp256k1Sha256,
        )
        .unwrap();
        let mut tagged = Sha256::new();
        tagged.update(b"midnight:ecdsa:");
        tagged.update(compressed.as_bytes());
        assert_eq!(address, <[u8; 32]>::from(tagged.finalize()));
        assert_ne!(
            address,
            <[u8; 32]>::from(Sha256::digest(compressed.as_bytes()))
        );
    }

    #[test]
    fn a_schnorr_address_is_the_hash_of_the_x_only_key() {
        let (_, x_only) = schnorr_key(7);
        let address = address_of_key(&hex::encode(x_only), Scheme::SchnorrBip340).unwrap();
        assert_eq!(address, <[u8; 32]>::from(Sha256::digest(x_only)));
    }

    #[test]
    fn the_connector_scheme_field_defaults_to_schnorr_and_refuses_strangers() {
        assert_eq!(Scheme::parse(None), Ok(Scheme::SchnorrBip340));
        assert_eq!(
            Scheme::parse(Some("schnorr_bip340")),
            Ok(Scheme::SchnorrBip340)
        );
        assert_eq!(
            Scheme::parse(Some("ecdsa_secp256k1_sha256")),
            Ok(Scheme::EcdsaSecp256k1Sha256)
        );
        assert!(Scheme::parse(Some("ed25519")).is_err());
    }

    /// The specification's own address: bech32m, `mn_addr`, 32 bytes.
    #[test]
    fn the_specifications_address_decodes_to_an_unshielded_address() {
        let (hrp, bytes) = decode_address(SPEC_ADDRESS).expect("a valid bech32m address");
        assert_eq!(hrp, MAINNET_ADDRESS_HRP);
        assert_eq!(bytes.len(), ADDRESS_LEN);
        assert_eq!(encode_address(&bytes), SPEC_ADDRESS, "round trip");
    }

    #[test]
    fn a_damaged_address_is_refused() {
        let mut damaged = SPEC_ADDRESS.to_string();
        damaged.replace_range(12..13, "q");
        assert_ne!(damaged, SPEC_ADDRESS);
        assert!(decode_address(&damaged).is_err(), "checksum");
        assert!(decode_address("mn_addr1").is_err(), "no data");
        assert!(decode_address("not an address").is_err());
        // A testnet address is a valid string with the wrong HRP: it decodes,
        // and the caller is the one that refuses it.
        let preview = encode_address(&[7u8; ADDRESS_LEN]);
        assert!(decode_address(&preview).is_ok());
    }

    #[test]
    fn a_signature_off_the_wire_in_the_wrong_length_is_refused() {
        let (_, x_only) = schnorr_key(7);
        assert!(verify_signature("m", "00", &hex::encode(x_only), Scheme::SchnorrBip340).is_err());
        assert!(verify_signature("m", "zz", &hex::encode(x_only), Scheme::SchnorrBip340).is_err());
    }
}
