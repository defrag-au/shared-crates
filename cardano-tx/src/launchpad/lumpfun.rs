//! LumpFun v5 — the "ada-order" cohort.
//!
//! A v5 buy is ONE signature and ONE plain payment. The wallet pays
//! `curve_amount + 2% + settler_fee + 1_500_000` to the cohort's shared order
//! address with the datum below; a settler then spends that UTxO into the
//! curve, at whatever the curve gives then and never under `min_out`.
//!
//! ⚠️ **The order address is shared by every trader.** Its payment part is
//! `23c870a9…`, a script with no stake part, so a movement through it cannot
//! name who placed the order — the datum does. Register it as a shared order
//! contract, never as a per-customer one.
//!
//! ⚠️ **`V_ADA` is the COMPILED offset**, never one recovered from a pool
//! state. The validator floors every trade, so a recovered offset sits a few
//! hundred lovelace high; the validator recomputes a quote with the compiled
//! value, so a `min_out` derived from the recovered one is a number the chain
//! disagrees with. The difference is far below a percent — but it is not zero,
//! and this is the side that builds.
//!
//! Measured on `$BUNNY` (Dust Bunny), the first v5 launch. The facts, the
//! offsets and the measured order are in
//! `cnft.dev-workers/docs/design/LUMPFUN_V5.md`; the golden datum in this
//! file's tests is the order from `2dcc4d22…#0`, copied from the chain.

use pallas_addresses::Address;
use pallas_crypto::hash::{Hash, Hasher};
use pallas_primitives::alonzo::{BigInt, Constr, PlutusData};
use pallas_primitives::{Fragment, MaybeIndefArray};

/// The cohort's shared order address (mainnet). Payment part only — no stake
/// part, which is why the order datum has to name the buyer.
pub const ORDER_ADDRESS: &str = "addr1wy3usu9fdmcngw3cccdq64nvn5uazvx4864p2972jtmqadqtseffg";

/// The same address's payment credential.
pub const ORDER_CRED: [u8; 28] = [
    0x23, 0xc8, 0x70, 0xa9, 0x6e, 0xf1, 0x34, 0x3a, 0x38, 0xc6, 0x1a, 0x0d, 0x56, 0x6c, 0x9d, 0x39,
    0xd1, 0x30, 0xd5, 0x3e, 0xaa, 0x15, 0x17, 0xca, 0x92, 0xf6, 0x0e, 0xb4,
];

/// [`ORDER_CRED`] in the hex form a wire type and a review row use. A test
/// holds the two together; a second literal would eventually disagree.
pub const ORDER_CRED_HEX: &str = "23c870a96ef1343a38c61a0d566c9d39d130d53eaa1517ca92f60eb4";

/// `V_ada` compiled into the v5 validator for this cohort, lovelace.
pub const V_ADA: i64 = 5_000_000_000;

/// The one-shot supply every v5 token is minted with.
pub const SUPPLY: i64 = 1_000_000_000;

/// The ADA the order funds for the buyer's token UTxO, which the settler
/// creates. It is part of the order's value, not a fee.
pub const TOKEN_OUTPUT_MIN_LOVELACE: u64 = 1_500_000;

/// The settler's tip on the measured order; `POST /api/tx/build` names it
/// `settlerFee`.
pub const DEFAULT_SETTLER_FEE: u64 = 450_000;

/// Errors from building a LumpFun order.
#[derive(Debug, thiserror::Error)]
pub enum LumpFunError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("CBOR encoding failed: {0}")]
    CborEncoding(String),
    #[error("address error: {0}")]
    Address(String),
}

/// The curve, as an index read it — `ada_reserve` net of the pool's own
/// min-ADA and of the two accrued fee buckets, `token_reserve` already virtual.
///
/// Deliberately NOT a datum decoder: one already exists on the ledger side,
/// and a second definition here is where the two would drift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Curve {
    pub ada_reserve: i64,
    pub token_reserve: i64,
}

