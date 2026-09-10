//! Seller-side marketplace builders: list, cancel, update.
//!
//! The buy path (`builder::buy`) spends a listing; this is the other half —
//! creating one, and taking it back. Both speak the jpg V2/V3 datum, which the
//! abandonware validator shares:
//!
//! ```text
//! Datum = Constr 0 [ payouts: [ Constr 0 [ Address, amount_lovelace: Int ] ], owner: pkh ]
//! Address = Constr 0 [ Constr 0|1 [ pkh|script ], Option<Inline(Credential)> ]
//! ```
//!
//! # Inline datums, on purpose
//!
//! jpg listed with hash datums and published the preimage off chain, which is
//! why buying a jpg listing means recovering a datum from metadata or a
//! superseded UTxO first. Ours are inline: the listing UTxO carries its own
//! datum, every indexer can read the price, and the buy path witnesses
//! nothing. The min-UTxO is a few hundred thousand lovelace higher; the seller
//! gets that back on sale or cancel.
//!
//! # What the datum promises
//!
//! `payouts` is what the validator will make the buyer pay, in order. The
//! marketplace fee is NOT in it — the validator computes that from the sum
//! and demands it as a separate output. A single seller payout is the whole
//! story for abandonware: no royalty line, by design.
//!
//! # Change is ADA-only in the fluent builder
//!
//! [`TxBuilder`] returns leftover lovelace to the seller but knows nothing
//! about assets, so a listing that spends a UTxO carrying OTHER assets must
//! explicitly return them, or the transaction fails `ValueNotConservedUTxO`.
//! [`build_list`] does that.

use address_registry::{MarketplaceType, ScriptReference};
use cardano_assets::{AssetId, UtxoApi};
use pallas_addresses::{Address, ShelleyDelegationPart, ShelleyPaymentPart};
use pallas_crypto::hash::Hash;
use pallas_primitives::conway::PlutusData;
use pallas_txbuilder::{ExUnits, Input};

use crate::builder::fluent::TxBuilder;
use crate::builder::marketplace::{DatumPayout, ParsedListing};
use crate::builder::script::{
    CollateralConfig, ScriptInput, ScriptSource, bytes, constr, encode_plutus_data, int, list,
};
use crate::builder::{TxDeps, UnsignedTx};
use crate::error::TxBuildError;
use crate::helpers::decode::{decode_asset_name, decode_policy_id, decode_tx_hash};
use crate::helpers::output::create_ada_output;
use crate::utxo::{AssetAmount, OutputParams, min_ada_with_coefficient};

/// Starting budgets for a cancel/update spend — the `WithdrawOrUpdate` branch
/// only checks a signature, so these are generous. Replaced by evaluation.
pub const DELIST_EX_UNITS_MEM: u64 = 400_000;
pub const DELIST_EX_UNITS_STEPS: u64 = 150_000_000;

// ============================================================================
// Datum
// ============================================================================

/// An address as the jpg/abandonware validators expect it in a payout.
///
/// Mirrors `parse_payout_address` in `marketplace.rs`: payment credential
/// `Constr 0|1 [hash]` (key|script), then `Option<Referenced<Credential>>` as
/// `Constr 0 [Constr 0 [Constr 0|1 [hash]]]` for an inline stake credential
/// or `Constr 1 []` for none. Pointer delegation is not representable and is
/// refused.
pub fn address_to_plutus(address: &Address) -> Result<PlutusData, TxBuildError> {
    let Address::Shelley(shelley) = address else {
        return Err(TxBuildError::BuildFailed(
            "payout address must be a Shelley address".into(),
        ));
    };
    let payment = match shelley.payment() {
        ShelleyPaymentPart::Key(h) => constr(0, vec![bytes(h.to_vec())]),
        ShelleyPaymentPart::Script(h) => constr(1, vec![bytes(h.to_vec())]),
    };
    let stake = match shelley.delegation() {
        ShelleyDelegationPart::Null => constr(1, vec![]),
        ShelleyDelegationPart::Key(h) => {
            constr(0, vec![constr(0, vec![constr(0, vec![bytes(h.to_vec())])])])
        }
        ShelleyDelegationPart::Script(h) => {
            constr(0, vec![constr(0, vec![constr(1, vec![bytes(h.to_vec())])])])
        }
        ShelleyDelegationPart::Pointer(_) => {
            return Err(TxBuildError::BuildFailed(
                "pointer-delegated payout addresses are not supported".into(),
            ));
        }
    };
    Ok(constr(0, vec![payment, stake]))
}

