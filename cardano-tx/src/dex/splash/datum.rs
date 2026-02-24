//! Splash DEX SpotOrder datum construction
//!
//! Builds the Plutus datum for a Splash spot order (swap).
//! The datum is a Constr 0 with 12 fields matching the on-chain validator.
//!
//! Reference: protocol-sdk spotOrderDatum.ts

use pallas_primitives::alonzo::{BigInt, Constr, PlutusData};
use pallas_primitives::{Fragment, MaybeIndefArray};

use super::SplashError;

/// Convert u64 to PlutusData BigInt (via i64 cast — pallas Int only implements From<i64>)
fn bigint(value: u64) -> PlutusData {
    PlutusData::BigInt(BigInt::Int((value as i64).into()))
}

/// Asset identifier for the datum (policy_id + asset_name as raw bytes)
pub struct DatumAsset {
    pub policy_id: Vec<u8>,
    pub asset_name: Vec<u8>,
}

impl DatumAsset {
    /// ADA (empty policy and name)
    pub fn ada() -> Self {
        Self {
            policy_id: vec![],
            asset_name: vec![],
        }
    }

    /// Native token from hex-encoded policy_id and asset_name
    pub fn from_hex(policy_id_hex: &str, asset_name_hex: &str) -> Result<Self, SplashError> {
        Ok(Self {
            policy_id: hex::decode(policy_id_hex)
                .map_err(|e| SplashError::InvalidInput(format!("bad policy_id hex: {e}")))?,
            asset_name: hex::decode(asset_name_hex)
                .map_err(|e| SplashError::InvalidInput(format!("bad asset_name hex: {e}")))?,
        })
    }

    fn to_plutus_data(&self) -> PlutusData {
        PlutusData::Constr(Constr {
            tag: 121,
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![
                PlutusData::BoundedBytes(self.policy_id.clone().into()),
                PlutusData::BoundedBytes(self.asset_name.clone().into()),
            ]),
        })
    }
}

/// Rational price (numerator / denominator)
pub struct RationalPrice {
    pub numerator: u64,
    pub denominator: u64,
}

impl RationalPrice {
    fn to_plutus_data(&self) -> PlutusData {
        PlutusData::Constr(Constr {
            tag: 121,
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![bigint(self.numerator), bigint(self.denominator)]),
        })
    }
}

/// Parameters for building a SpotOrder datum
pub struct SpotOrderParams {
    pub beacon: Vec<u8>,
    pub input_asset: DatumAsset,
    pub input_amount: u64,
    pub cost_per_ex_step: u64,
    pub min_marginal_output: u64,
    pub output_asset: DatumAsset,
    pub price: RationalPrice,
    pub executor_fee: u64,
    /// User's payment public key hash (28 bytes)
    pub payment_pkh: Vec<u8>,
    /// User's stake public key hash (28 bytes, optional)
    pub stake_pkh: Option<Vec<u8>>,
    /// Public key hash allowed to cancel the order (usually same as payment_pkh)
    pub cancel_pkh: Vec<u8>,
    /// List of permitted executor (batcher) public key hashes
    pub permitted_executors: Vec<Vec<u8>>,
}

