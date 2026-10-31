//! What a connected wallet is asked to prove, and how its proof is written.

use chains::ChainFamily;
use wallet_link::LinkedWallet;
use wallet_link::link::{CardanoLinkRequest, EvmLinkRequest};

/// The chains auth-service can link today, in the order to offer them.
pub const LINKABLE: [ChainFamily; 2] = [ChainFamily::Cardano, ChainFamily::Evm];

/// An account a connected wallet can prove it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Claim {
    /// A Cardano reward address, bech32 (`stake1…`): the one credential a
    /// CIP-8 signature proves, and the form links are stored in.
    Cardano { stake_address: String },
    /// An EVM account, `0x…`, in any case.
    Evm { address: String },
}

impl Claim {
    pub fn family(&self) -> ChainFamily {
        match self {
            Self::Cardano { .. } => ChainFamily::Cardano,
            Self::Evm { .. } => ChainFamily::Evm,
        }
    }

    /// The address as it was claimed.
    pub fn address(&self) -> &str {
        match self {
            Self::Cardano { stake_address } => stake_address,
            Self::Evm { address } => address,
        }
    }

    /// Whether `wallet` is this account already linked. EVM addresses compare
    /// without case, as the server compares them.
    pub fn is(&self, wallet: &LinkedWallet) -> bool {
        match self {
            Self::Cardano { stake_address } => wallet.cardano_address() == Some(stake_address),
            Self::Evm { address } => wallet
                .evm_address()
                .is_some_and(|a| a.eq_ignore_ascii_case(address)),
        }
    }

    /// What the wallet is asked to sign for `message`, the challenge's text
    /// exactly as the server sent it. `None` for a Cardano claim whose stake
    /// address is not bech32 this crate can read.
    pub fn sign_request(&self, message: &str) -> Option<SignRequest> {
        let payload = hex(message.as_bytes());
        match self {
            Self::Cardano { stake_address } => Some(SignRequest::Cardano {
                address_hex: bech32_to_hex(stake_address)?,
                payload_hex: payload,
            }),
            Self::Evm { address } => Some(SignRequest::Evm {
                address: address.clone(),
                message_hex: format!("0x{payload}"),
            }),
        }
    }
}

/// A signature the wallet is asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignRequest {
    /// CIP-30 `signData(address_hex, payload_hex)`: the reward address as
    /// hex, the message's UTF-8 bytes as bare hex.
    Cardano {
        address_hex: String,
        payload_hex: String,
    },
    /// EIP-191 `personal_sign([message_hex, address])`, the message as
    /// `0x`-prefixed hex.
    Evm { address: String, message_hex: String },
}

/// What the wallet signed with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signature {
    /// CIP-8: the COSE_Sign1 and the COSE_Key, both hex.
    Cardano { signature: String, key: String },
    /// EIP-191: 65 bytes, hex, with or without `0x`.
    Evm { signature: String },
}

/// The verify body for `claim` signed with `signature`. `None` when the two
/// are for different chains.
pub(crate) fn verify_call(claim: &Claim, signature: &Signature) -> Option<crate::ApiCall> {
    match (claim, signature) {
        (Claim::Cardano { stake_address }, Signature::Cardano { signature, key }) => {
            Some(crate::ApiCall::VerifyCardano(CardanoLinkRequest {
                stake_address: stake_address.clone(),
                signature: signature.clone(),
                public_key: key.clone(),
            }))
        }
        (Claim::Evm { address }, Signature::Evm { signature }) => {
            let bare = signature
                .strip_prefix("0x")
                .or_else(|| signature.strip_prefix("0X"))
                .unwrap_or(signature);
            Some(crate::ApiCall::VerifyEvm(EvmLinkRequest {
                address: address.clone(),
                signature: bare.to_string(),
            }))
        }
        _ => None,
    }
}

/// Lower-case hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[usize::from(b >> 4)] as char);
        out.push(DIGITS[usize::from(b & 0x0f)] as char);
    }
    out
}

const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