/// Parameters for a v5 buy order.
pub struct OrderRequest {
    /// The buyer's payment key hash — the fill pays this credential.
    pub owner_payment_cred: [u8; 28],
    /// The buyer's stake key hash.
    ///
    /// ⚠️ Required rather than optional: the measured order carries
    /// `Constr 0 [stake_cred]`, and the shape the venue writes for a wallet
    /// WITHOUT one has never been observed. Refusing is the only honest
    /// answer; encoding a guess would produce an order nobody has proven the
    /// validator accepts.
    pub owner_stake_cred: [u8; 28],
    /// The token's policy id.
    pub policy: [u8; 28],
    /// On-chain asset name, 1..=32 bytes.
    pub asset_name: Vec<u8>,
    /// Lovelace that reaches the curve. The two fees are ON TOP — see
    /// [`order_value`], which is what the wallet actually pays.
    pub curve_amount: u64,
    /// The least the buyer will accept; the fill pays more when the curve
    /// moves in their favour and never less.
    pub min_out: u64,
    /// The settler's tip, inside the order's value.
    pub settler_fee: u64,
}

/// A built order: the datum, its hash, and the output the caller places.
pub struct Order {
    /// CBOR-encoded datum bytes.
    pub datum_bytes: Vec<u8>,
    /// PlutusData for inclusion in the TX witness set.
    pub datum_plutus_data: PlutusData,
    /// Blake2b-256 hash of the datum CBOR.
    pub datum_hash: Hash<32>,
    /// The cohort's shared order address.
    pub script_address: Address,
    /// Lovelace the order UTxO holds.
    pub total_lovelace: u64,
}

/// The platform and creator fee on a curve amount: 1% each, floored, which is
/// how the validator floors them. `$BUNNY`'s measured 200 ADA order carried
/// exactly 2 ADA + 2 ADA.
pub fn fees(curve_amount: u64) -> u64 {
    2 * (curve_amount / 100)
}

/// What the order UTxO holds: the curve amount, both 1% fees, the settler's
/// tip, and the buyer's token output. `None` when it overflows.
///
/// Measured: a 200 ADA curve amount and a 0.45 ADA tip make `205_950_000`.
pub fn order_value(curve_amount: u64, settler_fee: u64) -> Option<u64> {
    curve_amount
        .checked_add(fees(curve_amount))?
        .checked_add(settler_fee)?
        .checked_add(TOKEN_OUTPUT_MIN_LOVELACE)
}

/// Tokens a buy of `curve_amount` receives, floored — the formula the
/// validator applies: a constant product over `(ada_reserve + V_ADA,
/// token_reserve)`.
///
/// `None` before the curve's first buy, where there is no reserve to price
/// against. That is undefined, not zero: a preview that showed "0 tokens"
/// would be a different claim from "no price yet".
pub fn quote(curve: &Curve, curve_amount: u64) -> Option<u64> {
    quote_effective(
        curve.ada_reserve.checked_add(V_ADA)?,
        curve.token_reserve,
        curve_amount,
    )
}

/// The same arithmetic against EFFECTIVE reserves — the pair the token ledger
/// publishes as `(token_reserve, ada_reserve + V_ADA)`.
///
/// Use this when the reserves came from a snapshot rather than a datum; a
/// caller that HAS the datum must use [`quote`], because the validator
/// recomputes the quote with the compiled offset.
pub fn quote_effective(ada_effective: i64, token_reserve: i64, curve_amount: u64) -> Option<u64> {
    if ada_effective <= 0 || token_reserve <= 0 {
        return None;
    }
    let g = i128::from(curve_amount);
    let numerator = i128::from(token_reserve) * g;
    let denominator = i128::from(ada_effective) + g;
    u64::try_from(numerator / denominator).ok()
}

