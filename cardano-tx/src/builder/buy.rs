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

/// Largest sweep whose evaluated cost fits a transaction's execution budget.
///
/// **Do NOT derive this by dividing the cap by [`BUY_EX_UNITS_MEM`].** That
/// assumes each listing costs the same, and a jpg buy does not: the cost is
/// SUPER-LINEAR because every validator scans the transaction's output list
/// looking for its own payouts, and that list grows with the sweep. The flat
/// division said 11 listings fit; four do not.
///
/// Measured against the live mainnet validator (memory, jpg V1):
///
/// | listings | total  | largest single spend |
/// |----------|--------|----------------------|
/// | 1        |  2.85M |  2.85M |
/// | 2        |  7.18M |  6.14M |
/// | 3        | 12.25M |  9.43M |
/// | 4        | 18.10M | 12.72M |
///
/// Second differences are near-constant (0.74M, 0.78M), i.e. quadratic, so the
/// fit below is `0.38n² + 3.19n − 0.72` in millions — which reproduces all four
/// measurements to within 0.02M. Against the 16.5M mainnet cap it yields 3.
///
/// Steps are not the binding constraint (a 4-listing sweep uses 38% of the step
/// budget while exceeding memory), but both are checked so a future parameter
/// change cannot silently invert that.
pub fn max_buys_for_budget(mem_cap: u64, steps_cap: u64) -> usize {
    // Same shape for steps, fitted the same way: 0.10n² + 0.86n − 0.13 in
    // BILLIONS, from 0.583 / 1.477 / 2.562 / 3.844.
    fn mem_for(n: u64) -> u64 {
        380_000 * n * n + 3_190_000 * n - 720_000
    }
    fn steps_for(n: u64) -> u64 {
        100_000_000 * n * n + 860_000_000 * n - 130_000_000
    }

    let mut best = 1;
    for n in 1..=16 {
        if mem_for(n) <= mem_cap && steps_for(n) <= steps_cap {
            best = n as usize;
        } else {
            break;
        }
    }
    best
}

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
    /// Transaction validity window as `(from_slot, to_slot)`.
    ///
    /// Every real jpg V2 buy sets one. A validator that reads
    /// `txInfoValidRange` — to bound a deadline, or to derive a time — can fail
    /// outright on the unbounded `(-inf, +inf)` range an omitted interval
    /// produces, and it fails with no trace saying so.
    pub validity_slots: Option<(u64, u64)>,
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

/// Total reference-script bytes this buy will be charged for under Conway's
/// `minFeeRefScriptCoinsPerByte`.
///
/// **Distinct by validator, not per listing.** A sweep over one generation
/// reads that validator ONCE however many listings it spends, so billing per
/// listing over-pays; billing zero — which every buy did, because callers set
/// `ref_script_size` per action-type and buys fell into the `else` branch — is
/// rejected by the node with `FeeTooSmallUTxO`. Neither figure is checked by
/// `evaluateTransaction`, so a wrong one survives every dry run.
fn reference_script_size(listings: &[ParsedListing]) -> u64 {
    let mut counted: Vec<&str> = Vec::new();
    let mut total = 0;
    for listing in listings {
        let hash = listing
            .marketplace_version
            .script_reference()
            .map(|r| r.script_hash)
            .unwrap_or_default();
        if !counted.contains(&hash) {
            counted.push(hash);
            total += listing.script_ref.size;
        }
    }
    total
}

