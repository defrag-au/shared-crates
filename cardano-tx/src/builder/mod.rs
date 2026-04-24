//! High-level transaction builders.
//!
//! Pure functions that take intents + dependencies and produce [`UnsignedTx`] values.
//! No IO — consumers provide UTxOs and protocol parameters, get back a staged
//! transaction ready for signing via Ed25519 or CIP-30.

pub mod buy;
pub mod collection_offer;
pub mod cost_models;
pub mod fluent;
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
    // Round 1: build with rough estimate, calculate real fee
    let preliminary_tx = build_fn(initial_estimate)?;
    let fee_round1 = crate::fee::calculate_fee(&preliminary_tx, params);

    // Round 2: rebuild with round-1 fee, recalculate
    let tx_round2 = build_fn(fee_round1)?;
    let final_fee = crate::fee::calculate_fee(&tx_round2, params);

    // Final build with converged fee
    let staging = build_fn(final_fee)?;

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
}