/// Build the address sub-datum (Constr 0 with payment + stake credentials)
///
/// Matches the Splash SDK's address encoding:
/// - Payment: Constr 0 { Constr 0 { pkh } }  (pub key credential)
/// - Stake:   Constr 0 { Constr 0 { Constr 0 { pkh } } }  (pub key, wrapped)
/// - or Constr 1 {}  (no stake credential)
fn build_address_datum(payment_pkh: &[u8], stake_pkh: Option<&[u8]>) -> PlutusData {
    // paymentCredentials = Constr 0 { paymentKeyHash }
    let payment_cred = PlutusData::Constr(Constr {
        tag: 121, // Constr 0
        any_constructor: None,
        fields: MaybeIndefArray::Def(vec![PlutusData::BoundedBytes(payment_pkh.to_vec().into())]),
    });

    let stake_cred = match stake_pkh {
        Some(pkh) => {
            // Constr 0 { Constr 0 { Constr 0 { pkh } } }
            // Innermost: Constr 0 { pkh } — the key hash credential
            let inner_key = PlutusData::Constr(Constr {
                tag: 121,
                any_constructor: None,
                fields: MaybeIndefArray::Def(vec![PlutusData::BoundedBytes(pkh.to_vec().into())]),
            });
            // Middle: Constr 0 { inner_key } — StakingHash wrapper
            let staking_hash = PlutusData::Constr(Constr {
                tag: 121,
                any_constructor: None,
                fields: MaybeIndefArray::Def(vec![inner_key]),
            });
            // Outer: Constr 0 { staking_hash } — Some wrapper
            PlutusData::Constr(Constr {
                tag: 121,
                any_constructor: None,
                fields: MaybeIndefArray::Def(vec![staking_hash]),
            })
        }
        None => {
            // Constr 1 {} — Nothing / no stake credential
            PlutusData::Constr(Constr {
                tag: 122,
                any_constructor: None,
                fields: MaybeIndefArray::Def(vec![]),
            })
        }
    };

    // address = Constr 0 { paymentCredentials, stakeCredentials }
    PlutusData::Constr(Constr {
        tag: 121,
        any_constructor: None,
        fields: MaybeIndefArray::Def(vec![payment_cred, stake_cred]),
    })
}

/// Build the full SpotOrder PlutusData from parameters
pub fn build_spot_order_datum(params: &SpotOrderParams) -> PlutusData {
    let executor_list: Vec<PlutusData> = params
        .permitted_executors
        .iter()
        .map(|pkh| PlutusData::BoundedBytes(pkh.clone().into()))
        .collect();

    PlutusData::Constr(Constr {
        tag: 121, // Constr 0
        any_constructor: None,
        fields: MaybeIndefArray::Def(vec![
            // type: always 0x00
            PlutusData::BoundedBytes(vec![0x00].into()),
            // beacon: 28 bytes
            PlutusData::BoundedBytes(params.beacon.clone().into()),
            // inputAsset
            params.input_asset.to_plutus_data(),
            // inputAmount
            bigint(params.input_amount),
            // costPerExStep
            bigint(params.cost_per_ex_step),
            // minMarginalOutput
            bigint(params.min_marginal_output),
            // outputAsset
            params.output_asset.to_plutus_data(),
            // price
            params.price.to_plutus_data(),
            // executorFee
            bigint(params.executor_fee),
            // address
            build_address_datum(&params.payment_pkh, params.stake_pkh.as_deref()),
            // cancelPkh
            PlutusData::BoundedBytes(params.cancel_pkh.clone().into()),
            // permittedExecutors
            PlutusData::Array(MaybeIndefArray::Def(executor_list)),
        ]),
    })
}

/// Encode a SpotOrder datum to CBOR bytes
pub fn encode_datum(params: &SpotOrderParams) -> Result<Vec<u8>, SplashError> {
    let datum = build_spot_order_datum(params);
    datum
        .encode_fragment()
        .map_err(|e| SplashError::CborEncoding(format!("{e}")))
}

/// Encode a SpotOrder datum to CBOR hex string
pub fn encode_datum_hex(params: &SpotOrderParams) -> Result<String, SplashError> {
    encode_datum(params).map(hex::encode)
}

// ============================================================================
// Datum decoding (inverse of build_spot_order_datum)
// ============================================================================

/// Decoded Splash spot order datum fields
#[derive(Debug, Clone)]
pub struct DecodedSpotOrder {
    pub order_type: Vec<u8>,
    pub beacon: Vec<u8>,
    pub input_policy_id: Vec<u8>,
    pub input_asset_name: Vec<u8>,
    pub input_amount: u64,
    pub cost_per_ex_step: u64,
    pub min_marginal_output: u64,
    pub output_policy_id: Vec<u8>,
    pub output_asset_name: Vec<u8>,
    pub price_numerator: u64,
    pub price_denominator: u64,
    pub executor_fee: u64,
    pub payment_pkh: Vec<u8>,
    pub stake_pkh: Option<Vec<u8>>,
    pub cancel_pkh: Vec<u8>,
    pub permitted_executors: Vec<Vec<u8>>,
}