/// Encode a listing datum: `Constr 0 [payouts, owner]`.
///
/// `owner` is the seller's payment key hash — the key whose signature the
/// validator demands to cancel or update.
pub fn encode_listing_datum(
    payouts: &[DatumPayout],
    owner_pkh: Hash<28>,
) -> Result<Vec<u8>, TxBuildError> {
    if payouts.is_empty() {
        return Err(TxBuildError::BuildFailed(
            "a listing needs at least one payout".into(),
        ));
    }
    let payout_items = payouts
        .iter()
        .map(|p| {
            if p.lovelace == 0 {
                return Err(TxBuildError::BuildFailed(
                    "a payout of 0 lovelace is rejected by the validator".into(),
                ));
            }
            let amount = i64::try_from(p.lovelace)
                .map_err(|_| TxBuildError::BuildFailed("payout exceeds i64".into()))?;
            Ok(constr(0, vec![address_to_plutus(&p.address)?, int(amount)]))
        })
        .collect::<Result<Vec<_>, TxBuildError>>()?;
    encode_plutus_data(&constr(
        0,
        vec![list(payout_items), bytes(owner_pkh.to_vec())],
    ))
}

/// The owner key hash from a listing datum — the signature a cancel needs.
///
/// Accepts both field orders the parser does (`[payouts, owner]` and
/// `[owner, payouts]`).
pub fn listing_owner_pkh(datum_cbor: &[u8]) -> Result<Hash<28>, TxBuildError> {
    let data: PlutusData = pallas_codec::minicbor::decode(datum_cbor)
        .map_err(|e| TxBuildError::CborParse(format!("{e}")))?;
    let PlutusData::Constr(c) = data else {
        return Err(TxBuildError::BuildFailed(
            "listing datum is not a constructor".into(),
        ));
    };
    let owner = c
        .fields
        .iter()
        .find_map(|f| match f {
            PlutusData::BoundedBytes(b) if b.len() == 28 => Some(b.to_vec()),
            _ => None,
        })
        .ok_or_else(|| {
            TxBuildError::BuildFailed("listing datum carries no 28-byte owner key hash".into())
        })?;
    let arr: [u8; 28] = owner.try_into().expect("checked length");
    Ok(Hash::from(arr))
}

// ============================================================================
// List
// ============================================================================

/// What the seller is putting up.
#[derive(Debug, Clone)]
pub struct ListingIntent {
    pub asset: AssetId,
    pub quantity: u64,
    /// What the seller receives. The buyer pays this plus the validator's
    /// fee; the UI should show both.
    pub price_lovelace: u64,
}

/// Build the transaction that locks `intent.asset` at `sale_address` under
/// an inline listing datum paying the seller `price_lovelace`.
///
/// `deps.from_address` is the seller: it receives the payout when sold, its
/// payment key is the datum's owner, and it funds the min-UTxO and fee.
pub fn build_list(
    deps: &TxDeps,
    sale_address: &Address,
    intent: &ListingIntent,
) -> Result<UnsignedTx, TxBuildError> {
    if intent.quantity == 0 {
        return Err(TxBuildError::BuildFailed(
            "cannot list a quantity of 0".into(),
        ));
    }
    let owner_pkh = extract_payment_key_hash(&deps.from_address)?;

    // The UTxO holding the asset is spent whole; whatever else it carries
    // goes straight back to the seller.
    let holding = deps
        .utxos
        .iter()
        .find(|u| {
            u.assets
                .iter()
                .any(|a| a.asset_id == intent.asset && a.quantity >= intent.quantity)
        })
        .ok_or_else(|| {
            TxBuildError::AssetNotFound(format!(
                "{} ×{} is not in the seller's UTxOs",
                intent.asset.concatenated(),
                intent.quantity
            ))
        })?;

    let datum = encode_listing_datum(
        &[DatumPayout {
            address: deps.from_address.clone(),
            lovelace: intent.price_lovelace,
        }],
        owner_pkh,
    )?;

    let listed: Vec<AssetAmount> = vec![(intent.asset.clone(), intent.quantity)];
    let listing_lovelace = min_ada_with_coefficient(
        deps.params.coins_per_utxo_byte,
        &listed,
        &OutputParams::with_datum(&datum),
    );
    let listing_output = with_assets(
        create_ada_output(sale_address.clone(), listing_lovelace).set_inline_datum(datum),
        &listed,
    )?;

    let mut builder = TxBuilder::new(deps.clone_shallow())
        .input(holding)?
        .output(listing_output);

    // Residual assets from the holding UTxO — ADA-only change would drop them.
    let residual = residual_assets(holding, &intent.asset, intent.quantity);
    if !residual.is_empty() {
        let lovelace = deps.params.min_utxo_for_assets(&residual);
        builder = builder.output(with_assets(
            create_ada_output(deps.from_address.clone(), lovelace),
            &residual,
        )?);
    }

    builder.build()
}

