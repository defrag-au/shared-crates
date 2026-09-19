//! LumpPad pool datum — `Constr 2` with eleven positional fields.
//!
//! Spec: `docs/design/LUMPPAD_INTEGRATION.md` §3. The state NFT wears a CIP-67
//! `(100)` label but this datum is pool state, NOT CIP-68 metadata; see §10
//! there before pointing any metadata reader at it.

use pallas_primitives::conway::PlutusData;
use pallas_primitives::{BigInt, Fragment};

use crate::builder::script::{bytes, constr_indef, encode_plutus_data, int};
use crate::error::TxBuildError;
use crate::route::leg::RouteError;

/// The constructor index LumpPad's pool datum uses (CBOR tag 123).
const POOL_DATUM_CONSTRUCTOR: u32 = 2;
/// Eleven positional fields.
const POOL_DATUM_FIELDS: usize = 11;

/// LumpPad pool state, as carried in the pool UTxO's inline datum.
///
/// The invariant observed on every state on chain: the UTxO's LUMP quantity
/// equals `reserve + platform_bucket + creator_bucket`. The fee buckets live
/// INSIDE the pool until someone runs Claim, so the LUMP balance is not the
/// curve reserve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolDatum {
    /// Policy of the token AND of the state NFT.
    pub token_policy: Vec<u8>,
    /// Raw asset name of the token.
    pub token_name: Vec<u8>,
    /// `R` — LUMP on the curve. Excludes both fee buckets.
    pub reserve: u64,
    /// `T` — tokens still in the pool. Always equals the UTxO's token quantity.
    pub tokens: u64,
    /// `A` — unclaimed platform fees (flat + `platform_fee_bps`).
    pub platform_bucket: u64,
    /// `B` — unclaimed creator fees (`creator_fee_bps`).
    pub creator_bucket: u64,
    /// Payment credential that receives `B` on Claim. The launcher's choice;
    /// SWOLE and HOSK-X pointed it at the burn address.
    pub creator_cred: Vec<u8>,
    /// Always the platform treasury; the mint policy enforces it.
    pub treasury_cred: Vec<u8>,
    pub creator_fee_bps: u64,
    pub platform_fee_bps: u64,
    /// Flat LUMP fee charged per trade, to the platform bucket.
    pub flat_fee: u64,
}

impl PoolDatum {
    /// Decode from the inline datum's CBOR.
    pub fn from_cbor(cbor: &[u8]) -> Result<Self, RouteError> {
        let data = PlutusData::decode_fragment(cbor)
            .map_err(|e| RouteError::Datum(format!("pool datum is not PlutusData: {e}")))?;
        Self::from_data(&data)
    }

    /// Decode from already-parsed PlutusData.
    pub fn from_data(data: &PlutusData) -> Result<Self, RouteError> {
        let fields = match data {
            PlutusData::Constr(c) if c.tag == 121 + POOL_DATUM_CONSTRUCTOR as u64 => &c.fields,
            PlutusData::Constr(c) => {
                return Err(RouteError::Datum(format!(
                    "expected a LumpPad pool datum (Constr {POOL_DATUM_CONSTRUCTOR}, CBOR tag \
                     {}), got CBOR tag {}",
                    121 + POOL_DATUM_CONSTRUCTOR,
                    c.tag
                )));
            }
            other => {
                return Err(RouteError::Datum(format!(
                    "expected a Constr, got {other:?}"
                )));
            }
        };

        if fields.len() != POOL_DATUM_FIELDS {
            return Err(RouteError::Datum(format!(
                "expected {POOL_DATUM_FIELDS} fields, got {}",
                fields.len()
            )));
        }

        Ok(Self {
            token_policy: field_bytes(&fields[0], "token_policy")?,
            token_name: field_bytes(&fields[1], "token_name")?,
            reserve: field_u64(&fields[2], "reserve")?,
            tokens: field_u64(&fields[3], "tokens")?,
            platform_bucket: field_u64(&fields[4], "platform_bucket")?,
            creator_bucket: field_u64(&fields[5], "creator_bucket")?,
            creator_cred: field_bytes(&fields[6], "creator_cred")?,
            treasury_cred: field_bytes(&fields[7], "treasury_cred")?,
            creator_fee_bps: field_u64(&fields[8], "creator_fee_bps")?,
            platform_fee_bps: field_u64(&fields[9], "platform_fee_bps")?,
            flat_fee: field_u64(&fields[10], "flat_fee")?,
        })
    }

    /// Encode back to PlutusData.
    ///
    /// Indefinite-length field array, because that is what LumpPad's own
    /// builder emits and what every pool UTxO on chain holds — see
    /// [`constr_indef`].
    pub fn to_data(&self) -> Result<PlutusData, RouteError> {
        Ok(constr_indef(
            POOL_DATUM_CONSTRUCTOR,
            vec![
                bytes(self.token_policy.clone()),
                bytes(self.token_name.clone()),
                datum_int(self.reserve, "reserve")?,
                datum_int(self.tokens, "tokens")?,
                datum_int(self.platform_bucket, "platform_bucket")?,
                datum_int(self.creator_bucket, "creator_bucket")?,
                bytes(self.creator_cred.clone()),
                bytes(self.treasury_cred.clone()),
                datum_int(self.creator_fee_bps, "creator_fee_bps")?,
                datum_int(self.platform_fee_bps, "platform_fee_bps")?,
                datum_int(self.flat_fee, "flat_fee")?,
            ],
        ))
    }

    /// Encode back to CBOR.
    pub fn to_cbor(&self) -> Result<Vec<u8>, RouteError> {
        let data = self.to_data()?;
        encode_plutus_data(&data).map_err(|e: TxBuildError| RouteError::Datum(e.to_string()))
    }