impl DecodedSpotOrder {
    /// Format an asset as a human-readable string
    fn format_asset(policy_id: &[u8], asset_name: &[u8]) -> String {
        if policy_id.is_empty() && asset_name.is_empty() {
            "lovelace (ADA)".to_string()
        } else {
            let policy = hex::encode(policy_id);
            let name = hex::encode(asset_name);
            // Try to show ASCII name if it's printable
            let name_display = if asset_name.iter().all(|b| b.is_ascii_graphic()) {
                format!("{name} \"{}\"", String::from_utf8_lossy(asset_name))
            } else {
                name
            };
            format!("{policy}.{name_display}")
        }
    }

    /// Pretty-print the decoded datum
    pub fn display(&self) -> String {
        let input_asset = Self::format_asset(&self.input_policy_id, &self.input_asset_name);
        let output_asset = Self::format_asset(&self.output_policy_id, &self.output_asset_name);
        let price_decimal = if self.price_denominator > 0 {
            self.price_numerator as f64 / self.price_denominator as f64
        } else {
            0.0
        };
        let executors: Vec<String> = self.permitted_executors.iter().map(hex::encode).collect();

        let mut out = String::new();
        out.push_str(&format!(
            "  type:                {}\n",
            hex::encode(&self.order_type)
        ));
        out.push_str(&format!(
            "  beacon:              {}\n",
            hex::encode(&self.beacon)
        ));
        out.push_str(&format!("  input asset:         {input_asset}\n"));
        out.push_str(&format!(
            "  input amount:        {} ({:.6} ADA)\n",
            self.input_amount,
            self.input_amount as f64 / 1_000_000.0
        ));
        out.push_str(&format!(
            "  cost per ex step:    {} ({:.6} ADA)\n",
            self.cost_per_ex_step,
            self.cost_per_ex_step as f64 / 1_000_000.0
        ));
        out.push_str(&format!(
            "  min marginal output: {}\n",
            self.min_marginal_output
        ));
        out.push_str(&format!("  output asset:        {output_asset}\n"));
        out.push_str(&format!(
            "  price:               {}/{} (= {price_decimal})\n",
            self.price_numerator, self.price_denominator
        ));
        out.push_str(&format!(
            "  executor fee:        {} ({:.6} ADA)\n",
            self.executor_fee,
            self.executor_fee as f64 / 1_000_000.0
        ));
        out.push_str(&format!(
            "  payment pkh:         {}\n",
            hex::encode(&self.payment_pkh)
        ));
        match &self.stake_pkh {
            Some(pkh) => out.push_str(&format!("  stake pkh:           {}\n", hex::encode(pkh))),
            None => out.push_str("  stake pkh:           (none)\n"),
        }
        out.push_str(&format!(
            "  cancel pkh:          {}\n",
            hex::encode(&self.cancel_pkh)
        ));
        out.push_str(&format!(
            "  permitted executors: [{}]\n",
            executors.join(", ")
        ));
        out
    }
}