fn polymod(values: impl Iterator<Item = u8>) -> u32 {
    const GEN: [u32; 5] = [0x3b6a_57b2, 0x2650_8e6d, 0x1ea1_19fa, 0x3d42_33dd, 0x2a14_62b3];
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

/// The bytes a bech32 string carries, as hex: a `stake1…` reward address
/// becomes the `e1…` header-and-hash CIP-30 `signData` takes. `None` for a
/// bad character, a mixed case, a short string or a checksum that fails.
///
/// Cardano addresses run past BIP-173's 90-character limit, so no length cap
/// is applied.
pub fn bech32_to_hex(s: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    if lower != s && s.to_ascii_uppercase() != s {
        return None;
    }
    let split = lower.rfind('1')?;
    let (hrp, data) = (&lower[..split], &lower[split + 1..]);
    if hrp.is_empty() || data.len() < 6 {
        return None;
    }
    let values: Vec<u8> = data
        .bytes()
        .map(|c| CHARSET.iter().position(|&x| x == c).map(|i| i as u8))
        .collect::<Option<_>>()?;
    let expanded = hrp
        .bytes()
        .map(|b| b >> 5)
        .chain([0])
        .chain(hrp.bytes().map(|b| b & 31));
    if polymod(expanded.chain(values.iter().copied())) != 1 {
        return None;
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
    // Leftover bits are padding, and must be zero.
    if bits >= 5 || (acc & ((1 << bits) - 1)) != 0 {
        return None;
    }
    Some(hex(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reward address the `wallet-link` tests use. Its bytes are a
    /// mainnet reward header (`e1`) and a 28-byte key hash.
    const STAKE: &str = "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9";

    #[test]
    fn a_reward_address_decodes_to_its_header_and_hash() {
        let hex = bech32_to_hex(STAKE).expect("valid bech32");
        assert_eq!(hex.len(), 58, "1 header byte + 28 hash bytes: {hex}");
        assert!(hex.starts_with("e1"), "mainnet reward header: {hex}");
    }

    #[test]
    fn a_damaged_address_is_refused() {
        let mut damaged = STAKE.to_string();
        damaged.replace_range(10..11, "q");
        assert_ne!(damaged, STAKE);
        assert_eq!(bech32_to_hex(&damaged), None, "checksum");
        assert_eq!(bech32_to_hex("stake1"), None, "no data");
        assert_eq!(bech32_to_hex("stake1bbbbbbbb"), None, "'b' is not bech32");
        assert_eq!(bech32_to_hex("Stake1u8962x"), None, "mixed case");
    }

    #[test]
    fn a_cardano_claim_signs_the_message_as_bare_hex_over_the_reward_address() {
        let claim = Claim::Cardano {
            stake_address: STAKE.into(),
        };
        let Some(SignRequest::Cardano {
            address_hex,
            payload_hex,
        }) = claim.sign_request("Hi\n")
        else {
            panic!("a Cardano request");
        };
        assert!(address_hex.starts_with("e1"));
        assert_eq!(payload_hex, "48690a");
    }

    #[test]
    fn an_evm_claim_signs_0x_hex_and_verifies_without_the_prefix() {
        let claim = Claim::Evm {
            address: "0xAbC".into(),
        };
        assert_eq!(
            claim.sign_request("Hi"),
            Some(SignRequest::Evm {
                address: "0xAbC".into(),
                message_hex: "0x4869".into()
            })
        );
        let call = verify_call(
            &claim,
            &Signature::Evm {
                signature: "0xdead".into(),
            },
        );
        assert_eq!(
            call,
            Some(crate::ApiCall::VerifyEvm(EvmLinkRequest {
                address: "0xAbC".into(),
                signature: "dead".into()
            }))
        );
    }

    #[test]
    fn an_evm_claim_matches_its_link_in_any_case() {
        let linked = LinkedWallet {
            chain: "eip155".into(),
            address: "0xabc".into(),
            handle: None,
        };
        assert!(
            Claim::Evm {
                address: "0xABC".into()
            }
            .is(&linked)
        );
        assert!(
            !Claim::Cardano {
                stake_address: "0xabc".into()
            }
            .is(&linked)
        );
    }
}
