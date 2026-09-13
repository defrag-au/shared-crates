//! The scalar newtypes of schema §2.8.
//!
//! Every one of these is a bytestring on chain and a **hex string** in JSON.
//! They are newtypes rather than aliases because a policy id and a script
//! hash are both 28 bytes and swapping them is the kind of mistake a type
//! system should be made to catch — the registry's own history has an
//! instance of exactly that (see `reference_stake_registry_shared_credentials`).
//!
//! `Address` is **raw address bytes, never bech32 text** (§2.4): a datum is
//! read by a validator that has no bech32 decoder.

use serde::{Deserialize, Serialize};

use crate::codec::{Bytes, DecodeError, PlutusCodec, as_bytes};
use pallas_primitives::PlutusData;

/// Declare a fixed-width byte newtype with both codecs.
macro_rules! hash_newtype {
    ($(#[$meta:meta])* $name:ident, $width:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub [u8; $width]);

        impl $name {
            pub const WIDTH: usize = $width;

            pub const fn as_bytes(&self) -> &[u8; $width] {
                &self.0
            }

            pub fn to_hex(&self) -> String {
                hex::encode(self.0)
            }

            /// Parse from hex, checking the width.
            pub fn from_hex(text: &str) -> Result<Self, DecodeError> {
                let bytes = hex::decode(text).map_err(|_| DecodeError::BadLength {
                    target: stringify!($name),
                    expected: $width,
                    actual: text.len() / 2,
                })?;
                Self::from_slice(&bytes)
            }

            pub fn from_slice(bytes: &[u8]) -> Result<Self, DecodeError> {
                <[u8; $width]>::try_from(bytes)
                    .map(Self)
                    .map_err(|_| DecodeError::BadLength {
                        target: stringify!($name),
                        expected: $width,
                        actual: bytes.len(),
                    })
            }
        }

        impl PlutusCodec for $name {
            fn to_data(&self) -> PlutusData {
                self.0.to_data()
            }

            fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
                Self::from_slice(as_bytes(data)?)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.to_hex())
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.to_hex())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = String::deserialize(d)?;
                Self::from_hex(&text).map_err(serde::de::Error::custom)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self([0u8; $width])
            }
        }
    };
}

hash_newtype!(
    /// A minting policy id.
    PolicyId,
    28
);
hash_newtype!(
    /// A Plutus script hash.
    ScriptHash,
    28
);
hash_newtype!(
    /// A payment key hash — what `extra_signatories` carries, and what the
    /// definition envelope's `owner` is.
    PaymentKeyHash,
    28
);
hash_newtype!(
    /// A transaction id. A definition's identity is its creating tx hash.
    TxHash,
    32
);
hash_newtype!(
    /// An opaque route id issued at registration (schema §6 Q3).
    ///
    /// **Never a Discord channel id and never a webhook URL.** A datum is
    /// public forever; the binding from this id to a destination lives in
    /// the worker and is revocable.
    RouteRef,
    16
);
hash_newtype!(
    /// The 16 random bytes the burn builder mints per claim.
    ClaimId,
    16
);

/// A raw Cardano address, as bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Address(pub Bytes);

impl Address {
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0.as_slice())
    }
}

impl From<Vec<u8>> for Address {
    fn from(bytes: Vec<u8>) -> Self {
        Self(Bytes(bytes))
    }
}

impl PlutusCodec for Address {
    fn to_data(&self) -> PlutusData {
        self.0.to_data()
    }

    fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
        Bytes::from_data(data).map(Self)
    }
}