/// Decode a Splash spot order datum from PlutusData.
///
/// Expects Constr 0 (tag 121) with exactly 12 fields matching
/// the structure produced by `build_spot_order_datum`.
pub fn decode_spot_order_datum(data: &PlutusData) -> Result<DecodedSpotOrder, SplashError> {
    let fields = match data {
        PlutusData::Constr(constr) if constr.tag == 121 => &constr.fields,
        _ => {
            return Err(SplashError::InvalidInput(
                "expected Constr 0 (tag 121)".into(),
            ))
        }
    };

    if fields.len() != 12 {
        return Err(SplashError::InvalidInput(format!(
            "expected 12 fields, got {}",
            fields.len()
        )));
    }

    let order_type = extract_bytes(&fields[0])?;
    let beacon = extract_bytes(&fields[1])?;
    let (input_policy_id, input_asset_name) = extract_asset(&fields[2])?;
    let input_amount = extract_bigint(&fields[3])?;
    let cost_per_ex_step = extract_bigint(&fields[4])?;
    let min_marginal_output = extract_bigint(&fields[5])?;
    let (output_policy_id, output_asset_name) = extract_asset(&fields[6])?;
    let (price_numerator, price_denominator) = extract_rational(&fields[7])?;
    let executor_fee = extract_bigint(&fields[8])?;
    let (payment_pkh, stake_pkh) = extract_address(&fields[9])?;
    let cancel_pkh = extract_bytes(&fields[10])?;
    let permitted_executors = extract_executor_list(&fields[11])?;

    Ok(DecodedSpotOrder {
        order_type,
        beacon,
        input_policy_id,
        input_asset_name,
        input_amount,
        cost_per_ex_step,
        min_marginal_output,
        output_policy_id,
        output_asset_name,
        price_numerator,
        price_denominator,
        executor_fee,
        payment_pkh,
        stake_pkh,
        cancel_pkh,
        permitted_executors,
    })
}

/// Extract a u64 from a PlutusData BigInt
fn extract_bigint(data: &PlutusData) -> Result<u64, SplashError> {
    match data {
        PlutusData::BigInt(BigInt::Int(int)) => {
            let val: i128 = (*int).into();
            if val >= 0 && val <= u64::MAX as i128 {
                Ok(val as u64)
            } else {
                Err(SplashError::InvalidInput(format!(
                    "bigint out of u64 range: {val}"
                )))
            }
        }
        _ => Err(SplashError::InvalidInput(format!(
            "expected BigInt, got {data:?}"
        ))),
    }
}

/// Extract raw bytes from a PlutusData BoundedBytes
fn extract_bytes(data: &PlutusData) -> Result<Vec<u8>, SplashError> {
    match data {
        PlutusData::BoundedBytes(bytes) => Ok(bytes.as_slice().to_vec()),
        _ => Err(SplashError::InvalidInput(format!(
            "expected BoundedBytes, got {data:?}"
        ))),
    }
}

/// Extract (policy_id, asset_name) from a Constr 0 with 2 BoundedBytes fields
fn extract_asset(data: &PlutusData) -> Result<(Vec<u8>, Vec<u8>), SplashError> {
    match data {
        PlutusData::Constr(constr) if constr.tag == 121 && constr.fields.len() == 2 => {
            let policy_id = extract_bytes(&constr.fields[0])?;
            let asset_name = extract_bytes(&constr.fields[1])?;
            Ok((policy_id, asset_name))
        }
        _ => Err(SplashError::InvalidInput(
            "expected asset Constr(121, [bytes, bytes])".into(),
        )),
    }
}

/// Extract (numerator, denominator) from a Constr 0 with 2 BigInt fields
fn extract_rational(data: &PlutusData) -> Result<(u64, u64), SplashError> {
    match data {
        PlutusData::Constr(constr) if constr.tag == 121 && constr.fields.len() == 2 => {
            let num = extract_bigint(&constr.fields[0])?;
            let den = extract_bigint(&constr.fields[1])?;
            Ok((num, den))
        }
        _ => Err(SplashError::InvalidInput(
            "expected rational Constr(121, [int, int])".into(),
        )),
    }
}

