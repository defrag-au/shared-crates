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
    let tx_hash_bytes = decode_tx_hash(&utxo.tx_hash)?;
    Ok(tx.input(Input::new(
        Hash::from(tx_hash_bytes),
        utxo.output_index as u64,
    )))
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
