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
