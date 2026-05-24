//! One-off: build an UNSIGNED Wayup collection-offer cancel TX
//! against real mainnet inputs and print its CBOR, so it can be
//! fed to Maestro `/transactions/evaluate`. Confirms the cancel
//! validates WITHOUT the 1 ADA courtesy fee and reports the real
//! per-cancel ex-units.
//!
//! Inputs are real, unspent as of 2026-05-24 (bidder cba51a…):
//!   - offer  5a6c5f13…:0  (120 ADA, Perp3876 asset-offer)
//!   - fee    98e91808…:1  (~4694 ADA, pure ADA — fee + collateral)
//!
//! Run: `cargo run -p cardano-tx --example wayup_cancel_eval`

use cardano_assets::utxo::UtxoApi;
use cardano_tx::builder::collection_offer::{
    build_cancel_offers_tx_with, CancelContract, CancelOfferRequest,
};
use cardano_tx::builder::TxDeps;
use cardano_tx::params::TxBuildParams;
use pallas_addresses::Address;
use pallas_txbuilder::BuildConway;

fn main() {
    let wallet = "addr1q99qpegypskhugq6nss8gjkwvjlj35wd54venfynreqxjgkt55dzuk6msq4qgq4g0dmz6tkvdvdfkmgd6uyd4urms2ksl0yc60";
    let from_address = Address::from_bech32(wallet).expect("wallet bech32");

    // Real pure-ADA wallet UTxO for fee + collateral.
    let fee_utxo = UtxoApi {
        tx_hash: "98e91808cf24f6e598fc41ff42f704722ef36730e6ee89e62e8e7133c7c88a76".to_string(),
        output_index: 1,
        lovelace: 4_694_828_471,
        assets: vec![],
        tags: vec![],
    };

    // Real unspent Wayup offer owned by cba51a (datum field 0 =
    // cba51a stake cred = the required cancel signer).
    let datum_cbor_hex = "d8799f581ccba51a2e5b5b802a0402a87b762d2ecc6b1a9b6d0dd708daf07b82ad9fd8799fd8799fd8799f581c5f08a64f580e581735070e1b1d2ce29ae6942ab45ccff5a1747d2283ffd8799fd8799fd8799f581c28f17fdd2d8b8f559ad61e899e31eae90b9e209cbedb1ee8a8c6c7d1ffffffffa140d8799f00a1401a00249f00ffffd8799fd8799fd8799f581c4a00e5040c2d7e201a9c20744ace64bf28d1cda55999a4931e406922ffd8799fd8799fd8799f581ccba51a2e5b5b802a0402a87b762d2ecc6b1a9b6d0dd708daf07b82adffffffffa1581ce6ba9c0ff27be029442c32533c6efd956a60d15ecb976acbb64c4de0d8799f00a148506572703338373601ffffd8799fd8799fd8799f581cf00407b340f62c8327c0c2737e41514d715b60664aca3b65bdce6c90ffd8799fd8799fd8799f581ca234f55927f22416761471cb23a7d4faa0546c882c1f7570013dfce7ffffffffa140d8799f00a1401a00b71b00ffffffff";

    let req = CancelOfferRequest {
        co_tx_hash: "5a6c5f13884dcdeb41ecd6eeb9589e05db61738955565d02bea829ccea877923".to_string(),
        co_output_index: 0,
        co_lovelace: 120_000_000,
        // Wayup cancel signer = the bidder's STAKE credential.
        owner_pkh: "cba51a2e5b5b802a0402a87b762d2ecc6b1a9b6d0dd708daf07b82ad".to_string(),
        datum_cbor_hex: Some(datum_cbor_hex.to_string()),
        ex_units_mem: None,
        ex_units_steps: None,
    };

    // Mainnet protocol params (stable values + live ex-unit prices).
    let params = TxBuildParams {
        min_fee_coefficient: 44,
        min_fee_constant: 155_381,
        coins_per_utxo_byte: 4_310,
        max_tx_size: 16_384,
        max_value_size: 5_000,
        price_mem: Some((577, 10_000)),
        price_step: Some((721, 10_000_000)),
        min_fee_ref_script_cost_per_byte: 15,
        ref_script_size: 2_557, // Wayup PlutusV2 script: 2560 CBOR bytes - 3-byte bytestring header
    };

    let deps = TxDeps {
        utxos: vec![fee_utxo],
        params,
        from_address,
        network_id: 1,
    };

    let unsigned = build_cancel_offers_tx_with(&deps, &[req], &CancelContract::wayup())
        .expect("build wayup cancel");
    let built = unsigned
        .staging
        .build_conway_raw()
        .expect("build_conway_raw");

    eprintln!("converged fee: {} lovelace", unsigned.fee);
    println!("{}", hex::encode(&built.tx_bytes.0));
}
