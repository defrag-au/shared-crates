//! Transaction input construction helpers

use cardano_assets::UtxoApi;
use pallas_crypto::hash::Hash;
use pallas_txbuilder::{Input, StagingTransaction};

use crate::error::TxBuildError;
use crate::helpers::decode::decode_tx_hash;

/// Add a single UTxO as an input to a staging transaction.
pub fn add_utxo_input(
    tx: StagingTransaction,
    utxo: &UtxoApi,
) -> Result<StagingTransaction, TxBuildError> {
    add_input_ref(tx, &utxo.tx_hash, utxo.output_index)
}

/// Add an input by bare reference (`tx_hash` hex + output index) — for builders
/// that work over the `Selectable` abstraction (see [`crate::plan`]) rather than a
/// concrete [`UtxoApi`]. A Cardano input is just a `tx_id#index` reference (the
/// value is resolved on-chain), so this is all the staging layer needs.
pub fn add_input_ref(
    tx: StagingTransaction,
    tx_hash: &str,
    output_index: u32,
) -> Result<StagingTransaction, TxBuildError> {
    let tx_hash_bytes = decode_tx_hash(tx_hash)?;
    Ok(tx.input(Input::new(Hash::from(tx_hash_bytes), output_index as u64)))
}

/// Add multiple UTxOs as inputs to a staging transaction.
pub fn add_utxo_inputs(
    mut tx: StagingTransaction,
    utxos: &[&UtxoApi],
) -> Result<StagingTransaction, TxBuildError> {
    for utxo in utxos {
        tx = add_utxo_input(tx, utxo)?;
    }
    Ok(tx)
}