// ============================================================================
// Cancel / update
// ============================================================================

/// What happens to the asset when its listing is spent by the owner.
#[derive(Debug, Clone)]
pub enum CancelOutcome {
    /// Cancel: the asset returns to the owner's address.
    ReturnToOwner,
    /// Update: the asset is re-locked at the sale address under a new datum
    /// with this price. One transaction, no window where it is unlisted.
    Relist { price_lovelace: u64 },
}

/// Prepare the transaction that spends `listing` on the seller's branch.
///
/// Returns the builder so the caller can `build_evaluated` against a live
/// evaluator (preferred) or `build` with the starting budgets. The owner
/// named in the datum must be the payment key of `deps.from_address`, since
/// that is the signature the wallet will provide.
pub fn prepare_cancel(
    deps: &TxDeps,
    listing: &ParsedListing,
    script_ref: &ScriptReference,
    outcome: CancelOutcome,
    collateral_utxo: Option<&UtxoApi>,
    validity_slots: Option<(u64, u64)>,
) -> Result<TxBuilder, TxBuildError> {
    let owner_pkh = listing_owner_pkh(&listing.datum_cbor)?;
    let signer_pkh = extract_payment_key_hash(&deps.from_address)?;
    if owner_pkh != signer_pkh {
        return Err(TxBuildError::BuildFailed(format!(
            "listing owner is {owner_pkh} but the signing address's payment key is {signer_pkh} — \
             only the owner can cancel or update"
        )));
    }
    let redeemer = listing
        .marketplace_version
        .delist_redeemer()
        .ok_or_else(|| {
            TxBuildError::BuildFailed(format!(
                "no delist redeemer registered for {:?}",
                listing.marketplace_version
            ))
        })?;

    // Conway bills the reference script's bytes; one validator, read once.
    let mut params = deps.params.clone();
    params.ref_script_size = listing.script_ref.size;

    let ref_tx_hash = decode_tx_hash(script_ref.tx_hash)?;
    let mut builder = TxBuilder::new(TxDeps {
        utxos: deps.utxos.clone(),
        params,
        from_address: deps.from_address.clone(),
        network_id: deps.network_id,
    })
    .spend_script_utxo(
        &listing.utxo,
        ScriptInput {
            script: ScriptSource::Reference {
                utxo: Input::new(Hash::from(ref_tx_hash), script_ref.output_index as u64),
                language: listing.script_ref.language,
            },
            datum_cbor: (!listing.datum_is_inline).then(|| listing.datum_cbor.clone()),
            redeemer_cbor: redeemer.encode(0),
            ex_units: ExUnits {
                mem: DELIST_EX_UNITS_MEM,
                steps: DELIST_EX_UNITS_STEPS,
            },
        },
    )?
    .with_signer(owner_pkh);

    let assets: Vec<AssetAmount> = listing
        .utxo
        .assets
        .iter()
        .map(|a| (a.asset_id.clone(), a.quantity))
        .collect();
    if assets.is_empty() {
        return Err(TxBuildError::BuildFailed(
            "listing UTxO carries no assets — nothing to return or relist".into(),
        ));
    }

    let output = match outcome {
        CancelOutcome::ReturnToOwner => {
            let lovelace = deps.params.min_utxo_for_assets(&assets);
            with_assets(
                create_ada_output(deps.from_address.clone(), lovelace),
                &assets,
            )?
        }
        CancelOutcome::Relist { price_lovelace } => {
            let datum = encode_listing_datum(
                &[DatumPayout {
                    address: deps.from_address.clone(),
                    lovelace: price_lovelace,
                }],
                owner_pkh,
            )?;
            let lovelace = min_ada_with_coefficient(
                deps.params.coins_per_utxo_byte,
                &assets,
                &OutputParams::with_datum(&datum),
            );
            // Re-lock at the same validator's sale address — the registry's,
            // for this version on this network, never a client-supplied one.
            let sale_address = sale_address_for(listing.marketplace_version, deps.network_id)?;
            with_assets(
                create_ada_output(sale_address, lovelace).set_inline_datum(datum),
                &assets,
            )?
        }
    };
    builder = builder.output(output);

    if let Some((from, to)) = validity_slots {
        builder = builder.valid_from(from).valid_to(to);
    }
    builder = match collateral_utxo {
        Some(utxo) => {
            let tx_hash = decode_tx_hash(&utxo.tx_hash)?;
            builder.with_collateral(CollateralConfig::Manual(Input::new(
                Hash::from(tx_hash),
                utxo.output_index as u64,
            )))
        }
        None => builder.with_collateral(CollateralConfig::Auto),
    };
    Ok(builder)
}