/// Extract (payment_pkh, Option<stake_pkh>) from the address datum structure.
///
/// Address = Constr 0 { payment_cred, stake_cred }
/// payment_cred = Constr 0 { pkh_bytes }
/// stake_cred = Constr 0 { Constr 0 { Constr 0 { pkh_bytes } } } (Some)
///            | Constr 1 {}  (None)
fn extract_address(data: &PlutusData) -> Result<(Vec<u8>, Option<Vec<u8>>), SplashError> {
    let addr_fields = match data {
        PlutusData::Constr(constr) if constr.tag == 121 && constr.fields.len() == 2 => {
            &constr.fields
        }
        _ => {
            return Err(SplashError::InvalidInput(
                "expected address Constr(121, [payment, stake])".into(),
            ))
        }
    };

    // payment_cred = Constr 0 { pkh_bytes }
    let payment_pkh = match &addr_fields[0] {
        PlutusData::Constr(constr) if constr.tag == 121 && constr.fields.len() == 1 => {
            extract_bytes(&constr.fields[0])?
        }
        _ => {
            return Err(SplashError::InvalidInput(
                "expected payment Constr(121, [bytes])".into(),
            ))
        }
    };

    // stake_cred: Constr 0 { Constr 0 { Constr 0 { pkh } } } or Constr 1 {}
    let stake_pkh = match &addr_fields[1] {
        PlutusData::Constr(constr) if constr.tag == 122 => None, // Constr 1 = Nothing
        PlutusData::Constr(outer) if outer.tag == 121 && outer.fields.len() == 1 => {
            // Constr 0 { Constr 0 { Constr 0 { pkh } } }
            match &outer.fields[0] {
                PlutusData::Constr(mid) if mid.tag == 121 && mid.fields.len() == 1 => {
                    match &mid.fields[0] {
                        PlutusData::Constr(inner)
                            if inner.tag == 121 && inner.fields.len() == 1 =>
                        {
                            Some(extract_bytes(&inner.fields[0])?)
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        _ => None,
    };

    Ok((payment_pkh, stake_pkh))
}

/// Extract list of executor PKH bytes from a PlutusData Array
fn extract_executor_list(data: &PlutusData) -> Result<Vec<Vec<u8>>, SplashError> {
    match data {
        PlutusData::Array(arr) => arr.iter().map(extract_bytes).collect(),
        _ => Err(SplashError::InvalidInput(
            "expected Array for permitted executors".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test vector from Splash Protocol SDK (spotOrderDatum.spec.ts)
    #[test]
    fn test_datum_matches_sdk_test_vector() {
        let expected_cbor = "d8798c4100581c73fc8e44a4c04433c4e5870982e7d94867e9be28e501bff03e4ac0cfd87982581cfb4f75d1ad4eb5c21efd5a32a90c076e63a79daccf25afe4ccd4f7144824504f50534e454b1a000f3c391a000dbba01a0ee0cfaad879824040d879821903e80100d87982d87981581c74104cd5ca6288c1dd2e22ee5c874fdcfc1b81897462d91153496430d87981d87981d87981581cde7866fe5068ebf3c87dcdb568da528da5dcb5f659d9b60010e7450f581c74104cd5ca6288c1dd2e22ee5c874fdcfc1b81897462d9115349643081581c5cb2c968e5d1c7197a6ce7615967310a375545d9bc65063a964335b2";

        let params = SpotOrderParams {
            beacon: hex::decode("73fc8e44a4c04433c4e5870982e7d94867e9be28e501bff03e4ac0cf")
                .unwrap(),
            input_asset: DatumAsset::from_hex(
                "fb4f75d1ad4eb5c21efd5a32a90c076e63a79daccf25afe4ccd4f714",
                "24504f50534e454b",
            )
            .unwrap(),
            input_amount: 998457,
            cost_per_ex_step: 900000,
            min_marginal_output: 249614250,
            output_asset: DatumAsset::ada(),
            price: RationalPrice {
                numerator: 1000,
                denominator: 1,
            },
            executor_fee: 0,
            payment_pkh: hex::decode("74104cd5ca6288c1dd2e22ee5c874fdcfc1b81897462d91153496430")
                .unwrap(),
            stake_pkh: Some(
                hex::decode("de7866fe5068ebf3c87dcdb568da528da5dcb5f659d9b60010e7450f").unwrap(),
            ),
            cancel_pkh: hex::decode("74104cd5ca6288c1dd2e22ee5c874fdcfc1b81897462d91153496430")
                .unwrap(),
            permitted_executors: vec![hex::decode(
                "5cb2c968e5d1c7197a6ce7615967310a375545d9bc65063a964335b2",
            )
            .unwrap()],
        };

        let result = encode_datum_hex(&params).unwrap();
        assert_eq!(result, expected_cbor);
    }

    #[test]
    fn test_ada_asset_encoding() {
        let ada = DatumAsset::ada();
        let plutus = ada.to_plutus_data();
        let bytes = plutus.encode_fragment().unwrap();
        // Constr 0 { empty_bytes, empty_bytes } = d87982 40 40
        assert_eq!(hex::encode(bytes), "d8798240".to_owned() + "40");
    }

    /// Roundtrip test: encode a datum then decode it, verify all fields match
    #[test]
    fn test_decode_roundtrip() {
        let params = SpotOrderParams {
            beacon: hex::decode("73fc8e44a4c04433c4e5870982e7d94867e9be28e501bff03e4ac0cf")
                .unwrap(),
            input_asset: DatumAsset::from_hex(
                "fb4f75d1ad4eb5c21efd5a32a90c076e63a79daccf25afe4ccd4f714",
                "24504f50534e454b",
            )
            .unwrap(),
            input_amount: 998457,
            cost_per_ex_step: 900000,
            min_marginal_output: 249614250,
            output_asset: DatumAsset::ada(),
            price: RationalPrice {
                numerator: 1000,
                denominator: 1,
            },
            executor_fee: 0,
            payment_pkh: hex::decode("74104cd5ca6288c1dd2e22ee5c874fdcfc1b81897462d91153496430")
                .unwrap(),
            stake_pkh: Some(
                hex::decode("de7866fe5068ebf3c87dcdb568da528da5dcb5f659d9b60010e7450f").unwrap(),
            ),
            cancel_pkh: hex::decode("74104cd5ca6288c1dd2e22ee5c874fdcfc1b81897462d91153496430")
                .unwrap(),
            permitted_executors: vec![hex::decode(
                "5cb2c968e5d1c7197a6ce7615967310a375545d9bc65063a964335b2",
            )
            .unwrap()],
        };

        // Encode to PlutusData, then decode
        let plutus = build_spot_order_datum(&params);
        let decoded = decode_spot_order_datum(&plutus).unwrap();

        assert_eq!(decoded.input_amount, 998457);
        assert_eq!(decoded.cost_per_ex_step, 900000);
        assert_eq!(decoded.min_marginal_output, 249614250);
        assert_eq!(decoded.price_numerator, 1000);
        assert_eq!(decoded.price_denominator, 1);
        assert_eq!(decoded.executor_fee, 0);
        assert_eq!(
            hex::encode(&decoded.input_policy_id),
            "fb4f75d1ad4eb5c21efd5a32a90c076e63a79daccf25afe4ccd4f714"
        );
        assert_eq!(hex::encode(&decoded.input_asset_name), "24504f50534e454b");
        assert!(decoded.output_policy_id.is_empty()); // ADA
        assert!(decoded.output_asset_name.is_empty());
        assert_eq!(
            hex::encode(&decoded.payment_pkh),
            "74104cd5ca6288c1dd2e22ee5c874fdcfc1b81897462d91153496430"
        );
        assert_eq!(
            decoded.stake_pkh.as_ref().map(hex::encode),
            Some("de7866fe5068ebf3c87dcdb568da528da5dcb5f659d9b60010e7450f".to_string())
        );
        assert_eq!(
            hex::encode(&decoded.cancel_pkh),
            "74104cd5ca6288c1dd2e22ee5c874fdcfc1b81897462d91153496430"
        );
        assert_eq!(decoded.permitted_executors.len(), 1);
        assert_eq!(
            hex::encode(&decoded.permitted_executors[0]),
            "5cb2c968e5d1c7197a6ce7615967310a375545d9bc65063a964335b2"
        );
    }
}
