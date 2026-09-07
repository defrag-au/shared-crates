//! Marketplace buy/sweep TX builder.
//!
//! Constructs Plutus script-spend transactions that buy NFTs out of marketplace
//! listings, for a single item or a multi-item sweep.
//!
//! # How it works
//!
//! 1. Each listing is a UTxO at a marketplace script address whose datum encodes
//!    payout obligations (seller take, marketplace fee, royalties).
//! 2. The buy TX consumes those UTxOs with the venue's buy redeemer, and creates
//!    outputs satisfying every payout obligation.
//! 3. The NFTs go to the buyer; coin selection and change are handled by the
//!    fluent builder.
//! 4. The validator is supplied by a CIP-33 reference input, not embedded.
//!
//! # Parameterised by contract deployment, not hardcoded
//!
//! The reference script UTxO and the buy redeemer both come from
//! [`address_registry::MarketplaceType`], keyed off the version the datum parser
//! identified. Nothing about jpg.store is baked in here, so standing up a
//! redeployed marketplace of our own means adding a registry entry — not
//! forking this builder.
//!
//! # Datum witnessing
//!
//! jpg.store listings carry **hash** datums, so the preimage must be supplied in
//! the witness set or the node rejects the spend with `MissingRequiredDatums`.
//! The inverse is equally fatal: witnessing a datum that is already *inline* on
//! the UTxO is rejected with `NotAllowedSupplementalDatums`. Which applies is
//! per-listing, carried on [`ParsedListing::datum_is_inline`].

use std::collections::BTreeMap;

use cardano_assets::utxo::UtxoApi;
use pallas_addresses::Address;
use pallas_crypto::hash::Hash;
use pallas_txbuilder::{ExUnits, Input, Output};

use crate::builder::TxDeps;
use crate::builder::UnsignedTx;
use crate::builder::fluent::TxBuilder;
use crate::builder::marketplace::ParsedListing;
use crate::builder::script::{CollateralConfig, ScriptInput, ScriptSource};
use crate::error::TxBuildError;
use crate::helpers::decode::decode_tx_hash;
use crate::helpers::output::create_ada_output;
use crate::params::TxBuildParams;

/// Starting ExUnits for a buy redeemer, used only for the first pass.
///
/// These are estimates, deliberately generous, and are meant to be replaced:
/// prefer [`build_buy_evaluated`], which runs the real validator and patches in
/// measured units. Building with fixed units both overpays on fees and, on a
/// sweep, walks into the per-transaction execution cap — N listings multiply
/// these, and the block limit is not far away.
/// Public so batch planners can derive a sweep cap from the same figure the
/// builder starts with, rather than keeping a second estimate that drifts.
pub const BUY_EX_UNITS_MEM: u64 = 1_400_000;
pub const BUY_EX_UNITS_STEPS: u64 = 500_000_000;

/// Dependencies for a marketplace buy TX.
#[derive(Debug)]
pub struct BuyDeps {
    /// Buyer's wallet UTxOs — fund the payouts, the fee, and collateral.
    pub buyer_utxos: Vec<UtxoApi>,
    /// Protocol parameters. `params.cost_models` should carry the LIVE cost
    /// models; a stale language view means `PPViewHashesDontMatch`.
    pub params: TxBuildParams,
    /// Buyer's receiving address — gets the NFTs and the change.
    pub buyer_address: Address,
    /// Network ID (1 = mainnet, 0 = testnet).
    pub network_id: u8,
    /// Collateral UTxO. `None` auto-selects a pure-ADA one.
    pub collateral_utxo: Option<UtxoApi>,
}

/// Build a marketplace buy/sweep TX with *estimated* execution units.
///
/// Prefer [`build_buy_evaluated`] wherever an evaluator is available; see
/// [`BUY_EX_UNITS_MEM`] for why fixed units are a poor substitute.
pub fn build_buy(deps: &BuyDeps, listings: &[ParsedListing]) -> Result<UnsignedTx, TxBuildError> {
    prepare_buy(deps, listings)?.build()
}

/// Build a marketplace buy/sweep TX with execution units measured by running the
/// scripts through `evaluator`.
///
/// This is also the cheapest possible correctness check: evaluation executes the
/// real validator against our redeemer, datum and payout outputs, so a
/// mis-parsed payout or a wrong reference script fails here rather than on
/// submission — without spending anything.
pub async fn build_buy_evaluated<E>(
    deps: &BuyDeps,
    listings: &[ParsedListing],
    evaluator: &E,
) -> Result<UnsignedTx, TxBuildError>
where
    E: crate::evaluate::TxEvaluator + ?Sized,
{
    prepare_buy(deps, listings)?
        .build_evaluated(evaluator)
        .await
}

