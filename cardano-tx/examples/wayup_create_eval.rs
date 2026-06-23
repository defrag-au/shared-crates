//! One-off: build an UNSIGNED Wayup collection-offer CREATE TX and dump its
//! CBOR + a structural summary. Confirms the offer output sits at the bidder
//! frankenaddress with an INLINE datum (byte-exact to Wayup's format), that no
//! script-data hash is produced (datum-only TXs don't need one when inline),
//! and that the `674` marker label is attached.
//!
//! Run: `cargo run -p cardano-tx --example wayup_create_eval`

use cardano_assets::utxo::UtxoApi;
use cardano_tx::builder::collection_offer::{
    build_wayup_collection_offers_tx, CollectionOfferRequest,
};
use cardano_tx::builder::TxDeps;
use cardano_tx::params::TxBuildParams;
use pallas_addresses::{Address, Network};
use pallas_txbuilder::BuildConway;

fn main() {
    let wallet = "addr1q99qpegypskhugq6nss8gjkwvjlj35wd54venfynreqxjgkt55dzuk6msq4qgq4g0dmz6tkvdvdfkmgd6uyd4urms2ksl0yc60";
    let from_address = Address::from_bech32(wallet).expect("wallet bech32");

    let funding = UtxoApi {
        tx_hash: "98e91808cf24f6e598fc41ff42f704722ef36730e6ee89e62e8e7133c7c88a76".to_string(),
        output_index: 1,
        lovelace: 200_000_000,
        assets: vec![],
        tags: vec![],
    };

    // Two collection offers (10 ADA + 25 ADA) on the same collection so we
    // exercise the multi-offer path. Same bidder → same frankenaddress.
    let mk = |lovelace: u64| CollectionOfferRequest {
        policy_id: "a316bcf768f0309be743b4b7d067f3348017bf0f00f6a29562aebda2".into(),
        total_lovelace: lovelace,
        buyer_pkh: "4a00e5040c2d7e201a9c20744ace64bf28d1cda55999a4931e406922".into(),
        buyer_stake_hash: Some("cba51a2e5b5b802a0402a87b762d2ecc6b1a9b6d0dd708daf07b82ad".into()),
        royalty_pct: 0.05,
        royalty_pkh: "04bdd97da6dfacd1fb4c5d5e1a14292e4ee0f2015a46f8543e552c49".into(),
        royalty_stake_hash: Some("8eaef6e3337032021c887b180b74dfc0ff625d167df78b77e476b68d".into()),
        royalty_is_key: true,
        royalty_stake_is_key: true,
        network: Network::Mainnet,
    };

    let params = TxBuildParams {
        min_fee_coefficient: 44,
        min_fee_constant: 155_381,
        coins_per_utxo_byte: 4_310,
        max_tx_size: 16_384,
        max_value_size: 5_000,
        price_mem: Some((577, 10_000)),
        price_step: Some((721, 10_000_000)),
        min_fee_ref_script_cost_per_byte: 15,
        ref_script_size: 0,
        cost_models: cardano_tx::builder::cost_models::PlutusCostModels::EMPTY,
    };

    let deps = TxDeps {
        utxos: vec![funding],
        params,
        from_address,
        network_id: 1,
    };

    let unsigned = build_wayup_collection_offers_tx(&deps, &[mk(10_000_000), mk(25_000_000)])
        .expect("build wayup create");
    let built = unsigned
        .staging
        .build_conway_raw()
        .expect("build_conway_raw");

    eprintln!("converged fee: {} lovelace", unsigned.fee);
    println!("{}", hex::encode(&built.tx_bytes.0));
}