// ============================================================================
// Helpers
// ============================================================================

fn with_assets(
    mut output: pallas_txbuilder::Output,
    assets: &[AssetAmount],
) -> Result<pallas_txbuilder::Output, TxBuildError> {
    for (asset_id, quantity) in assets {
        let policy = decode_policy_id(asset_id.policy_id())?;
        let name = decode_asset_name(asset_id.asset_name_hex());
        output = output
            .add_asset(Hash::from(policy), name, *quantity)
            .map_err(|e| TxBuildError::BuildFailed(format!("add asset to output: {e}")))?;
    }
    Ok(output)
}

/// Everything the holding UTxO carries other than the listed quantity.
fn residual_assets(holding: &UtxoApi, listed: &AssetId, quantity: u64) -> Vec<AssetAmount> {
    holding
        .assets
        .iter()
        .filter_map(|a| {
            let left = if a.asset_id == *listed {
                a.quantity.saturating_sub(quantity)
            } else {
                a.quantity
            };
            (left > 0).then(|| (a.asset_id.clone(), left))
        })
        .collect()
}

fn extract_payment_key_hash(address: &Address) -> Result<Hash<28>, TxBuildError> {
    match address {
        Address::Shelley(sh) => match sh.payment() {
            ShelleyPaymentPart::Key(h) => Ok(*h),
            ShelleyPaymentPart::Script(_) => Err(TxBuildError::BuildFailed(
                "seller address is a script address; a listing owner must be a key".into(),
            )),
        },
        _ => Err(TxBuildError::BuildFailed(
            "seller address must be a Shelley address".into(),
        )),
    }
}

trait CloneShallow {
    fn clone_shallow(&self) -> Self;
}

impl CloneShallow for TxDeps {
    fn clone_shallow(&self) -> Self {
        TxDeps {
            utxos: self.utxos.clone(),
            params: self.params.clone(),
            from_address: self.from_address.clone(),
            network_id: self.network_id,
        }
    }
}