/// Assemble the buy into a [`TxBuilder`], shared by both entry points.
fn prepare_buy(deps: &BuyDeps, listings: &[ParsedListing]) -> Result<TxBuilder, TxBuildError> {
    if listings.is_empty() {
        return Err(TxBuildError::BuildFailed(
            "No listings provided".to_string(),
        ));
    }

    let mut builder = TxBuilder::new(TxDeps {
        utxos: deps.buyer_utxos.clone(),
        params: deps.params.clone(),
        from_address: deps.buyer_address.clone(),
        network_id: deps.network_id,
    });

    // Reference inputs are NOT added here: `spend_script_utxo` derives them
    // from `ScriptSource::Reference` and the assembler deduplicates, so a sweep
    // across listings at one contract references the script once while a sweep
    // spanning V1 and V2/V3 gets one reference per distinct validator. Adding
    // them here as well produced a duplicate entry in the Conway reference-input
    // *set*, which the ledger rejects outright.
    for listing in listings {
        let version = listing.marketplace_version;

        let script_ref = version.script_reference().ok_or_else(|| {
            TxBuildError::BuildFailed(format!(
                "No reference script registered for {version:?} — a buy cannot supply the \
                 validator. Add it to address-registry's `script_reference()`."
            ))
        })?;
        let redeemer = version.buy_redeemer().ok_or_else(|| {
            TxBuildError::BuildFailed(format!("No buy redeemer registered for {version:?}."))
        })?;
        let redeemer_cbor = hex::decode(redeemer.cbor_hex).map_err(|e| {
            TxBuildError::BuildFailed(format!("{version:?} buy redeemer is not valid hex: {e}"))
        })?;

        let ref_tx_hash = decode_tx_hash(script_ref.tx_hash)?;
        builder = builder.spend_script_utxo(
            &listing.utxo,
            ScriptInput {
                script: ScriptSource::Reference {
                    utxo: Input::new(Hash::from(ref_tx_hash), script_ref.output_index as u64),
                },
                // Supply the preimage only for hash datums — see the module doc.
                datum_cbor: (!listing.datum_is_inline).then(|| listing.datum_cbor.clone()),
                redeemer_cbor,
                ex_units: ExUnits {
                    mem: BUY_EX_UNITS_MEM,
                    steps: BUY_EX_UNITS_STEPS,
                },
            },
        )?;
    }

    // Every payout obligation the datums impose.
    for output in build_payout_outputs(listings)? {
        builder = builder.output(output);
    }

    // The NFTs, to the buyer. Kept separate from change: an output carrying many
    // assets has a real min-ADA cost and a max-value-size limit, and conflating
    // it with change hides both.
    for output in build_buyer_asset_outputs(deps, listings)? {
        builder = builder.output(output);
    }

    builder = builder.with_signer(Hash::from(extract_payment_key_hash(&deps.buyer_address)?));

    builder = match &deps.collateral_utxo {
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

/// The buyer's asset outputs: every NFT from every listing, each output sized to
/// clear its own min-UTxO floor.
///
/// A large sweep cannot go into one output — the ledger caps an output's value
/// at `maxValueSize` and rejects the whole transaction with `OutputTooBigUTxO`.
/// [`split_by_value_size`](crate::utxo::split_by_value_size) is the shared
/// splitter the sweep path already uses; reusing it keeps one implementation of
/// that limit rather than a second copy that drifts.
fn build_buyer_asset_outputs(
    deps: &BuyDeps,
    listings: &[ParsedListing],
) -> Result<Vec<Output>, TxBuildError> {
    let asset_amounts: Vec<crate::utxo::AssetAmount> = listings
        .iter()
        .flat_map(|l| l.utxo.assets.iter())
        .map(|a| (a.asset_id.clone(), a.quantity))
        .collect();

    if asset_amounts.is_empty() {
        return Err(TxBuildError::BuildFailed(
            "Listings carry no assets — refusing to build a buy that receives nothing".to_string(),
        ));
    }

    let bundles = crate::utxo::split_by_value_size(&asset_amounts, deps.params.max_value_size);

    bundles
        .into_iter()
        .map(|bundle| {
            let min_ada = deps.params.min_utxo_for_assets(&bundle);
            let mut output = create_ada_output(deps.buyer_address.clone(), min_ada);
            for (asset_id, quantity) in &bundle {
                let policy = crate::helpers::decode::decode_policy_id(asset_id.policy_id())?;
                let name = crate::helpers::decode::decode_asset_name(asset_id.asset_name_hex());
                output = output
                    .add_asset(Hash::from(policy), name, *quantity)
                    .map_err(|e| {
                        TxBuildError::BuildFailed(format!("Failed to add NFT to buyer output: {e}"))
                    })?;
            }
            Ok(output)
        })
        .collect()
}

/// Build payout outputs from all listing payouts.
///
/// Payouts to the same address are merged into a single output — a sweep across
/// several listings pays one marketplace fee address many times over, and the
/// validators check the total received per address, not the output count.
fn build_payout_outputs(listings: &[ParsedListing]) -> Result<Vec<Output>, TxBuildError> {
    let mut merged: BTreeMap<Vec<u8>, (Address, u64)> = BTreeMap::new();

    for listing in listings {
        for payout in &listing.payouts {
            let entry = merged
                .entry(payout.address.to_vec())
                .or_insert_with(|| (payout.address.clone(), 0));
            entry.1 = entry.1.checked_add(payout.lovelace).ok_or_else(|| {
                TxBuildError::BuildFailed("Payout total overflowed u64".to_string())
            })?;
        }
    }

    Ok(merged
        .into_values()
        .map(|(address, lovelace)| create_ada_output(address, lovelace))
        .collect())
}

/// Extract the 28-byte payment key hash from a Shelley address.
fn extract_payment_key_hash(address: &Address) -> Result<[u8; 28], TxBuildError> {
    use pallas_addresses::ShelleyPaymentPart;

    match address {
        Address::Shelley(shelley) => match shelley.payment() {
            ShelleyPaymentPart::Key(hash) => Ok(**hash),
            ShelleyPaymentPart::Script(_) => Err(TxBuildError::BuildFailed(
                "Buyer address is a script address; a buy needs a key-hash signer".to_string(),
            )),
        },
        _ => Err(TxBuildError::BuildFailed(
            "Buyer address must be a Shelley address".to_string(),
        )),
    }
}

/// The total a buyer must find to satisfy every payout, before fees. The
/// listings' own lovelace comes back in the same TX, so it offsets this.
pub fn total_payout_lovelace(listings: &[ParsedListing]) -> u64 {
    listings
        .iter()
        .flat_map(|l| &l.payouts)
        .map(|p| p.lovelace)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use address_registry::MarketplaceType;

    fn buyer() -> Address {
        Address::from_bech32(
            "addr_test1qz2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3jcu5d8ps7zex2k2xt3uqxgjqnnj83ws8lhrn648jjxtwq2ytjqp",
        )
        .unwrap()
    }

    fn deps() -> BuyDeps {
        BuyDeps {
            buyer_utxos: vec![],
            params: TxBuildParams::default(),
            buyer_address: buyer(),
            network_id: 0,
            collateral_utxo: None,
        }
    }

    #[test]
    fn empty_listings_is_rejected() {
        assert!(build_buy(&deps(), &[]).is_err());
    }

    #[test]
    fn payouts_to_the_same_address_are_merged() {
        use crate::builder::marketplace::DatumPayout;

        let addr = buyer();
        let listing = |lovelace| ParsedListing {
            utxo: UtxoApi {
                tx_hash: "a".repeat(64),
                output_index: 0,
                lovelace: 1_000_000,
                assets: vec![],
                tags: vec![],
            },
            datum_cbor: vec![],
            datum_is_inline: false,
            payouts: vec![DatumPayout {
                address: addr.clone(),
                lovelace,
            }],
            marketplace_version: MarketplaceType::JpgStoreV1,
        };

        let outputs = build_payout_outputs(&[listing(100), listing(250)]).unwrap();
        assert_eq!(outputs.len(), 1, "same address must collapse to one output");
        assert_eq!(outputs[0].lovelace, 350);
    }

    /// A version with no registered reference script must fail with a message
    /// that says what to do, not a generic build error — this is the exact
    /// state V1 and V2 were both in before the registry was corrected.
    #[test]
    fn missing_reference_script_names_the_registry() {
        use crate::builder::marketplace::DatumPayout;

        let listing = ParsedListing {
            utxo: UtxoApi {
                tx_hash: "a".repeat(64),
                output_index: 0,
                lovelace: 1_000_000,
                assets: vec![],
                tags: vec![],
            },
            datum_cbor: vec![],
            datum_is_inline: false,
            payouts: vec![DatumPayout {
                address: buyer(),
                lovelace: 100,
            }],
            // Wayup has no reference script registered.
            marketplace_version: MarketplaceType::Wayup,
        };

        let err = build_buy(&deps(), &[listing]).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("script_reference"),
            "error should point at the registry, got: {msg}"
        );
    }

    /// Both jpg versions we can actually buy must resolve a reference script
    /// and a redeemer. If this fails the registry has regressed.
    #[test]
    fn buyable_jpg_versions_resolve_contract_parameters() {
        for version in [
            MarketplaceType::JpgStoreV1,
            MarketplaceType::JpgStoreV2,
            MarketplaceType::JpgStoreV3,
        ] {
            assert!(
                version.script_reference().is_some(),
                "{version:?} needs a reference script"
            );
            let redeemer = version.buy_redeemer().expect("redeemer");
            assert!(
                hex::decode(redeemer.cbor_hex).is_ok(),
                "{version:?} redeemer must be valid hex"
            );
        }
    }

    #[test]
    fn script_address_buyer_is_rejected() {
        let script_addr =
            Address::from_bech32("addr1w8rjw3pawl0kelu4mj3c8x20fsczf5pl744s9mxz9v8n7eg0fcr8k")
                .unwrap();
        assert!(extract_payment_key_hash(&script_addr).is_err());
    }
}
