//! High-level transaction builders.
//!
//! Pure functions that take intents + dependencies and produce [`UnsignedTx`] values.
//! No IO — consumers provide UTxOs and protocol parameters, get back a staged
//! transaction ready for signing via Ed25519 or CIP-30.

pub mod buy;
pub mod collection_offer;
pub mod cost_models;
pub mod debag;
pub mod fluent;
pub mod listing;
pub mod marketplace;
pub mod mint;
pub mod script;
pub mod send;
pub mod swap;

use cardano_assets::UtxoApi;
use pallas_addresses::Address;
use pallas_txbuilder::StagingTransaction;

use crate::error::TxBuildError;
use crate::params::TxBuildParams;

/// Pure dependencies for transaction building — no IO, no worker types.
///
/// Consumers construct this from their platform-specific types:
/// - Workers: convert from `SendParams` (AddressUtxo → UtxoApi, ProtocolParameters → TxBuildParams)
/// - Browser: from CIP-30 wallet UTxOs + fetched protocol params
/// - CLI: from any indexer API
#[derive(Clone)]
pub struct TxDeps {
    pub utxos: Vec<UtxoApi>,
    pub params: TxBuildParams,
    pub from_address: Address,
    pub network_id: u8,
}

/// A fully assembled but unsigned transaction.
///
/// The staging TX has fee and network_id already set. Consumers choose how to sign:
/// - Server: `build_conway_raw()` → `sign_transaction(built, &secret_key)`
/// - Browser: extract CBOR hex → pass to CIP-30 `wallet.signTx()`
pub struct UnsignedTx {
    pub staging: StagingTransaction,
    pub fee: u64,
}

impl std::fmt::Debug for UnsignedTx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnsignedTx")
            .field("fee", &self.fee)
            .finish_non_exhaustive()
    }
}

/// A signed transaction ready for submission — hex tx hash + hex CBOR.
/// Produced by [`UnsignedTx::build_and_sign`].
pub struct SignedTx {
    pub tx_hash: String,
    pub tx_cbor_hex: String,
}

/// A UTxO reference (`tx_hash#index`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UtxoRef {
    pub tx_hash: String,
    pub output_index: u32,
}

/// One output a built tx creates — address + value + whether it carries native
/// assets, with its position (output index).
#[derive(Debug, Clone)]
pub struct TxOutputRef {
    pub output_index: u32,
    /// Bech32 destination. A caller maintaining a wallet ledger keeps only the
    /// outputs whose address is its own wallet (change / parcels / self-sends).
    pub address: String,
    pub lovelace: u64,
    pub has_assets: bool,
}

/// The ledger-relevant **effects** of a built transaction: exactly what it spends
/// (`spent`) and exactly what it creates (`outputs`). Because a built tx is
/// deterministic, these are exact and known BEFORE submit — so a caller can
/// maintain a local wallet-UTxO ledger with no chain round-trip (mark `spent`
/// consumed, insert the self-`outputs` as new UTxOs). See
/// `cnft.dev-workers/docs/design/WALLET_UTXO_LEDGER.md`.
#[derive(Debug, Clone)]
pub struct TxEffects {
    pub tx_hash: String,
    pub spent: Vec<UtxoRef>,
    pub outputs: Vec<TxOutputRef>,
}

/// Read the spent-input refs off a staged tx (clean `tx_hash#index`).
fn staging_spent(staging: &StagingTransaction) -> Vec<UtxoRef> {
    staging
        .inputs
        .iter()
        .flatten()
        .map(|i| UtxoRef {
            tx_hash: hex::encode(i.tx_hash.0),
            output_index: i.txo_index as u32,
        })
        .collect()
}

/// Read the created outputs off a staged tx (address + value + asset flag, indexed).
fn staging_outputs(staging: &StagingTransaction) -> Vec<TxOutputRef> {
    staging
        .outputs
        .iter()
        .flatten()
        .enumerate()
        .map(|(ix, o)| TxOutputRef {
            output_index: ix as u32,
            address: o.address.0.to_bech32().unwrap_or_default(),
            lovelace: o.lovelace,
            has_assets: o.assets.is_some(),
        })
        .collect()
}

