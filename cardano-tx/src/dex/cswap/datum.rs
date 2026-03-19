//! CSWAP concentrated liquidity order datum construction
//!
//! Builds the Plutus datum for a CSWAP swap order. The datum is a
//! Constr 0 with 6 fields matching the on-chain order validator.
//!
//! Reverse-engineered from on-chain transactions.

use pallas_primitives::alonzo::{BigInt, Constr, PlutusData};
use pallas_primitives::{Fragment, MaybeIndefArray};

use super::CswapError;

/// Convert u64 to PlutusData BigInt
fn bigint(value: u64) -> PlutusData {
    PlutusData::BigInt(BigInt::Int((value as i64).into()))
}

/// Parameters for building a CSWAP order datum
pub struct CswapOrderParams {
    /// User's payment public key hash (28 bytes)
    pub payment_pkh: Vec<u8>,
    /// User's stake public key hash (28 bytes, optional)
    pub stake_pkh: Option<Vec<u8>>,
    /// Minimum assets the user must receive.
    /// Each entry: (policy_id_bytes, asset_name_bytes, quantity)
    pub min_receive: Vec<(Vec<u8>, Vec<u8>, u64)>,
    /// Asset being sold + remainder (0 = sell all).
    /// Each entry: (policy_id_bytes, asset_name_bytes, remainder)
    pub sell_asset: Vec<(Vec<u8>, Vec<u8>, u64)>,
    /// Execution parameter (1000 for buys, varies for sells)
    pub execution_param: u64,
    /// Fee tier (always 15)
    pub fee_tier: u64,
}

/// Build the Plutus address datum (same encoding as Splash).
///
/// Address = Constr 0 { payment_cred, stake_cred }
/// payment_cred = Constr 0 { pkh_bytes }  (pub key credential)
/// stake_cred = Constr 0 { Constr 0 { Constr 0 { pkh } } }  (Some, pub key)
///            | Constr 1 {}  (None)
fn build_address_datum(payment_pkh: &[u8], stake_pkh: Option<&[u8]>) -> PlutusData {
    let payment_cred = PlutusData::Constr(Constr {
        tag: 121, // Constr 0
        any_constructor: None,
        fields: MaybeIndefArray::Def(vec![PlutusData::BoundedBytes(payment_pkh.to_vec().into())]),
    });

    let stake_cred = match stake_pkh {
        Some(pkh) => {
            let inner_key = PlutusData::Constr(Constr {
                tag: 121,
                any_constructor: None,
                fields: MaybeIndefArray::Def(vec![PlutusData::BoundedBytes(pkh.to_vec().into())]),
            });
            let staking_hash = PlutusData::Constr(Constr {
                tag: 121,
                any_constructor: None,
                fields: MaybeIndefArray::Def(vec![inner_key]),
            });
            PlutusData::Constr(Constr {
                tag: 121,
                any_constructor: None,
                fields: MaybeIndefArray::Def(vec![staking_hash]),
            })
        }
        None => PlutusData::Constr(Constr {
            tag: 122, // Constr 1 = Nothing
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![]),
        }),
    };

    PlutusData::Constr(Constr {
        tag: 121,
        any_constructor: None,
        fields: MaybeIndefArray::Def(vec![payment_cred, stake_cred]),
    })
}

/// Build an asset list entry: [policy_id, asset_name, quantity]
fn build_asset_entry(policy_id: &[u8], asset_name: &[u8], quantity: u64) -> PlutusData {
    PlutusData::Array(MaybeIndefArray::Indef(vec![
        PlutusData::BoundedBytes(policy_id.to_vec().into()),
        PlutusData::BoundedBytes(asset_name.to_vec().into()),
        bigint(quantity),
    ]))
}

/// Build the full CSWAP order PlutusData from parameters.
///
/// Constructor 0, 6 fields:
///   [0] destination address
///   [1] min_receive: list of [policy, name, amount]
///   [2] sell_asset: list of [policy, name, remainder]
///   [3] Constructor 0 {} (market order type)
///   [4] execution_param (integer)
///   [5] fee_tier (integer)
pub fn build_cswap_order_datum(params: &CswapOrderParams) -> PlutusData {
    // [0] destination address
    let destination = build_address_datum(&params.payment_pkh, params.stake_pkh.as_deref());

    // [1] min_receive list
    let min_receive_entries: Vec<PlutusData> = params
        .min_receive
        .iter()
        .map(|(policy, name, qty)| build_asset_entry(policy, name, *qty))
        .collect();
    let min_receive = PlutusData::Array(MaybeIndefArray::Indef(min_receive_entries));

    // [2] sell_asset list
    let sell_asset_entries: Vec<PlutusData> = params
        .sell_asset
        .iter()
        .map(|(policy, name, remainder)| build_asset_entry(policy, name, *remainder))
        .collect();
    let sell_asset = PlutusData::Array(MaybeIndefArray::Indef(sell_asset_entries));

    // [3] order type — Constructor 0 {} (market swap)
    let order_type = PlutusData::Constr(Constr {
        tag: 121,
        any_constructor: None,
        fields: MaybeIndefArray::Def(vec![]),
    });

    PlutusData::Constr(Constr {
        tag: 121, // Constr 0
        any_constructor: None,
        fields: MaybeIndefArray::Def(vec![
            destination,
            min_receive,
            sell_asset,
            order_type,
            bigint(params.execution_param),
            bigint(params.fee_tier),
        ]),
    })
}