/// An address in the shape **a validator can compare**: Plutus's own
/// `Address`, which is what `output.address` is inside a script.
///
/// [`Address`] above is raw ledger bytes, which is right for a trigger's
/// sink (the validator never compares it — it is matched off chain) and
/// wrong for a claim's `recipient`, which `escrow.ak` compares against a
/// transaction output on the settlement path. Raw bytes and a Plutus
/// `Address` are **not** the same encoding, and the only ways to bridge
/// them in a validator are to hand-parse the header byte or to store the
/// structured form. This is the structured form.
///
/// ```text
/// Constr 0 [
///   Constr 0|1 [payment hash],                     -- key | script
///   Constr 0 [Constr 0 [Constr 0|1 [stake hash]]]  -- Some(StakingHash(c))
///   | Constr 1 []                                  -- None
/// ]
/// ```
///
/// **The network is not part of this.** The Plutus form does not carry one,
/// so neither does this type: a field that cannot survive `to_data` would
/// make two equal addresses compare unequal after a round trip. It is
/// context — every address in one deployment is on one network — and
/// [`Self::to_bytes`] takes it as an argument.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainAddress {
    pub payment: crate::types::fuel::Credential,
    #[serde(default)]
    pub stake: Option<crate::types::fuel::Credential>,
}

impl ChainAddress {
    /// Split a raw address into its credentials, discarding the network.
    ///
    /// Rejects anything that is not a base or enterprise address — pointer
    /// addresses and reward accounts are not places a prize is paid, and
    /// accepting one here would mean encoding something `to_bytes` could
    /// not rebuild.
    pub fn from_bytes(raw: &[u8]) -> Result<Self, DecodeError> {
        let header = *raw.first().ok_or(DecodeError::BadLength {
            target: "ChainAddress",
            expected: 29,
            actual: 0,
        })?;
        let kind = header >> 4;
        let payment_is_script = kind & 0b0001 != 0;
        let (has_stake, stake_is_script) = match kind {
            0b0000 | 0b0001 => (true, false),
            0b0010 | 0b0011 => (true, true),
            0b0110 | 0b0111 => (false, false),
            _ => {
                return Err(DecodeError::WrongShape {
                    expected: "a base or enterprise address",
                    found: "address",
                });
            }
        };
        let expected = if has_stake { 57 } else { 29 };
        if raw.len() != expected {
            return Err(DecodeError::BadLength {
                target: "ChainAddress",
                expected,
                actual: raw.len(),
            });
        }
        let credential = |bytes: &[u8], is_script: bool| {
            let hash = Bytes::from(bytes.to_vec());
            if is_script {
                crate::types::fuel::Credential::Script { hash }
            } else {
                crate::types::fuel::Credential::Key { hash }
            }
        };
        Ok(Self {
            payment: credential(&raw[1..29], payment_is_script),
            stake: has_stake.then(|| credential(&raw[29..57], stake_is_script)),
        })
    }

    /// Rebuild the raw ledger address on a given network.
    pub fn to_bytes(&self, network: u8) -> Vec<u8> {
        let kind = match (self.payment.is_script(), &self.stake) {
            (false, Some(s)) if !s.is_script() => 0b0000,
            (true, Some(s)) if !s.is_script() => 0b0001,
            (false, Some(_)) => 0b0010,
            (true, Some(_)) => 0b0011,
            (false, None) => 0b0110,
            (true, None) => 0b0111,
        };
        let mut bytes = vec![(kind << 4) | (network & 0x0f)];
        bytes.extend_from_slice(self.payment.hash().as_slice());
        if let Some(stake) = &self.stake {
            bytes.extend_from_slice(stake.hash().as_slice());
        }
        bytes
    }
}

impl PlutusCodec for ChainAddress {
    fn to_data(&self) -> PlutusData {
        let stake = match &self.stake {
            // Some(StakingHash(credential)) — two wrappers, both the
            // ledger's, neither ours to simplify away.
            Some(credential) => {
                crate::codec::constr(0, vec![crate::codec::constr(0, vec![credential.to_data()])])
            }
            None => crate::codec::constr(1, vec![]),
        };
        crate::codec::constr(0, vec![self.payment.to_data(), stake])
    }

    fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
        let fields = crate::codec::read_constr_zero(data, 2)?;
        let payment = crate::types::fuel::Credential::from_data(&fields[0])?;
        let (index, outer) = crate::codec::read_constr(&fields[1])?;
        let stake = match index {
            1 => None,
            0 => {
                let inner = crate::codec::read_constr_zero(
                    outer.first().ok_or(DecodeError::ShortEnvelope {
                        expected: 1,
                        actual: 0,
                    })?,
                    1,
                )?;
                Some(crate::types::fuel::Credential::from_data(&inner[0])?)
            }
            found => return Err(DecodeError::WrongConstructor { expected: 0, found }),
        };
        Ok(Self { payment, stake })
    }
}