/// A version's sale address on a network, from the registry's deployment
/// record. The first listed sale address is the canonical one (the
/// enterprise form for abandonware).
pub fn sale_address_for(version: MarketplaceType, network_id: u8) -> Result<Address, TxBuildError> {
    let network = address_registry::RegistryNetwork::from_network_id(network_id);
    let deployment = version.deployment(network).ok_or_else(|| {
        TxBuildError::BuildFailed(format!("no {version:?} deployment on {network:?}"))
    })?;
    let sale = deployment.sale_addresses.first().ok_or_else(|| {
        TxBuildError::BuildFailed(format!("{version:?} deployment lists no sale address"))
    })?;
    Address::from_bech32(sale.address)
        .map_err(|e| TxBuildError::BuildFailed(format!("sale address does not decode: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::marketplace::parse_listing_datum;

    const SELLER: &str = "addr_test1qqpple6hhjkkfz2fltkl5wf3txrn5e62qyw0j8jxrg8ur8veuq628l4gr3d5esl8z4d48dekypu39gh6d4xly63t7rtqq29vk7";
    const SELLER_PKH: &str = "021fe757bcad648949faedfa393159873a674a011cf91e461a0fc19d";
    const SELLER_STAKE: &str = "99e034a3fea81c5b4cc3e7155b53b736207912a2fa6d4df26a2bf0d6";

    fn seller() -> Address {
        Address::from_bech32(SELLER).unwrap()
    }

    /// The datum we write is the datum the buy path reads: one round trip,
    /// same address, same amount, same owner — and it is the ABANDONWARE
    /// version that parses it, so the two halves cannot drift.
    #[test]
    fn datum_round_trips_through_the_buy_parser() {
        let owner = extract_payment_key_hash(&seller()).unwrap();
        let datum = encode_listing_datum(
            &[DatumPayout {
                address: seller(),
                lovelace: 25_000_000,
            }],
            owner,
        )
        .unwrap();

        let payouts = parse_listing_datum(&datum, MarketplaceType::Abandonware, 0).unwrap();
        assert_eq!(payouts.len(), 1);
        assert_eq!(payouts[0].lovelace, 25_000_000);
        assert_eq!(payouts[0].address.to_bech32().unwrap(), SELLER);
        assert_eq!(listing_owner_pkh(&datum).unwrap().to_string(), SELLER_PKH);
    }

    /// Byte-for-byte pin of the shape the validator's `Datum` type expects
    /// (`Constr 0 [payouts, owner]`, addresses as `Constr 0 [cred, Option]`).
    /// Recomputing this by hand from the Aiken types is the check that the
    /// encoder targets the CONTRACT, not merely our own parser.
    ///
    /// Definite-length arrays (`82`, `81`): the helpers emit those, Lucid's
    /// `Data.to` emits those, and a datum is decoded as PlutusData either
    /// way. Only hash-style datums would care about the exact bytes, and
    /// ours are inline.
    #[test]
    fn datum_bytes_match_the_contract_layout() {
        let owner = extract_payment_key_hash(&seller()).unwrap();
        let datum = encode_listing_datum(
            &[DatumPayout {
                address: seller(),
                lovelace: 1_000_000,
            }],
            owner,
        )
        .unwrap();
        let expected = format!(
            "d87982\
               81\
                 d87982\
                   d87982\
                     d87981581c{SELLER_PKH}\
                     d87981d87981d87981581c{SELLER_STAKE}\
                   1a000f4240\
               581c{SELLER_PKH}"
        );
        assert_eq!(hex::encode(&datum), expected);
    }

    #[test]
    fn enterprise_address_encodes_no_stake() {
        let ent = Address::from_bech32(
            "addr_test1vqpple6hhjkkfz2fltkl5wf3txrn5e62qyw0j8jxrg8ur8vgpznwd2",
        )
        .or_else(|_| {
            // Fall back to constructing one from the seller's payment part.
            let Address::Shelley(sh) = seller() else {
                unreachable!()
            };
            Ok::<_, pallas_addresses::Error>(Address::Shelley(
                pallas_addresses::ShelleyAddress::new(
                    sh.network(),
                    sh.payment().clone(),
                    ShelleyDelegationPart::Null,
                ),
            ))
        })
        .unwrap();
        let data = address_to_plutus(&ent).unwrap();
        let encoded = hex::encode(encode_plutus_data(&data).unwrap());
        // `Constr 1 []` for `None` closes the address.
        assert!(encoded.ends_with("d87a80"), "{encoded}");
    }

    #[test]
    fn zero_price_is_refused() {
        let owner = extract_payment_key_hash(&seller()).unwrap();
        let err = encode_listing_datum(
            &[DatumPayout {
                address: seller(),
                lovelace: 0,
            }],
            owner,
        )
        .unwrap_err();
        assert!(format!("{err:?}").contains("0 lovelace"));
    }

    #[test]
    fn residual_assets_exclude_the_listed_quantity() {
        let a = AssetId::new_unchecked("ab".repeat(28), "01".into());
        let b = AssetId::new_unchecked("cd".repeat(28), "02".into());
        let holding = UtxoApi {
            tx_hash: "a".repeat(64),
            output_index: 0,
            lovelace: 2_000_000,
            assets: vec![
                cardano_assets::AssetQuantity {
                    asset_id: a.clone(),
                    quantity: 3,
                },
                cardano_assets::AssetQuantity {
                    asset_id: b.clone(),
                    quantity: 1,
                },
            ],
            tags: vec![],
        };
        let residual = residual_assets(&holding, &a, 1);
        assert_eq!(residual, vec![(a, 2), (b, 1)]);
    }
}
