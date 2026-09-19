//! Splash royalty-pool datum — `Constr 0` with fifteen positional fields.
//!
//! Spec: `LUMPPAD_INTEGRATION.md` §7.5. This is the DIRECTLY SPENDABLE pool,
//! not the spot-order datum in [`crate::dex::splash`], which describes an
//! order for a batcher to fill.
//!
//! The fields are kept as raw `PlutusData` and read through accessors, because
//! the validator's rule for a swap is that everything except the two counters
//! on the input side is **byte-identical**. Re-deriving a `DAOPolicy` list or
//! a royalty public key from typed fields is a way to get that subtly wrong
//! for no benefit — we never author these values, we only carry them.

use pallas_primitives::conway::PlutusData;
use pallas_primitives::{BigInt, Fragment};

use crate::builder::script::{constr_indef, encode_plutus_data, int};
use crate::route::leg::RouteError;

const FIELDS: usize = 15;

/// Positional field indices, named so the accessors below read as the spec
/// table does.
mod field {
    pub const POOL_NFT: usize = 0;
    pub const POOL_X: usize = 1;
    pub const POOL_Y: usize = 2;
    pub const FEE_NUM: usize = 4;
    pub const TREASURY_FEE: usize = 5;
    pub const ROYALTY_FEE: usize = 6;
    pub const TREASURY_X: usize = 7;
    pub const TREASURY_Y: usize = 8;
    pub const ROYALTY_X: usize = 9;
    pub const ROYALTY_Y: usize = 10;
}

/// An asset as the datum encodes it: `Constr 0 [policy, name]`, with ADA as
/// the empty policy and empty name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatumAssetRef {
    pub policy_id: Vec<u8>,
    pub asset_name: Vec<u8>,
}

impl DatumAssetRef {
    pub fn is_ada(&self) -> bool {
        self.policy_id.is_empty() && self.asset_name.is_empty()
    }

    pub fn policy_hex(&self) -> String {
        hex::encode(&self.policy_id)
    }

    pub fn name_hex(&self) -> String {
        hex::encode(&self.asset_name)
    }
}

/// A Splash royalty pool's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoyaltyPoolDatum {
    fields: Vec<PlutusData>,
}

impl RoyaltyPoolDatum {
    pub fn from_cbor(cbor: &[u8]) -> Result<Self, RouteError> {
        let data = PlutusData::decode_fragment(cbor)
            .map_err(|e| RouteError::Datum(format!("pool datum is not PlutusData: {e}")))?;
        Self::from_data(&data)
    }

    pub fn from_data(data: &PlutusData) -> Result<Self, RouteError> {
        let fields: Vec<PlutusData> = match data {
            PlutusData::Constr(c) if c.tag == 121 => c.fields.iter().cloned().collect(),
            other => {
                return Err(RouteError::Datum(format!(
                    "expected a Splash royalty-pool datum (Constr 0, CBOR tag 121), got {other:?}"
                )));
            }
        };
        if fields.len() != FIELDS {
            return Err(RouteError::Datum(format!(
                "expected {FIELDS} fields, got {}",
                fields.len()
            )));
        }
        Ok(Self { fields })
    }

    pub fn to_data(&self) -> PlutusData {
        constr_indef(0, self.fields.clone())
    }

    pub fn to_cbor(&self) -> Result<Vec<u8>, RouteError> {
        encode_plutus_data(&self.to_data()).map_err(|e| RouteError::Datum(e.to_string()))
    }

    pub fn pool_nft(&self) -> Result<DatumAssetRef, RouteError> {
        asset_field(&self.fields[field::POOL_NFT], "poolNft")
    }

    /// `poolX` — the asset a swap's X side names.
    pub fn pool_x(&self) -> Result<DatumAssetRef, RouteError> {
        asset_field(&self.fields[field::POOL_X], "poolX")
    }

    /// `poolY`.
    pub fn pool_y(&self) -> Result<DatumAssetRef, RouteError> {
        asset_field(&self.fields[field::POOL_Y], "poolY")
    }

    pub fn fee_num(&self) -> Result<u64, RouteError> {
        int_field(&self.fields[field::FEE_NUM], "feeNum")
    }

    pub fn treasury_fee(&self) -> Result<u64, RouteError> {
        int_field(&self.fields[field::TREASURY_FEE], "treasuryFee")
    }

    pub fn royalty_fee(&self) -> Result<u64, RouteError> {
        int_field(&self.fields[field::ROYALTY_FEE], "royaltyFee")
    }

