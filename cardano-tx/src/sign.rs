//! Pure transaction signing
//!
//! Ed25519 signing for built transactions. No IO — just cryptography.

use pallas_crypto::key::ed25519::SecretKey;
use pallas_txbuilder::BuiltTransaction;

use crate::error::TxBuildError;

/// Sign a built transaction with an Ed25519 secret key.
pub fn sign_transaction(
    tx: BuiltTransaction,
    secret_key: &SecretKey,
) -> Result<BuiltTransaction, TxBuildError> {
    tx.sign(secret_key)
        .map_err(|e| TxBuildError::SignFailed(format!("{e}")))
}

/// Sign a built transaction with MULTIPLE Ed25519 keys — one vkey witness per key.
/// Needed when a tx's required signers span more than one of the engine's keys (e.g.
/// a mint spending a funding UTxO at the operational address `O` while its policy
/// requires the deposit key `D` — see docs/design/WALLET_UTXO_LEDGER.md). `.sign()`
/// is chainable + appends a witness per call, so this just folds over the keys.
/// Pass only the keys actually required (de-duplicated) — an unneeded witness is
/// ~101 wasted bytes, not an error. An empty slice returns the tx unsigned.
pub fn sign_transaction_multi(
    tx: BuiltTransaction,
    secret_keys: &[&SecretKey],
) -> Result<BuiltTransaction, TxBuildError> {
    let mut tx = tx;
    for sk in secret_keys {
        // `sk` is `&&SecretKey` (iterating `&[&SecretKey]`); `.sign` wants `&S`.
        tx = tx
            .sign(*sk)
            .map_err(|e| TxBuildError::SignFailed(format!("{e}")))?;
    }
    Ok(tx)
}