    /// The LUMP the pool UTxO must hold for this state: reserve plus both
    /// unclaimed fee buckets.
    pub fn lump_in_pool(&self) -> Result<u64, RouteError> {
        self.reserve
            .checked_add(self.platform_bucket)
            .and_then(|v| v.checked_add(self.creator_bucket))
            .ok_or(RouteError::Overflow)
    }

    /// The token's asset name as hex.
    pub fn token_name_hex(&self) -> String {
        hex::encode(&self.token_name)
    }

    /// The token's policy as hex.
    pub fn token_policy_hex(&self) -> String {
        hex::encode(&self.token_policy)
    }
}

fn field_bytes(data: &PlutusData, name: &'static str) -> Result<Vec<u8>, RouteError> {
    match data {
        PlutusData::BoundedBytes(b) => Ok(b.as_slice().to_vec()),
        other => Err(RouteError::Datum(format!(
            "{name}: expected bytes, got {other:?}"
        ))),
    }
}

fn field_u64(data: &PlutusData, name: &'static str) -> Result<u64, RouteError> {
    match data {
        PlutusData::BigInt(BigInt::Int(i)) => {
            let value: i128 = (*i).into();
            u64::try_from(value)
                .map_err(|_| RouteError::Datum(format!("{name}: {value} is not a u64")))
        }
        other => Err(RouteError::Datum(format!(
            "{name}: expected an integer, got {other:?}"
        ))),
    }
}

/// A datum integer, rejecting anything a Plutus `Int` cannot hold rather than
/// wrapping silently. Every field here is bounded well below this in practice
/// (supply is 1e9), so tripping it means the decode was wrong.
fn datum_int(value: u64, name: &'static str) -> Result<PlutusData, RouteError> {
    i64::try_from(value)
        .map(int)
        .map_err(|_| RouteError::Datum(format!("{name}: {value} exceeds a Plutus Int")))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The SWOLE pool's inline datum, read off mainnet UTxO
    /// `0aca3489a43d669b1aef62717933f141b38a940b9aa4923e180512c7a7b3ae6f#0`
    /// (Koios `asset_utxos` on `<policy>.000643b0504f4f4c`, 2026-09-14).
    pub(crate) const SWOLE_POOL_DATUM_CBOR: &str = "d87b9f581c023ea41eae25fd3d3042ec5c79b95fa287676eb3c99cd4bfd20fe6594553574f4c451a00317efa1a2d1520181a00028fb61a000127d4581c22c9a103ed3f2fa97c982d76d6e2af50c5d54ac306983b196c8fcdab581cc79844a83ab36100765fbc19fb8d738a0d46657708f6ad08c8c637f318641832192710ff";

    fn swole() -> PoolDatum {
        PoolDatum::from_cbor(&hex::decode(SWOLE_POOL_DATUM_CBOR).unwrap()).unwrap()
    }

    #[test]
    fn decodes_the_swole_pool_state() {
        let d = swole();
        assert_eq!(
            d.token_policy_hex(),
            "023ea41eae25fd3d3042ec5c79b95fa287676eb3c99cd4bfd20fe659"
        );
        assert_eq!(d.token_name_hex(), "53574f4c45");
        assert_eq!(String::from_utf8(d.token_name.clone()).unwrap(), "SWOLE");
        assert_eq!(d.reserve, 3_243_770);
        assert_eq!(d.tokens, 756_359_192);
        assert_eq!(d.platform_bucket, 167_862);
        assert_eq!(d.creator_bucket, 75_732);
        assert_eq!(
            hex::encode(&d.creator_cred),
            "22c9a103ed3f2fa97c982d76d6e2af50c5d54ac306983b196c8fcdab"
        );
        assert_eq!(
            hex::encode(&d.treasury_cred),
            "c79844a83ab36100765fbc19fb8d738a0d46657708f6ad08c8c637f3"
        );
        assert_eq!(d.creator_fee_bps, 100);
        assert_eq!(d.platform_fee_bps, 50);
        assert_eq!(d.flat_fee, 10_000);
    }

    /// The chain datum re-encodes BYTE-IDENTICALLY. LumpPad emits an
    /// indefinite-length field array; a definite-length re-encode would be the
    /// same value and a different byte string.
    #[test]
    fn round_trips_byte_for_byte() {
        let original = hex::decode(SWOLE_POOL_DATUM_CBOR).unwrap();
        let reencoded = PoolDatum::from_cbor(&original).unwrap().to_cbor().unwrap();
        assert_eq!(
            hex::encode(&reencoded),
            SWOLE_POOL_DATUM_CBOR,
            "re-encoding the chain's own datum must reproduce it exactly"
        );
        assert_eq!(reencoded, original);
    }

    /// `LUMP in UTxO == R + A + B` — the invariant every observed state holds.
    /// The SWOLE pool UTxO held 3,487,364 LUMP at this state.
    #[test]
    fn lump_in_pool_is_reserve_plus_both_buckets() {
        assert_eq!(swole().lump_in_pool().unwrap(), 3_487_364);
    }

    #[test]
    fn a_cip68_metadata_datum_is_not_a_pool_datum() {
        // `Constr 0 [..]` — what a real CIP-68 (100) token carries. The state
        // NFT wears the same label, so the decoder must reject rather than
        // mis-read it.
        let not_a_pool = crate::builder::script::constr(0, vec![int(1)]);
        assert!(matches!(
            PoolDatum::from_data(&not_a_pool),
            Err(RouteError::Datum(_))
        ));
    }

    #[test]
    fn a_short_field_list_is_rejected() {
        let truncated = constr_indef(POOL_DATUM_CONSTRUCTOR, vec![int(1), int(2)]);
        assert!(matches!(
            PoolDatum::from_data(&truncated),
            Err(RouteError::Datum(_))
        ));
    }
}