impl UnsignedTx {
    /// Build the Conway-era tx + sign it with an Ed25519 key, returning
    /// the hex tx hash and CBOR. The staging tx already carries `fee` +
    /// `network_id` (set during construction), so this is just the
    /// final assembly + signature — the server-side counterpart to
    /// handing the CBOR to a CIP-30 wallet in the browser.
    ///
    /// Keeps the `pallas_txbuilder` build trait internal to this crate
    /// so consumers (workers, CLIs) don't have to depend on it directly
    /// just to turn an [`UnsignedTx`] into submittable bytes.
    pub fn build_and_sign(
        self,
        secret_key: &pallas_crypto::key::ed25519::SecretKeyExtended,
    ) -> Result<SignedTx, TxBuildError> {
        use pallas_txbuilder::BuildConway;
        let built = self
            .staging
            .build_conway_raw()
            .map_err(|e| TxBuildError::BuildFailed(e.to_string()))?;
        let signed = crate::sign::sign_transaction(built, secret_key)?;
        Ok(SignedTx {
            tx_hash: hex::encode(signed.tx_hash.0),
            tx_cbor_hex: hex::encode(&signed.tx_bytes),
        })
    }

    /// Build + sign with MULTIPLE keys — for a tx whose inputs span more than one of
    /// the engine's addresses (e.g. a Mode-B refund spending un-split payments at the
    /// deposit address `D` AND orphaned parcels at the operational address `O`). One
    /// vkey witness per key; pass only the keys actually required. See
    /// `cnft.dev-workers/docs/design/WALLET_UTXO_LEDGER.md`.
    pub fn build_and_sign_multi(
        self,
        secret_keys: &[&pallas_crypto::key::ed25519::SecretKeyExtended],
    ) -> Result<SignedTx, TxBuildError> {
        use pallas_txbuilder::BuildConway;
        let built = self
            .staging
            .build_conway_raw()
            .map_err(|e| TxBuildError::BuildFailed(e.to_string()))?;
        let signed = crate::sign::sign_transaction_multi(built, secret_keys)?;
        Ok(SignedTx {
            tx_hash: hex::encode(signed.tx_hash.0),
            tx_cbor_hex: hex::encode(&signed.tx_bytes),
        })
    }

    /// Build + sign AND return the tx's [`TxEffects`] (spent inputs + created
    /// outputs) for a caller maintaining a local wallet-UTxO ledger. The spent /
    /// output sets are read off the staged tx before assembly, so they exactly
    /// match what lands on chain. Use this on every flow whose inputs are selected
    /// internally (send / refund / dust / split); the mint builder surfaces its own
    /// `ChangeUtxo` so it doesn't need this.
    pub fn build_and_sign_tracked(
        self,
        secret_key: &pallas_crypto::key::ed25519::SecretKeyExtended,
    ) -> Result<(SignedTx, TxEffects), TxBuildError> {
        let spent = staging_spent(&self.staging);
        let outputs = staging_outputs(&self.staging);
        let signed = self.build_and_sign(secret_key)?;
        let effects = TxEffects {
            tx_hash: signed.tx_hash.clone(),
            spent,
            outputs,
        };
        Ok((signed, effects))
    }

    /// [`build_and_sign_multi`] + the tx's [`TxEffects`] — the multi-key analogue of
    /// [`build_and_sign_tracked`], for a wallet-ledger caller whose tx spends inputs
    /// across more than one of its addresses (e.g. the Mode-B refund over `D` + `O`).
    pub fn build_and_sign_multi_tracked(
        self,
        secret_keys: &[&pallas_crypto::key::ed25519::SecretKeyExtended],
    ) -> Result<(SignedTx, TxEffects), TxBuildError> {
        let spent = staging_spent(&self.staging);
        let outputs = staging_outputs(&self.staging);
        let signed = self.build_and_sign_multi(secret_keys)?;
        let effects = TxEffects {
            tx_hash: signed.tx_hash.clone(),
            spent,
            outputs,
        };
        Ok((signed, effects))
    }
}