/// Build a v5 buy order.
pub fn build_order(req: &OrderRequest) -> Result<Order, LumpFunError> {
    if req.asset_name.is_empty() || req.asset_name.len() > 32 {
        return Err(LumpFunError::InvalidInput(format!(
            "an asset name is 1..=32 bytes, got {}",
            req.asset_name.len()
        )));
    }
    if req.curve_amount == 0 {
        return Err(LumpFunError::InvalidInput(
            "curve_amount is zero — a v5 buy is an ADA amount, not a token amount".to_string(),
        ));
    }
    let total_lovelace = order_value(req.curve_amount, req.settler_fee)
        .ok_or_else(|| LumpFunError::InvalidInput("the order value overflows u64".to_string()))?;

    let datum_plutus_data = build_order_datum(req);
    let datum_bytes = datum_plutus_data
        .encode_fragment()
        .map_err(|e| LumpFunError::CborEncoding(format!("{e}")))?;
    let datum_hash = Hasher::<256>::hash(&datum_bytes);
    let script_address = Address::from_bech32(ORDER_ADDRESS)
        .map_err(|e| LumpFunError::Address(format!("the order address does not decode: {e}")))?;

    Ok(Order {
        datum_bytes,
        datum_plutus_data,
        datum_hash,
        script_address,
        total_lovelace,
    })
}

/// The order as a transaction output: the value at the shared address, with
/// the datum inline.
pub fn order_output(order: &Order) -> pallas_txbuilder::Output {
    super::order_output(
        order.script_address.clone(),
        order.total_lovelace,
        &order.datum_bytes,
    )
}

/// Constructor 0, eight fields — the measured `2dcc4d22…#0`:
///
/// ```text
/// Constr 0 [
///   owner_payment_cred,                 // bytes(28)
///   Constr 0 [ owner_stake_cred ],      // bytes(28)
///   policy_id,                          // bytes(28)
///   asset_name,                         // bytes
///   Constr 0 [],                        // Buy
///   curve_amount,                       // 200_000_000
///   min_out,                            // 25_663_636
///   settler_fee,                        // 450_000
/// ]
/// ```
///
/// A sell is `Constr 1` by inference in the design note and is NOT verified on
/// chain, so this module builds buys only.
fn build_order_datum(req: &OrderRequest) -> PlutusData {
    constr(
        121,
        vec![
            PlutusData::BoundedBytes(req.owner_payment_cred.to_vec().into()),
            constr(
                121,
                vec![PlutusData::BoundedBytes(
                    req.owner_stake_cred.to_vec().into(),
                )],
            ),
            PlutusData::BoundedBytes(req.policy.to_vec().into()),
            PlutusData::BoundedBytes(req.asset_name.clone().into()),
            constr(121, vec![]),
            bigint(req.curve_amount),
            bigint(req.min_out),
            bigint(req.settler_fee),
        ],
    )
}

/// A constructor encoded the way the venue's own order is: fields as an
/// INDEFINITE array (`d8799f … ff`), and an empty constructor definite
/// (`d87980`). Any encoding decodes to the same data, but the settler reads
/// datums this code wrote, and matching the measured order byte for byte is
/// the one encoding certain to be read.
fn constr(tag: u64, fields: Vec<PlutusData>) -> PlutusData {
    PlutusData::Constr(Constr {
        tag,
        any_constructor: None,
        fields: if fields.is_empty() {
            MaybeIndefArray::Def(fields)
        } else {
            MaybeIndefArray::Indef(fields)
        },
    })
}