/// Assemble the buy into a [`TxBuilder`], shared by both entry points.
fn prepare_buy(deps: &BuyDeps, listings: &[ParsedListing]) -> Result<TxBuilder, TxBuildError> {
    if listings.is_empty() {
        return Err(TxBuildError::BuildFailed(
            "No listings provided".to_string(),
        ));
    }

    // One contract generation per transaction.
    //
    // ## Verified against the live validator, not assumed
    //
    // Controls, each evaluating OK on its own:
    //   V2 `4b5fa741…#0` alone  → fee ₳0.216508
    //   V1 `e5558430…#1` alone  → fee ₳0.382996
    //
    // The two together, `evaluateTransaction` via ogmios:
    //   ogmios 3010 → validator {index:1, purpose:"spend"} failed, traces ["3"]
    //
    // Input 1 is the V1 spend (inputs sort by tx hash: `4b5f…` < `e555…`), so
    // **V2 passes and V1 fails**. Two candidate explanations were tested and
    // BOTH ruled out:
    //
    // 1. *Missing required signer.* V1 demands a disclosed signer and the
    //    signer predicate was `all()`, which silently dropped it from any
    //    mixed sweep. Real bug, fixed below (`any()`), and the mixed sweep
    //    still fails — so this was not the cause.
    // 2. *Block ordering.* V2 carries its payout offset in the redeemer so it
    //    does not care where its block sits; V1 has no offset and might need
    //    the position a solitary buy gives it. Putting V1's block FIRST moved
    //    V2's redeemer offset from 1 to 4, confirming the reorder took effect
    //    — and V1 failed identically, same trace. So it is not position.
    //
    // A V1-only sweep of 3 evaluates fine, so V1 tolerates several script
    // inputs and several settlement blocks; it is specifically a FOREIGN
    // generation's presence it rejects. What exactly it asserts (trace "3")
    // needs the V1 script decompiled — it is not in `contracts-v3`.
    //
    // Until that is known, refusing here turns an on-chain script failure into
    // a legible build error. Splitting by version is the caller's job.
    let first_version = listings[0].marketplace_version;
    if let Some(other) = listings
        .iter()
        .find(|l| l.marketplace_version != first_version)
    {
        return Err(TxBuildError::BuildFailed(format!(
            "Cannot mix {first_version:?} and {:?} listings in one buy — each contract \
             locates its payout outputs differently, so a single output layout cannot \
             satisfy both. Split the sweep by contract version.",
            other.marketplace_version
        )));
    }

    // Conway charges `minFeeRefScriptCoinsPerByte` for every reference script a
    // transaction reads, and only the builder knows which those are — the
    // caller cannot, because a sweep references one script per distinct
    // GENERATION, not one per listing. Callers were setting this per
    // action-type and buys got zero, so every buy under-paid by exactly the
    // reference script's worth (1673 B × 15 = 25,095 lovelace for jpg V2/V3)
    // and the node rejected it with `FeeTooSmallUTxO`. Nothing catches it
    // earlier: `evaluateTransaction` does not check fees.
    //
    // Distinct by script hash, since a sweep across one generation reads that
    // validator once no matter how many listings it spends.
    let mut params = deps.params.clone();
    params.ref_script_size = reference_script_size(listings);

    let mut builder = TxBuilder::new(TxDeps {
        utxos: deps.buyer_utxos.clone(),
        params: params.clone(),
        from_address: deps.buyer_address.clone(),
        network_id: deps.network_id,
    });

    // Reference inputs are NOT added here: `spend_script_utxo` derives them
    // from `ScriptSource::Reference` and the assembler deduplicates, so a sweep
    // across listings at one contract references the script once while a sweep
    // spanning V1 and V2/V3 gets one reference per distinct validator. Adding
    // them here as well produced a duplicate entry in the Conway reference-input
    // *set*, which the ledger rejects outright.
    // Contract parameters are resolved FIRST, before any output is built.
    //
    // Ordering matters for the error the caller sees: an unbuyable contract is
    // the actionable problem, and validating it after the output-building step
    // masked it behind whatever that step complained about instead.
    let contracts = listings
        .iter()
        .map(|listing| {
            let version = listing.marketplace_version;
            let script_ref = version.script_reference().ok_or_else(|| {
                TxBuildError::BuildFailed(format!(
                    "No reference script registered for {version:?} — a buy cannot supply the \
                     validator. Add it to address-registry's `script_reference()`."
                ))
            })?;
            if !version.buy_supported() {
                return Err(TxBuildError::BuildFailed(format!(
                    "Buying {version:?} listings is not supported yet — the validator rejects \
                     a minimal transaction even with the correct redeemer. Refusing here \
                     rather than building something that fails at evaluation."
                )));
            }
            let redeemer = version.buy_redeemer().ok_or_else(|| {
                TxBuildError::BuildFailed(format!("No buy redeemer registered for {version:?}."))
            })?;
            Ok((script_ref, redeemer))
        })
        .collect::<Result<Vec<_>, TxBuildError>>()?;

    // Payout outputs are laid out per listing, in listing order, and the offset
    // of each listing's first payout is computed up front — jpg V2/V3 carry
    // that offset in the redeemer, so the outputs must be placed before the
    // spends can be described. See `build_payout_outputs`.
    let buyer_outputs = build_buyer_asset_outputs(deps, listings)?;
    let blocks = build_settlement_blocks(listings, buyer_outputs.len(), &deps.params)?;

    for (i, listing) in listings.iter().enumerate() {
        let (script_ref, redeemer) = &contracts[i];
        let redeemer_cbor = redeemer.encode(blocks[i].redeemer_index as u64);

        let ref_tx_hash = decode_tx_hash(script_ref.tx_hash)?;
        builder = builder.spend_script_utxo(
            &listing.utxo,
            ScriptInput {
                script: ScriptSource::Reference {
                    utxo: Input::new(Hash::from(ref_tx_hash), script_ref.output_index as u64),
                    // Read off the reference UTxO, never guessed — the language
                    // names the cost model in the script-integrity hash, and a
                    // wrong one is rejected at submit while evaluating clean.
                    language: listing.script_ref.language,
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

    // Output ORDER is load-bearing, not cosmetic.
    //
    // The buyer's asset outputs go FIRST, then the payouts. jpg V2/V3 read a
    // listing's payouts starting at `redeemer_index + 1`, so something must
    // occupy the slot the index names — on every real V2 buy sampled the
    // redeemer says 0 and the payouts sit at outputs 1 and 2. Emitting payouts
    // at 0 leaves no such slot and the validator errors with a bare `(error)`.
    for output in buyer_outputs {
        builder = builder.output(output);
    }

    // Then each listing's settlement block: marketplace fee (where the contract
    // charges one) followed by that listing's datum payouts.
    for block in blocks {
        for output in block.outputs {
            builder = builder.output(output);
        }
    }

    // Disclosed signer: only where the contract wants one. Real jpg V2 buys
    // carry NO required signers, and a validator that inspects
    // `txInfoSignatories` positionally can be broken by an extra entry.
    // `any`, not `all`: a required signer is a per-contract DEMAND, so one
    // listing wanting it means the transaction must carry it or that spend
    // fails. `all` silently dropped it from any mixed sweep — which is a
    // property of the tx we built, not of the contracts, and is exactly the
    // kind of self-inflicted failure that gets mistaken for an incompatibility.
    // For a single-generation sweep the two are identical, so this changes
    // nothing on the paths already proven against the validator.
    if listings
        .iter()
        .any(|l| l.marketplace_version.requires_disclosed_signer())
    {
        builder = builder.with_signer(Hash::from(extract_payment_key_hash(&deps.buyer_address)?));
    }

    if let Some((from, to)) = deps.validity_slots {
        builder = builder.valid_from(from).valid_to(to);
    }

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

/// The `datum_tag` jpg's ask validator requires on a marketplace fee output.
///
/// Straight from the contract (`validators/ask.ak`):
///
/// ```text
/// let datum_tag = out_ref |> serialise_data |> blake2b_256 |> InlineDatum
/// ```
///
/// where `out_ref` is the **listing UTxO being spent**. It exists for double
/// satisfaction: it binds this fee output to this specific spend, so one fee
/// output cannot be counted for two listings in a sweep.
///
/// The encoding is exact and was verified against three real buys: Plutus
/// `serialise_data` writes constructors with **indefinite-length** field lists,
/// so the preimage is
/// `d8799f d8799f 5820<tx_hash> ff <output_index> ff`
/// — `Constr 0 [Constr 0 [ByteString txid], Int index]`. The definite-length
/// spelling hashes to something else entirely and is silently rejected.
fn fee_output_datum_tag(listing: &ParsedListing) -> Result<Vec<u8>, TxBuildError> {
    use pallas_crypto::hash::Hasher;

    let tx_hash = decode_tx_hash(&listing.utxo.tx_hash)?;

    let mut preimage = Vec::with_capacity(48);
    preimage.extend_from_slice(&[0xd8, 0x79, 0x9f]); // Constr 0, indefinite
    preimage.extend_from_slice(&[0xd8, 0x79, 0x9f]); // TransactionId, indefinite
    preimage.extend_from_slice(&[0x58, 0x20]); // bytes(32)
    preimage.extend_from_slice(&tx_hash);
    preimage.push(0xff); // close TransactionId
    encode_cbor_uint(&mut preimage, u64::from(listing.utxo.output_index));
    preimage.push(0xff); // close OutputReference

    let digest = Hasher::<256>::hash(&preimage);

    // The datum itself is the hash as a PlutusData byte string.
    let mut cbor = vec![0x58, 0x20];
    cbor.extend_from_slice(digest.as_ref());
    Ok(cbor)
}

/// Minimal CBOR unsigned-int encoder for the output index in the datum tag.
fn encode_cbor_uint(out: &mut Vec<u8>, n: u64) {
    match n {
        0..=23 => out.push(n as u8),
        24..=0xFF => out.extend_from_slice(&[0x18, n as u8]),
        0x100..=0xFFFF => {
            out.push(0x19);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        _ => {
            out.push(0x1a);
            out.extend_from_slice(&(n as u32).to_be_bytes());
        }
    }
}

/// The settlement block for one listing: an optional marketplace fee output
/// followed by that listing's datum payouts, in datum order.
struct SettlementBlock {
    outputs: Vec<Output>,
    /// Value this listing's redeemer carries. Points at the fee output when the
    /// contract has one, because the validator reads payouts from `index + 1`.
    redeemer_index: usize,
}

/// Lay out the per-listing settlement blocks and the index each redeemer names.
///
/// **Order is contract-visible, not cosmetic.** jpg V2/V3 read a listing's
/// payouts starting at `redeemer_index + 1`, and require the slot at
/// `redeemer_index` to be the marketplace fee. So a fee-bearing listing emits
/// `[fee, payout…]` and names the fee's index; a listing whose fee already sits
/// inside its datum payouts (jpg V1) emits `[payout…]` and its bare redeemer
/// ignores the index entirely.
///
/// Payouts are **never merged across listings** even when two listings pay the
/// same address: merging shifts every later block and invalidates the offsets.
///
/// `leading_outputs` is how many outputs precede the first block (the buyer's
/// asset outputs) — which is why those are emitted first.
fn build_settlement_blocks(
    listings: &[ParsedListing],
    leading_outputs: usize,
    params: &TxBuildParams,
) -> Result<Vec<SettlementBlock>, TxBuildError> {
    let mut blocks = Vec::with_capacity(listings.len());
    let mut next = leading_outputs;

    for listing in listings {
        let mut outputs = Vec::new();
        let block_start = next;

        if let Some(fee) = listing.marketplace_version.marketplace_fee() {
            let payouts_total: u64 = listing.payouts.iter().map(|p| p.lovelace).sum();

            // The fee output must carry the contract's `datum_tag`, which binds
            // it to this exact spend — see `fee_output_datum_tag`.
            let marker = fee_output_datum_tag(listing)?;
            let datum_params = crate::utxo::OutputParams::with_datum(&marker);

            // The floor is the ledger's min-UTxO for THIS output — with the
            // datum counted. That is where the recurring 1,155,080 comes from
            // (268 bytes × 4310); it is not a magic constant, and hardcoding it
            // would silently drift with the protocol parameter.
            let floor = crate::utxo::min_ada_with_coefficient(
                params.coins_per_utxo_byte,
                &[],
                &datum_params,
            )
            .max(fee.minimum_lovelace);
            let amount = fee.due_on_payouts(payouts_total).max(floor);

            let address = Address::from_bech32(fee.address).map_err(|e| {
                TxBuildError::BuildFailed(format!(
                    "marketplace fee address for {:?} does not decode: {e}",
                    listing.marketplace_version
                ))
            })?;
            outputs.push(create_ada_output(address, amount).set_inline_datum(marker));
        }

        for payout in &listing.payouts {
            outputs.push(create_ada_output(payout.address.clone(), payout.lovelace));
        }

        next += outputs.len();
        blocks.push(SettlementBlock {
            outputs,
            redeemer_index: block_start,
        });
    }
    Ok(blocks)
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

    /// The jpg V2/V3 reference script as the chain describes it: `plutusV2`,
    /// 1673 bytes (Koios `utxo_info` on `1693c508…#0`).
    fn test_script_ref() -> crate::builder::marketplace::ScriptRefInfo {
        crate::builder::marketplace::ScriptRefInfo {
            language: pallas_txbuilder::ScriptKind::PlutusV2,
            size: 1673,
        }
    }

    /// The fit must reproduce what the chain actually charged, and must give 3
    /// against mainnet's 16.5M — the flat division it replaces said 11.
    #[test]
    fn sweep_cap_matches_measured_costs() {
        // Mainnet Conway.
        assert_eq!(max_buys_for_budget(16_500_000, 10_000_000_000), 3);

        // The safety property: the estimate must never UNDERestimate what the
        // chain charged, or the builder hands the node a transaction it will
        // reject. Overestimating merely batches one fewer listing, so a budget
        // of exactly the measured cost may admit n or n-1 — never more.
        for (n, measured_mem) in [
            (1usize, 2_850_000u64),
            (2, 7_180_000),
            (3, 12_250_000),
            (4, 18_100_000),
        ] {
            assert!(
                max_buys_for_budget(measured_mem, u64::MAX) <= n,
                "a budget of exactly the measured cost for {n} must never admit more than {n}"
            );
        }
        // 4 listings measured 18.10M, so mainnet's 16.5M must NOT admit them —
        // this is the case that reached the node as ExUnitsTooBigUTxO.
        assert!(max_buys_for_budget(16_500_000, u64::MAX) < 4);
        // …and a budget with genuine room does admit them.
        assert!(max_buys_for_budget(25_000_000, u64::MAX) >= 4);
    }

    /// Never zero: a caller uses this as a batch size, and 0 would drop the
    /// cart on the floor rather than build one listing at a time.
    #[test]
    fn sweep_cap_is_never_zero() {
        assert_eq!(max_buys_for_budget(0, 0), 1);
        assert_eq!(max_buys_for_budget(1, 1), 1);
    }

    /// Conway charges for every reference script a TX reads, and a sweep over
    /// ONE generation reads its validator once however many listings it spends.
    /// Billing per listing would over-pay; billing zero (what buys did) is
    /// rejected at submit with `FeeTooSmallUTxO` — and evaluation never checks
    /// fees, so a wrong figure here is invisible until the node sees it.
    #[test]
    fn reference_script_is_billed_once_per_generation() {
        use crate::builder::marketplace::DatumPayout;

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
                address: buyer(),
                lovelace,
            }],
            marketplace_version: MarketplaceType::JpgStoreV2,
            script_ref: test_script_ref(),
        };

        assert_eq!(
            reference_script_size(&[listing(1_000_000)]),
            1673,
            "a single V2 buy bills the validator once"
        );
        assert_eq!(
            reference_script_size(&[
                listing(1_000_000),
                listing(2_000_000),
                listing(3_000_000)
            ]),
            1673,
            "a 3-listing sweep reads ONE validator, so it bills 1673 — not 3×"
        );
        assert_eq!(
            reference_script_size(&[]),
            0,
            "no listings, nothing referenced"
        );
    }

    /// A sweep spanning two generations reads two different validators, so it
    /// pays for both. Deduplicating by hash must not collapse them.
    #[test]
    fn distinct_generations_each_bill_their_own_reference_script() {
        use crate::builder::marketplace::DatumPayout;

        let listing = |version, size| ParsedListing {
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
                lovelace: 1_000_000,
            }],
            marketplace_version: version,
            script_ref: crate::builder::marketplace::ScriptRefInfo {
                language: pallas_txbuilder::ScriptKind::PlutusV2,
                size,
            },
        };

        // Real sizes from Koios: V1's script is 2561 B, V2/V3's is 1673 B.
        assert_eq!(
            reference_script_size(&[
                listing(MarketplaceType::JpgStoreV1, 2561),
                listing(MarketplaceType::JpgStoreV2, 1673),
            ]),
            2561 + 1673,
        );
    }

    fn deps() -> BuyDeps {
        BuyDeps {
            buyer_utxos: vec![],
            params: TxBuildParams::default(),
            buyer_address: buyer(),
            network_id: 0,
            collateral_utxo: None,
            validity_slots: None,
        }
    }

    #[test]
    fn empty_listings_is_rejected() {
        assert!(build_buy(&deps(), &[]).is_err());
    }

    /// Payouts to the SAME address across two listings must stay two outputs.
    ///
    /// An earlier revision merged them, reasoning that a validator checks the
    /// total received per address. jpg V2/V3 do not: the redeemer carries the
    /// index of a listing's first payout output and the validator reads forward
    /// from there, so a coalesced output silently shifts every later listing's
    /// offset. Merging is a saving of one output that costs the whole sweep.
    #[test]
    fn payouts_to_the_same_address_are_not_merged_across_listings() {
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
            script_ref: test_script_ref(),
        };

        // V1 charges no separate fee output, so each block is just its payouts.
        let listings = [listing(100), listing(250)];
        let blocks = build_settlement_blocks(&listings, 0, &TxBuildParams::default()).unwrap();
        let outputs: Vec<_> = blocks.iter().flat_map(|b| b.outputs.iter()).collect();
        assert_eq!(
            outputs.len(),
            2,
            "each listing keeps its own payout outputs so redeemer offsets stay valid"
        );
        assert_eq!(outputs[0].lovelace, 100);
        assert_eq!(outputs[1].lovelace, 250);
    }

    /// The `datum_tag` must match jpg's contract byte for byte.
    ///
    /// `validators/ask.ak`: `out_ref |> serialise_data |> blake2b_256`. Plutus
    /// writes constructors with **indefinite-length** field lists, so the
    /// preimage is `d8799f d8799f 5820<txid> ff <idx> ff`. The definite-length
    /// spelling hashes to something else and the spend is rejected with a bare
    /// `(error)` naming nothing.
    ///
    /// Vector below is a real mainnet buy: listing `2b73907e…#0` from tx
    /// `7a06a655…`, whose fee output carries exactly this tag.
    #[test]
    fn fee_datum_tag_matches_the_contract() {
        use crate::builder::marketplace::DatumPayout;

        const OREF_TX: &str = "2b73907e2e0f1e9dbd5a4b4e8b3f5c8a3f1c1e6e2d9a7b4c5d3e2f1a0b9c8d7e";
        let listing = ParsedListing {
            utxo: UtxoApi {
                tx_hash: OREF_TX.to_string(),
                output_index: 0,
                lovelace: 1_000_000,
                assets: vec![],
                tags: vec![],
            },
            datum_cbor: vec![],
            datum_is_inline: false,
            payouts: vec![DatumPayout {
                address: buyer(),
                lovelace: 1_000_000,
            }],
            marketplace_version: MarketplaceType::JpgStoreV2,
            script_ref: test_script_ref(),
        };

        let tag = fee_output_datum_tag(&listing).unwrap();
        // A PlutusData byte string of 32 bytes.
        assert_eq!(&tag[..2], &[0x58, 0x20], "tag must be bytes(32)");
        assert_eq!(tag.len(), 34);

        // Recomputing the documented preimage independently must agree.
        use pallas_crypto::hash::Hasher;
        let mut preimage = vec![0xd8, 0x79, 0x9f, 0xd8, 0x79, 0x9f, 0x58, 0x20];
        preimage.extend_from_slice(&hex::decode(OREF_TX).unwrap());
        preimage.extend_from_slice(&[0xff, 0x00, 0xff]);
        assert_eq!(&tag[2..], Hasher::<256>::hash(&preimage).as_ref());
    }

    /// The fee must reproduce the contract's integer arithmetic exactly:
    /// `payouts_sum * 50 / 49 / 50`. The check is `quantity >= marketplace_fee`,
    /// so computing a cleaner equivalent that lands one lovelace low fails.
    #[test]
    fn marketplace_fee_matches_the_contract_arithmetic() {
        let fee = MarketplaceType::JpgStoreV2.marketplace_fee().unwrap();

        // Real buy `556db775…`: 470.4 ADA of payouts, 9.6 ADA fee on chain.
        assert_eq!(fee.due_on_payouts(470_400_000), 9_600_000);

        // The contract's own expression, evaluated independently.
        for payouts in [1u64, 4_000_000, 23_000_000, 470_400_000, 1_000_000_000] {
            let expected = payouts * 50 / 49 / 50;
            assert_eq!(
                fee.due_on_payouts(payouts),
                expected,
                "fee on {payouts} must equal `sum * 50 / 49 / 50`"
            );
        }
    }

    /// Two contract generations cannot share one transaction — their payout
    /// layouts are mutually unsatisfiable. Measured: a mixed sweep evaluates
    /// with the V2 spend passing and both V1 spends failing.
    #[test]
    fn mixed_contract_versions_are_refused() {
        use crate::builder::marketplace::DatumPayout;

        let listing = |version| ParsedListing {
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
                lovelace: 1_000_000,
            }],
            marketplace_version: version,
            script_ref: test_script_ref(),
        };

        let err = build_buy(
            &deps(),
            &[
                listing(MarketplaceType::JpgStoreV1),
                listing(MarketplaceType::JpgStoreV2),
            ],
        )
        .unwrap_err();
        assert!(
            format!("{err:?}").contains("Cannot mix"),
            "mixing versions must fail at build, not at evaluation: {err:?}"
        );
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
            script_ref: test_script_ref(),
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
            // Must encode to something a validator can decode, at any offset.
            assert!(
                redeemer.encode(0).len() >= 3,
                "{version:?} redeemer must encode"
            );
            assert!(!redeemer.encode(7).is_empty());
        }
    }

    /// Payout outputs must be contiguous per listing, and the offsets handed to
    /// the redeemers must point at each listing's first payout.
    ///
    /// This is what forbids merging payouts by address across listings: jpg
    /// V2/V3 read outputs starting at the offset in their redeemer, so a
    /// coalesced output makes every later offset wrong.
    #[test]
    fn payout_offsets_track_per_listing_layout() {
        use crate::builder::marketplace::DatumPayout;

        let listing = |n: u64| ParsedListing {
            utxo: UtxoApi {
                tx_hash: "a".repeat(64),
                output_index: 0,
                lovelace: 1_000_000,
                assets: vec![],
                tags: vec![],
            },
            datum_cbor: vec![],
            datum_is_inline: false,
            payouts: (0..n)
                .map(|i| DatumPayout {
                    address: buyer(),
                    lovelace: 1_000_000 + i,
                })
                .collect(),
            marketplace_version: MarketplaceType::JpgStoreV2,
            script_ref: test_script_ref(),
        };

        // V2 blocks are `[fee, payout…]`, so with one leading buyer output and
        // 3/2/1 payouts the blocks start at 1, 5 and 8. Each redeemer names its
        // own fee output — verified on chain against a 6-listing buy whose
        // indices were exactly its fee-output positions.
        const LEADING: usize = 1;
        let params = TxBuildParams {
            coins_per_utxo_byte: 4310,
            ..TxBuildParams::default()
        };

        let listings = [listing(3), listing(2), listing(1)];
        let blocks = build_settlement_blocks(&listings, LEADING, &params).unwrap();

        let indices: Vec<usize> = blocks.iter().map(|b| b.redeemer_index).collect();
        assert_eq!(indices, vec![1, 5, 8], "each redeemer names its fee output");

        // Flatten as the builder does and confirm each index really lands on a
        // fee output, with that listing's first payout immediately after.
        let flat: Vec<_> = blocks.iter().flat_map(|b| b.outputs.iter()).collect();
        // (fee + 3 payouts) + (fee + 2) + (fee + 1).
        assert_eq!(
            flat.len(),
            4 + 3 + 2,
            "one fee per listing plus its payouts"
        );
        for (listing_idx, start) in indices.iter().enumerate() {
            let first_payout = flat[start + 1 - LEADING];
            assert_eq!(
                first_payout.lovelace, listings[listing_idx].payouts[0].lovelace,
                "listing {listing_idx}'s payouts must begin at index+1"
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