/// `policy_id ‖ asset_name` as one bytestring — the concatenated form every
/// Cardano API uses, so nothing has to re-join it.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetId(pub Bytes);

impl AssetId {
    pub fn new(policy: PolicyId, name: &[u8]) -> Self {
        let mut bytes = policy.0.to_vec();
        bytes.extend_from_slice(name);
        Self(Bytes(bytes))
    }

    /// The policy half. `None` when the value is too short to hold one —
    /// which a decoded `AssetId` never is, but a hand-built one can be.
    pub fn policy(&self) -> Option<PolicyId> {
        PolicyId::from_slice(self.0.as_slice().get(..PolicyId::WIDTH)?).ok()
    }

    /// The asset-name half, which may be empty.
    pub fn name(&self) -> &[u8] {
        self.0.as_slice().get(PolicyId::WIDTH..).unwrap_or(&[])
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0.as_slice())
    }
}

impl PlutusCodec for AssetId {
    fn to_data(&self) -> PlutusData {
        self.0.to_data()
    }

    fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
        let bytes = Bytes::from_data(data)?;
        // 28 bytes of policy plus 0..32 of name — a value that cannot be an
        // asset id is a decode failure, not something to discover later.
        let len = bytes.len();
        if !(PolicyId::WIDTH..=PolicyId::WIDTH + 32).contains(&len) {
            return Err(DecodeError::BadLength {
                target: "AssetId",
                expected: PolicyId::WIDTH,
                actual: len,
            });
        }
        Ok(Self(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pallas_primitives::Fragment;

    #[test]
    fn a_hash_newtype_round_trips_through_data_and_json() {
        let policy = PolicyId([7u8; 28]);
        let data = policy.to_data();
        assert_eq!(PolicyId::from_data(&data).unwrap(), policy);

        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(json, format!("\"{}\"", "07".repeat(28)));
        assert_eq!(serde_json::from_str::<PolicyId>(&json).unwrap(), policy);
    }

    #[test]
    fn a_wrong_width_is_refused_on_both_codecs() {
        let short = Bytes::from(vec![0u8; 27]).to_data();
        assert!(matches!(
            PolicyId::from_data(&short),
            Err(DecodeError::BadLength {
                expected: 28,
                actual: 27,
                ..
            })
        ));
        assert!(serde_json::from_str::<PolicyId>("\"0102\"").is_err());
    }

    #[test]
    fn an_asset_id_splits_into_policy_and_name() {
        let policy = PolicyId([1u8; 28]);
        let asset = AssetId::new(policy, b"SNEK");
        assert_eq!(asset.policy(), Some(policy));
        assert_eq!(asset.name(), b"SNEK");

        let data = asset.to_data();
        assert_eq!(AssetId::from_data(&data).unwrap(), asset);
        assert_eq!(data.encode_fragment().unwrap().len(), 2 + 28 + 4);
    }

    #[test]
    fn an_asset_id_with_no_name_is_valid() {
        let asset = AssetId::new(PolicyId([2u8; 28]), b"");
        assert_eq!(asset.name(), b"");
        assert_eq!(AssetId::from_data(&asset.to_data()).unwrap(), asset);
    }

    #[test]
    fn a_too_short_asset_id_is_a_decode_failure() {
        let short = Bytes::from(vec![0u8; 10]).to_data();
        assert!(matches!(
            AssetId::from_data(&short),
            Err(DecodeError::BadLength {
                target: "AssetId",
                ..
            })
        ));
    }

    #[test]
    fn an_address_is_raw_bytes_never_bech32() {
        // A mainnet enterprise address's raw form: header byte + 28.
        let raw = vec![0x61u8]
            .into_iter()
            .chain([9u8; 28])
            .collect::<Vec<u8>>();
        let address = Address::from(raw.clone());
        assert_eq!(address.as_slice(), raw.as_slice());
        assert_eq!(Address::from_data(&address.to_data()).unwrap(), address);
    }
}