    pub fn treasury_x(&self) -> Result<u64, RouteError> {
        int_field(&self.fields[field::TREASURY_X], "treasuryX")
    }

    pub fn treasury_y(&self) -> Result<u64, RouteError> {
        int_field(&self.fields[field::TREASURY_Y], "treasuryY")
    }

    pub fn royalty_x(&self) -> Result<u64, RouteError> {
        int_field(&self.fields[field::ROYALTY_X], "royaltyX")
    }

    pub fn royalty_y(&self) -> Result<u64, RouteError> {
        int_field(&self.fields[field::ROYALTY_Y], "royaltyY")
    }

    /// `f = feeNum − treasuryFee − royaltyFee`, the factor the constant
    /// product is evaluated at.
    pub fn swap_fee_num(&self) -> Result<u64, RouteError> {
        let treasury = self.treasury_fee()?;
        let royalty = self.royalty_fee()?;
        self.fee_num()?
            .checked_sub(treasury)
            .and_then(|v| v.checked_sub(royalty))
            .ok_or_else(|| {
                RouteError::Datum("pool fees exceed feeNum; datum is not usable".to_string())
            })
    }

    /// A copy with the two counters on one side bumped and EVERY other field
    /// byte-identical — the validator's own rule for a swap's new datum.
    pub fn with_counters_bumped(
        &self,
        side: CounterSide,
        treasury_delta: u64,
        royalty_delta: u64,
    ) -> Result<Self, RouteError> {
        let (treasury_ix, royalty_ix) = match side {
            CounterSide::X => (field::TREASURY_X, field::ROYALTY_X),
            CounterSide::Y => (field::TREASURY_Y, field::ROYALTY_Y),
        };
        let mut fields = self.fields.clone();
        fields[treasury_ix] = bumped(&fields[treasury_ix], treasury_delta, "treasury")?;
        fields[royalty_ix] = bumped(&fields[royalty_ix], royalty_delta, "royalty")?;
        Ok(Self { fields })
    }
}

/// Which side's counters a swap bumps: the side the INPUT arrived on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterSide {
    X,
    Y,
}

fn bumped(data: &PlutusData, delta: u64, name: &'static str) -> Result<PlutusData, RouteError> {
    let current = int_field(data, name)?;
    let next = current.checked_add(delta).ok_or(RouteError::Overflow)?;
    i64::try_from(next)
        .map(int)
        .map_err(|_| RouteError::Datum(format!("{name} counter {next} exceeds a Plutus Int")))
}