/// Two-round fee convergence.
///
/// The Cardano fee depends on TX size, which depends on the fee field itself (circular).
/// This function resolves it by building twice: once with a rough estimate to get the
/// real fee, then again with the calculated fee. Two rounds suffice because the fee
/// field's CBOR size changes by at most 1 byte between rounds.
///
/// `build_fn` receives a fee and must return a complete `StagingTransaction` with that
/// fee and network_id set.
pub fn converge_fee(
    build_fn: impl Fn(u64) -> Result<StagingTransaction, TxBuildError>,
    initial_estimate: u64,
    params: &TxBuildParams,
) -> Result<UnsignedTx, TxBuildError> {
    converge_fee_with_witnesses(build_fn, initial_estimate, params, 1)
}

/// [`converge_fee`] that sizes the fee for `num_witnesses` vkey signatures.
///
/// Use this when the transaction needs a required-signer whose key differs
/// from the wallet payer's (e.g. a Wayup cancel, authorised by the bidder's
/// stake key). See [`crate::fee::calculate_fee_with_witnesses`].
pub fn converge_fee_with_witnesses(
    build_fn: impl Fn(u64) -> Result<StagingTransaction, TxBuildError>,
    initial_estimate: u64,
    params: &TxBuildParams,
    num_witnesses: u32,
) -> Result<UnsignedTx, TxBuildError> {
    // Round 1: build with rough estimate, calculate real fee
    let preliminary_tx = build_fn(initial_estimate)?;
    let fee_round1 =
        crate::fee::calculate_fee_with_witnesses(&preliminary_tx, params, num_witnesses);

    // Round 2: rebuild with round-1 fee, recalculate
    let tx_round2 = build_fn(fee_round1)?;
    let final_fee = crate::fee::calculate_fee_with_witnesses(&tx_round2, params, num_witnesses);

    // Final build with converged fee
    let staging = build_fn(final_fee)?;

    // `maxTxSize` — the other limit nothing checks before submit. The evaluator
    // runs the scripts and says nothing about size, so an oversized batch
    // evaluates clean and the node rejects it with `MaxTxSizeUTxO`.
    //
    // Guarded here rather than in any one builder because it is a property of
    // every transaction, and because this is the one place that holds the FINAL
    // bytes: a size measured before fee convergence is the wrong size.
    //
    // A cap of 0 means "no ceiling", and is a DELIBERATE opt-out: the default
    // is `CONWAY_MAX_TX_SIZE`, not zero, precisely so that forgetting to set
    // protocol parameters leaves the guard on rather than off.
    if params.max_tx_size > 0
        && let Some(size) = crate::fee::signed_tx_size(&staging, num_witnesses)
    {
        let cap = u64::from(params.max_tx_size);
        if size > cap {
            return Err(TxBuildError::TxSizeExceeded { size, cap });
        }
    }

    Ok(UnsignedTx {
        staging,
        fee: final_fee,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_converge_fee_produces_result() {
        let params = TxBuildParams {
            min_fee_coefficient: 44,
            min_fee_constant: 155381,
            coins_per_utxo_byte: 4310,
            max_tx_size: 16384,
            max_value_size: 5000,
            price_mem: None,
            price_step: None,
            ..Default::default()
        };

        // Minimal TX that builds successfully
        let from_addr =
            Address::from_bech32("addr_test1qz2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3jcu5d8ps7zex2k2xt3uqxgjqnnj83ws8lhrn648jjxtwq2ytjqp")
                .unwrap();
        let to_addr = from_addr.clone();

        let result = converge_fee(
            |fee| {
                let amount = 2_000_000u64;
                let tx = StagingTransaction::new()
                    .output(crate::helpers::output::create_ada_output(
                        to_addr.clone(),
                        amount,
                    ))
                    .output(crate::helpers::output::create_ada_output(
                        from_addr.clone(),
                        5_000_000 - amount - fee,
                    ))
                    .fee(fee)
                    .network_id(0);
                Ok(tx)
            },
            200_000,
            &params,
        );

        assert!(result.is_ok());
        let unsigned = result.unwrap();
        assert!(unsigned.fee > 0);
        assert!(unsigned.fee < 1_000_000); // Sanity check — simple TX fee shouldn't be huge
    }

    /// A two-output transfer, built against caps that do and don't admit it.
    ///
    /// Shared by the `maxTxSize` guard tests so the only difference between
    /// them is the cap itself.
    fn build_with_size_cap(max_tx_size: u32) -> Result<UnsignedTx, TxBuildError> {
        let params = TxBuildParams {
            min_fee_coefficient: 44,
            min_fee_constant: 155381,
            coins_per_utxo_byte: 4310,
            max_tx_size,
            max_value_size: 5000,
            ..Default::default()
        };
        let addr = Address::from_bech32("addr_test1qz2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3jcu5d8ps7zex2k2xt3uqxgjqnnj83ws8lhrn648jjxtwq2ytjqp")
            .unwrap();

        converge_fee(
            |fee| {
                Ok(StagingTransaction::new()
                    .output(crate::helpers::output::create_ada_output(
                        addr.clone(),
                        2_000_000,
                    ))
                    .output(crate::helpers::output::create_ada_output(
                        addr.clone(),
                        3_000_000 - fee,
                    ))
                    .fee(fee)
                    .network_id(0))
            },
            200_000,
            &params,
        )
    }

    /// A transaction over `maxTxSize` must fail the BUILD, not the submit.
    ///
    /// Nothing downstream catches this: `evaluateTransaction` runs the scripts
    /// and never looks at size, so without this guard an oversized sweep is
    /// reported as "EVALUATED OK" and then rejected by the node with
    /// `MaxTxSizeUTxO`. Measured on mainnet, a 50-listing jpg V2 sweep
    /// serialises to 21,132 bytes against the 16,384 limit while sitting at
    /// only 96% of the execution budget — size binds first, so the execution
    /// guard alone does not cover this.
    #[test]
    fn a_transaction_over_max_tx_size_is_refused_at_build() {
        // Far below anything serialisable, so the cap is unambiguously the
        // thing that rejects it.
        match build_with_size_cap(200) {
            Err(TxBuildError::TxSizeExceeded { size, cap }) => {
                assert_eq!(cap, 200, "the guard must report the cap it applied");
                assert!(size > cap, "size {size} should exceed cap {cap}");
            }
            other => panic!("expected TxSizeExceeded, got {other:?}"),
        }
    }

    /// `max_tx_size: 0` is the explicit opt-out, and must still be honoured.
    #[test]
    fn a_zero_max_tx_size_opts_out_of_the_guard() {
        assert!(
            build_with_size_cap(0).is_ok(),
            "an explicit cap of 0 must not reject the transaction"
        );
    }

    /// Defaulted params must be BOUNDED.
    ///
    /// The size ceiling is the one nothing downstream reports — the evaluator
    /// never looks at size — so a caller that forgets to supply protocol
    /// parameters must end up guarded, not unguarded. This is the whole reason
    /// `Default` is hand-written instead of derived; a future field added to
    /// the derive would silently reinstate `max_tx_size: 0`.
    #[test]
    fn default_params_carry_a_real_size_cap() {
        assert_eq!(
            TxBuildParams::default().max_tx_size,
            crate::params::CONWAY_MAX_TX_SIZE,
            "defaulted params must not be unbounded"
        );
        assert_ne!(TxBuildParams::default().max_tx_size, 0);
    }

    /// The guard measures the SIGNED size, which is what the ledger checks.
    ///
    /// The unsigned CBOR a builder returns omits the vkey witnesses — ~100
    /// bytes each — so a guard written against it would pass transactions the
    /// node then rejects.
    #[test]
    fn signed_size_exceeds_unsigned_size() {
        use pallas_txbuilder::BuildConway;

        let unsigned = build_with_size_cap(16384).expect("builds");
        let unsigned_len = unsigned
            .staging
            .clone()
            .build_conway_raw()
            .unwrap()
            .tx_bytes
            .0
            .len() as u64;
        let signed_len = crate::fee::signed_tx_size(&unsigned.staging, 1).expect("measurable");
        assert!(
            signed_len > unsigned_len,
            "signed {signed_len} should exceed unsigned {unsigned_len} by the witness bytes"
        );
    }
}