/// Encode a CSWAP order datum to CBOR bytes
pub fn encode_datum(params: &CswapOrderParams) -> Result<Vec<u8>, CswapError> {
    let datum = build_cswap_order_datum(params);
    datum
        .encode_fragment()
        .map_err(|e| CswapError::CborEncoding(format!("{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_buy_order_datum() {
        // BUY: ADA -> Aliens
        let params = CswapOrderParams {
            payment_pkh: hex::decode("8fc751e6882866e2a113d7e31a55e8eb2721f70d953eecbfc5b48aa2")
                .unwrap(),
            stake_pkh: Some(
                hex::decode("38220e3d6473be31a145f81eac6c32fd71231da373ff9ea07de72b2f").unwrap(),
            ),
            min_receive: vec![
                // 2 ADA min UTxO
                (vec![], vec![], 2_000_000),
                // min 405,535 Aliens
                (
                    hex::decode("16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb7")
                        .unwrap(),
                    hex::decode("416c69656e73").unwrap(),
                    405_535,
                ),
            ],
            sell_asset: vec![
                // Selling ADA, 0 = sell all
                (vec![], vec![], 0),
            ],
            execution_param: 1000,
            fee_tier: 15,
        };

        let datum = build_cswap_order_datum(&params);

        // Verify it's Constr 0 with 6 fields
        match &datum {
            PlutusData::Constr(c) => {
                assert_eq!(c.tag, 121);
                assert_eq!(c.fields.len(), 6);
            }
            _ => panic!("expected Constr"),
        }

        // Verify we can encode to CBOR
        let cbor = datum.encode_fragment().unwrap();
        assert!(!cbor.is_empty());
        // Should start with d879 (Constr 0 tag)
        assert_eq!(cbor[0], 0xd8);
        assert_eq!(cbor[1], 0x79);
    }

    #[test]
    fn test_build_sell_order_datum() {
        // SELL: Aliens -> ADA
        let params = CswapOrderParams {
            payment_pkh: hex::decode("8fc751e6882866e2a113d7e31a55e8eb2721f70d953eecbfc5b48aa2")
                .unwrap(),
            stake_pkh: None,
            min_receive: vec![
                // min ADA to receive
                (vec![], vec![], 50_000_000),
            ],
            sell_asset: vec![
                // Selling Aliens, 0 = sell all
                (
                    hex::decode("16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb7")
                        .unwrap(),
                    hex::decode("416c69656e73").unwrap(),
                    0,
                ),
            ],
            execution_param: 5,
            fee_tier: 15,
        };

        let cbor = encode_datum(&params).unwrap();
        assert!(!cbor.is_empty());
    }

    #[test]
    fn test_address_datum_with_stake_key() {
        let datum = build_address_datum(
            &hex::decode("8fc751e6882866e2a113d7e31a55e8eb2721f70d953eecbfc5b48aa2").unwrap(),
            Some(&hex::decode("38220e3d6473be31a145f81eac6c32fd71231da373ff9ea07de72b2f").unwrap()),
        );

        // Should be Constr 0 with 2 fields
        match &datum {
            PlutusData::Constr(c) => {
                assert_eq!(c.tag, 121);
                assert_eq!(c.fields.len(), 2);
            }
            _ => panic!("expected Constr"),
        }
    }

    #[test]
    fn test_address_datum_without_stake_key() {
        let datum = build_address_datum(
            &hex::decode("8fc751e6882866e2a113d7e31a55e8eb2721f70d953eecbfc5b48aa2").unwrap(),
            None,
        );

        match &datum {
            PlutusData::Constr(c) => {
                assert_eq!(c.tag, 121);
                assert_eq!(c.fields.len(), 2);
                // Second field should be Constr 1 (Nothing)
                match &c.fields[1] {
                    PlutusData::Constr(inner) => assert_eq!(inner.tag, 122),
                    _ => panic!("expected Constr 1 for None stake"),
                }
            }
            _ => panic!("expected Constr"),
        }
    }
}