fn int_field(data: &PlutusData, name: &'static str) -> Result<u64, RouteError> {
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

fn asset_field(data: &PlutusData, name: &'static str) -> Result<DatumAssetRef, RouteError> {
    match data {
        PlutusData::Constr(c) if c.tag == 121 && c.fields.len() == 2 => {
            let take = |d: &PlutusData| match d {
                PlutusData::BoundedBytes(b) => Ok(b.as_slice().to_vec()),
                other => Err(RouteError::Datum(format!(
                    "{name}: expected bytes, got {other:?}"
                ))),
            };
            Ok(DatumAssetRef {
                policy_id: take(&c.fields[0])?,
                asset_name: take(&c.fields[1])?,
            })
        }
        other => Err(RouteError::Datum(format!(
            "{name}: expected Constr 0 [policy, name], got {other:?}"
        ))),
    }
}

/// Is this field list an indefinite-length CBOR array? Used by the round-trip
/// test to assert the assumption `to_data` bakes in.
#[cfg(test)]
pub(crate) fn is_indefinite(data: &PlutusData) -> bool {
    use pallas_primitives::MaybeIndefArray;
    matches!(data, PlutusData::Constr(c) if matches!(c.fields, MaybeIndefArray::Indef(_)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The LUMP/ADA royalty pool's inline datum, read off mainnet UTxO
    /// `d92955b626329b41222677f6d78e8feed16b6705c6885de00030b340b406efe7#1`
    /// on 2026-09-14 — the state `COMPOSED_ROUTES.md` §8.2 pins.
    pub(crate) const LUMP_ADA_POOL_DATUM_CBOR: &str = "d8799fd8799f581cd8eb52caf3289a2880288b23141ce3d2a7025dcf76f26fd5659add0658209f4408661725f5f141a17e75dd0982bbfe6f6053ae779d1fb74fec800e752b44ffd8799f4040ffd8799f581c73797786382c0832b5787a5b306f5308488f14571b7061f79396ad2c444c756d70ffd8799f581c6e917b8b965078a39804a6313e5be73535612421acd70aa83f0ec2005820a99ff37ddda0aaa9a1404e403bb605eb000f75cc7484a2bd7470870654c8c890ff1a0001831c183218321a08da22661a0010b3fb1a025382d71a00049b089fd8799fd87a9f581c66e711a4bf9ddf46ff239143870b6893055a4fd4dea9f99fed6665cdffffff581c75c4570eb625ae881b32a34c52b159f6f3f3f2c7aaabf5bac4688133582072c68f905716a5f59a0ee2552ab68559f42287d335396d8f430da98e96c5009c01ff";

    fn pool() -> RoyaltyPoolDatum {
        RoyaltyPoolDatum::from_cbor(&hex::decode(LUMP_ADA_POOL_DATUM_CBOR).unwrap()).unwrap()
    }

    #[test]
    fn decodes_the_lump_ada_pool() {
        let d = pool();
        assert!(d.pool_x().unwrap().is_ada(), "poolX is ADA");
        assert_eq!(
            d.pool_y().unwrap().policy_hex(),
            "73797786382c0832b5787a5b306f5308488f14571b7061f79396ad2c"
        );
        assert_eq!(d.pool_y().unwrap().name_hex(), "4c756d70");
        assert_eq!(
            d.pool_nft().unwrap().policy_hex(),
            "d8eb52caf3289a2880288b23141ce3d2a7025dcf76f26fd5659add06"
        );
        assert_eq!(d.fee_num().unwrap(), 99_100);
        assert_eq!(d.treasury_fee().unwrap(), 50);
        assert_eq!(d.royalty_fee().unwrap(), 50);
        assert_eq!(d.swap_fee_num().unwrap(), 99_000);
        // The §8.2 snapshot.
        assert_eq!(d.treasury_x().unwrap(), 148_513_382);
        assert_eq!(d.treasury_y().unwrap(), 1_094_651);
        assert_eq!(d.royalty_x().unwrap(), 39_027_415);
        assert_eq!(d.royalty_y().unwrap(), 301_832);
    }

    #[test]
    fn round_trips_byte_for_byte() {
        let original = hex::decode(LUMP_ADA_POOL_DATUM_CBOR).unwrap();
        let reencoded = RoyaltyPoolDatum::from_cbor(&original)
            .unwrap()
            .to_cbor()
            .unwrap();
        assert_eq!(hex::encode(&reencoded), LUMP_ADA_POOL_DATUM_CBOR);
    }

    /// `to_data` emits an indefinite-length field array. Assert the chain's
    /// datum really is one, so the assumption is checked rather than assumed.
    #[test]
    fn the_chains_datum_is_indefinite_length() {
        let data =
            PlutusData::decode_fragment(&hex::decode(LUMP_ADA_POOL_DATUM_CBOR).unwrap()).unwrap();
        assert!(is_indefinite(&data));
    }

    /// Bumping the X counters leaves EVERY other byte alone — the validator's
    /// rule for a swap's new datum.
    #[test]
    fn bumping_counters_changes_only_those_counters() {
        let before = pool();
        let after = before
            .with_counters_bumped(CounterSide::X, 25_500, 25_500)
            .unwrap();

        assert_eq!(after.treasury_x().unwrap(), 148_513_382 + 25_500);
        assert_eq!(after.royalty_x().unwrap(), 39_027_415 + 25_500);
        // The Y-side counters are untouched for an X→Y swap.
        assert_eq!(after.treasury_y().unwrap(), before.treasury_y().unwrap());
        assert_eq!(after.royalty_y().unwrap(), before.royalty_y().unwrap());
        // Everything else, including the DAO policy list and the royalty key.
        assert_eq!(after.pool_nft().unwrap(), before.pool_nft().unwrap());
        assert_eq!(after.fee_num().unwrap(), before.fee_num().unwrap());
        for ix in [3usize, 11, 12, 13, 14] {
            assert_eq!(
                encode_plutus_data(&after.fields[ix]).unwrap(),
                encode_plutus_data(&before.fields[ix]).unwrap(),
                "field {ix} must be byte-identical"
            );
        }
    }

    #[test]
    fn a_lumppad_datum_is_not_a_splash_datum() {
        let lumppad =
            hex::decode(crate::route::lumppad::datum::tests::SWOLE_POOL_DATUM_CBOR).unwrap();
        assert!(matches!(
            RoyaltyPoolDatum::from_cbor(&lumppad),
            Err(RouteError::Datum(_))
        ));
    }
}