fn bigint(value: u64) -> PlutusData {
    PlutusData::BigInt(BigInt::Int((value as i64).into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The order `2dcc4d22…#0`, copied from the chain rather than rebuilt.
    /// Same bytes as the decoder crate's is-not-a-curve-datum test.
    const MEASURED_ORDER: &str = "d8799f581c4a00e5040c2d7e201a9c20744ace64bf28d1cda55999a4931e406922d8799f581ccba51a2e5b5b802a0402a87b762d2ecc6b1a9b6d0dd708daf07b82adff581ca1b085f414fb7799d378e90e39d24c9e54c841a311d72b2f57cba2934542554e4e59d879801a0bebc2001a018798941a0006ddd0ff";

    fn bunny_request(curve_amount: u64, min_out: u64) -> OrderRequest {
        OrderRequest {
            owner_payment_cred: hex::decode(
                "4a00e5040c2d7e201a9c20744ace64bf28d1cda55999a4931e406922",
            )
            .unwrap()
            .try_into()
            .unwrap(),
            owner_stake_cred: hex::decode(
                "cba51a2e5b5b802a0402a87b762d2ecc6b1a9b6d0dd708daf07b82ad",
            )
            .unwrap()
            .try_into()
            .unwrap(),
            policy: hex::decode("a1b085f414fb7799d378e90e39d24c9e54c841a311d72b2f57cba293")
                .unwrap()
                .try_into()
                .unwrap(),
            asset_name: hex::decode("42554e4e59").unwrap(),
            curve_amount,
            min_out,
            settler_fee: DEFAULT_SETTLER_FEE,
        }
    }

    /// The encoded datum is the chain's, byte for byte. A validator that
    /// accepts the venue's own orders must accept ours, and this is the test
    /// that says so rather than "it round-trips through our own encoder".
    #[test]
    fn the_measured_order_datum_is_reproduced_byte_for_byte() {
        let order = build_order(&bunny_request(200_000_000, 25_663_636)).unwrap();
        assert_eq!(hex::encode(&order.datum_bytes), MEASURED_ORDER);
    }

    /// The measured order's value, component by component:
    /// 200,000,000 curve + 2,000,000 platform + 2,000,000 creator +
    /// 450,000 settler + 1,500,000 token output.
    #[test]
    fn the_measured_order_holds_205_950_000_lovelace() {
        assert_eq!(fees(200_000_000), 4_000_000);
        assert_eq!(order_value(200_000_000, 450_000), Some(205_950_000));
    }

    /// The published formula against the fill state: a 5 ADA buy gets
    /// 654,253 tokens, matching `POST /api/tx/build` to the unit. The curve
    /// amount here is the ADA that reaches the curve, not the wallet's spend.
    #[test]
    fn the_documented_quote_reproduces_654_253_tokens() {
        let fill_state = Curve {
            ada_reserve: 1_555_000_000,
            token_reserve: 858_381_090,
        };
        assert_eq!(quote(&fill_state, 5_000_000), Some(654_253));
        // The ledger publishes the effective pair, so a snapshot taken from it
        // must price identically to the datum plus the compiled offset.
        assert_eq!(
            quote_effective(1_555_000_000 + V_ADA, 858_381_090, 5_000_000),
            Some(654_253)
        );
    }

    /// The hex and the bytes are one fact in two spellings, and the wire type
    /// uses the first while the builder uses the second.
    #[test]
    fn the_order_credential_has_one_spelling() {
        assert_eq!(hex::encode(ORDER_CRED), ORDER_CRED_HEX);
    }

    /// Before the first buy there is no reserve to price against. `None`,
    /// never `0` — "no price yet" and "worth nothing" are different claims.
    #[test]
    fn a_curve_with_no_first_buy_has_no_quote() {
        let launch_state = Curve {
            ada_reserve: 0,
            token_reserve: 1_000_000_000,
        };
        assert_eq!(quote(&launch_state, 5_000_000), None);
    }

    /// The address in the docs is the credential in the docs — an explorer
    /// building an order from a copied constant is the one place a typo would
    /// send money to a contract nobody watches.
    #[test]
    fn the_order_address_carries_the_documented_credential() {
        let Address::Shelley(shelley) = Address::from_bech32(ORDER_ADDRESS).expect("decodes")
        else {
            panic!("a script address is a Shelley address")
        };
        assert_eq!(shelley.payment().to_hex(), hex::encode(ORDER_CRED));
        assert!(
            shelley.payment().is_script(),
            "the order contract is a script, not a wallet"
        );
        assert!(
            matches!(
                shelley.delegation(),
                pallas_addresses::ShelleyDelegationPart::Null
            ),
            "the shared order address has no stake part"
        );
    }

    #[test]
    fn a_zero_curve_amount_is_refused() {
        assert!(matches!(
            build_order(&bunny_request(0, 1)),
            Err(LumpFunError::InvalidInput(_))
        ));
    }

    /// A 33-byte name cannot be minted on Cardano, so an order carrying one
    /// could never fill. Refuse it at build time, where the message is useful.
    #[test]
    fn an_asset_name_that_cannot_exist_is_refused() {
        let mut req = bunny_request(2_000_000, 1);
        req.asset_name = vec![0x41; 33];
        assert!(matches!(
            build_order(&req),
            Err(LumpFunError::InvalidInput(_))
        ));
    }
}
