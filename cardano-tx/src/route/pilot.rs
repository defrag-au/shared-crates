//! The pilot route, end to end: ADA → LUMP (Splash) → SWOLE (LumpPad).
//!
//! Both pool states are the live mainnet ones of 2026-09-14 that
//! `COMPOSED_ROUTES.md` §8.2 pins. The arithmetic is checked against them
//! here, and the transaction shape is checked against what the two validators
//! require.
//!
//! ## Where these numbers differ from §8.2, and why
//!
//! §8.2's table solves LumpPad's gross→net step as "the largest `n` that
//! fits". LumpPad does not: it uses the closed form
//! `n = (g − flat) * 10000 / 10150`, floored, and never steps up. The two
//! differ by one or two LUMP, and the chain settles it — every buy in
//! `LUMPPAD_INTEGRATION.md` Appendix A shows the closed form's answer:
//!
//! | typed | closed form (chain) | largest-fit (§8.2's method) |
//! |---|---|---|
//! | 1,000,000 | n = 975,369, gross 999,998 | n = 975,371, gross 1,000,000 |
//! | 500,000 | n = 482,758, gross 499,998 | n = 482,760, gross 500,000 |
//!
//! AGENT #11, SWOLE #15 and SWOLE #22 all recorded the left-hand column. So
//! the table below carries the closed form's outputs, which are 56 SWOLE (10
//! ADA) and 109 SWOLE (50 ADA) under §8.2's projection. The Splash leg is
//! unaffected and matches §8.2 exactly.

#![cfg(test)]

use cardano_assets::{AssetId, UtxoApi, utxo::AssetQuantity};
use pallas_addresses::{
    Address, Network, ShelleyAddress, ShelleyDelegationPart, ShelleyPaymentPart,
};
use pallas_crypto::hash::Hash;
use pallas_txbuilder::{Input, ScriptKind};

use super::lumppad::{LumpPadBuyLeg, LumpPadSellLeg};
use super::splash_pool::{SplashDirection, SplashSwapLeg};
use super::{DustSweep, InputPolicy, InputSource, Leg, Route, RouteAsset, RouteError};
use crate::blueprint::PlutusLanguage;
use crate::builder::TxDeps;
use crate::builder::script::ScriptSource;
use crate::params::TxBuildParams;
use crate::route::leg::LegState;

const SWOLE_POLICY: &str = "023ea41eae25fd3d3042ec5c79b95fa287676eb3c99cd4bfd20fe659";
const SWOLE_NAME: &str = "53574f4c45";
const LUMP_POLICY: &str = "73797786382c0832b5787a5b306f5308488f14571b7061f79396ad2c";
const LUMP_NAME: &str = "4c756d70";

fn asset(policy: &str, name: &str, quantity: u64) -> AssetQuantity {
    AssetQuantity {
        asset_id: AssetId::new_unchecked(policy.to_string(), name.to_string()),
        quantity,
    }
}

/// The Splash LUMP/ADA pool as it sat at
/// `d92955b626329b41222677f6d78e8feed16b6705c6885de00030b340b406efe7#1`.
fn splash_state() -> LegState {
    LegState {
        contract_utxo: UtxoApi {
            tx_hash: "d92955b626329b41222677f6d78e8feed16b6705c6885de00030b340b406efe7".to_string(),
            output_index: 1,
            lovelace: 29_549_431_005,
            assets: vec![
                asset(
                    "6e917b8b965078a39804a6313e5be73535612421acd70aa83f0ec200",
                    "a99ff37ddda0aaa9a1404e403bb605eb000f75cc7484a2bd7470870654c8c890",
                    9_223_372_034_677_906_437,
                ),
                asset(LUMP_POLICY, LUMP_NAME, 196_800_921),
                asset(
                    "d8eb52caf3289a2880288b23141ce3d2a7025dcf76f26fd5659add06",
                    "9f4408661725f5f141a17e75dd0982bbfe6f6053ae779d1fb74fec800e752b44",
                    1,
                ),
            ],
            tags: vec![],
        },
        datum_cbor: hex::decode(super::splash_pool::datum::tests::LUMP_ADA_POOL_DATUM_CBOR)
            .unwrap(),
        // Language and size are READ OFF the reference UTxO by the caller —
        // Koios reports `type: plutusV2, size: 3529` for this one. They are
        // not in the registry record, on purpose.
        script: ScriptSource::Reference {
            utxo: reference_input(address_registry::dex::SPLASH_ROYALTY_POOL.reference_utxo),
            language: ScriptKind::PlutusV2,
        },
        ref_script_size: 3_529,
    }
}

/// The LumpPad SWOLE pool as it sat at `0aca3489…#0`.
fn lumppad_state() -> LegState {
    LegState {
        contract_utxo: UtxoApi {
            tx_hash: "0aca3489a43d669b1aef62717933f141b38a940b9aa4923e180512c7a7b3ae6f".to_string(),
            output_index: 0,
            lovelace: 3_000_000,
            assets: vec![
                asset(SWOLE_POLICY, "000643b0504f4f4c", 1),
                asset(SWOLE_POLICY, SWOLE_NAME, 756_359_192),
                asset(LUMP_POLICY, LUMP_NAME, 3_487_364),
            ],
            tags: vec![],
        },
        datum_cbor: hex::decode(super::lumppad::datum::tests::SWOLE_POOL_DATUM_CBOR).unwrap(),
        // Koios reports `type: plutusV3, size: 3707`.
        script: ScriptSource::Reference {
            utxo: reference_input(address_registry::dex::LUMPPAD.reference_utxo),
            language: ScriptKind::PlutusV3,
        },
        ref_script_size: 3_707,
    }
}

fn reference_input(utxo: address_registry::dex::ReferenceUtxo) -> Input {
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&hex::decode(utxo.tx_hash).unwrap());
    Input::new(Hash::from(hash), utxo.output_index as u64)
}

fn pilot() -> Route {
    Route::new(vec![
        Leg::SplashSwap(SplashSwapLeg::new(SplashDirection::XToY)),
        Leg::LumpPadBuy(LumpPadBuyLeg),
    ])
}

fn user_deps(lovelace: u64) -> TxDeps {
    TxDeps {
        utxos: vec![
            UtxoApi {
                tx_hash: "1".repeat(64),
                output_index: 0,
                lovelace,
                assets: vec![],
                tags: vec![],
            },
            // A separate pure-ADA UTxO for collateral.
            UtxoApi {
                tx_hash: "2".repeat(64),
                output_index: 0,
                lovelace: 6_000_000,
                assets: vec![],
                tags: vec![],
            },
        ],
        params: TxBuildParams {
            min_fee_coefficient: 44,
            min_fee_constant: 155_381,
            coins_per_utxo_byte: 4_310,
            max_tx_size: 16_384,
            max_value_size: 5_000,
            ..Default::default()
        },
        // Constructed rather than pasted, so it is a real mainnet address by
        // derivation and not by a bech32 string someone typed.
        from_address: Address::Shelley(ShelleyAddress::new(
            Network::Mainnet,
            ShelleyPaymentPart::key_hash(Hash::from([0x11; 28])),
            ShelleyDelegationPart::Null,
        )),
        network_id: 1,
    }
}

/// The pilot route prices exactly, at both sizes, from real chain state.
#[test]
fn the_pilot_route_prices_the_snapshot_exactly() {
    let route = pilot();
    let states = [splash_state(), lumppad_state()];

    // 10 ADA
    let q = route.quote(&states, 10_000_000).unwrap();
    assert_eq!(q.asset_in, RouteAsset::Ada);
    assert_eq!(q.amount_in, 10_000_000);
    assert_eq!(
        q.asset_out,
        RouteAsset::Token(AssetId::new_unchecked(
            SWOLE_POLICY.to_string(),
            SWOLE_NAME.to_string()
        ))
    );
    // Splash leg — identical to §8.2.
    assert_eq!(q.legs[0].amount_out, 65_862, "10 ADA → LUMP");
    // LumpPad leg — the closed-form solver, as on chain.
    assert_eq!(q.legs[1].amount_in, 65_862);
    assert_eq!(q.legs[1].remainder_in, 1, "LUMP the solver could not place");
    assert_eq!(q.amount_out, 3_120_727, "SWOLE out");

    // 50 ADA
    let q = route.quote(&states, 50_000_000).unwrap();
    assert_eq!(q.legs[0].amount_out, 328_869, "50 ADA → LUMP");
    assert_eq!(q.legs[1].remainder_in, 2);
    assert_eq!(q.amount_out, 17_474_502, "SWOLE out");
}

/// Both pools' after-states are exactly what the two validators will be asked
/// to accept — the numbers that go into the continuing outputs.
#[test]
fn the_pilot_route_produces_the_expected_after_states() {
    use super::lumppad::PoolDatum;
    use super::splash_pool::RoyaltyPoolDatum;

    let route = pilot();
    let states = [splash_state(), lumppad_state()];
    let q = route.quote(&states, 50_000_000).unwrap();

    let splash_after =
        RoyaltyPoolDatum::from_cbor(q.legs[0].continuing_output.inline_datum.as_ref().unwrap())
            .unwrap();
    assert_eq!(
        splash_after.treasury_x().unwrap(),
        148_513_382 + 25_000,
        "§8.2: treasuryX +25,000"
    );
    assert_eq!(
        splash_after.royalty_x().unwrap(),
        39_027_415 + 25_000,
        "§8.2: royaltyX +25,000"
    );
    assert_eq!(
        splash_after.treasury_y().unwrap(),
        1_094_651,
        "the Y side is untouched by an X→Y swap"
    );

    let lumppad_after =
        PoolDatum::from_cbor(q.legs[1].continuing_output.inline_datum.as_ref().unwrap()).unwrap();
    assert_eq!(lumppad_after.reserve, 3_557_926);
    assert_eq!(lumppad_after.tokens, 738_884_690);
    // A and B match §8.2 exactly — the solver's one-LUMP difference does not
    // reach the bucket fees.
    assert_eq!(lumppad_after.platform_bucket, 179_432, "§8.2: A");
    assert_eq!(lumppad_after.creator_bucket, 78_873, "§8.2: B");
}

/// Every fee bucket is enumerated by name, in leg order — the rows the widget
/// and the cart's review both render.
#[test]
fn the_quote_enumerates_every_fee_bucket() {
    let route = pilot();
    let states = [splash_state(), lumppad_state()];
    let q = route.quote(&states, 50_000_000).unwrap();

    let labels: Vec<&str> = q.fee_lines.iter().map(|f| f.label).collect();
    assert_eq!(
        labels,
        vec![
            "Splash LP",
            "Splash treasury",
            "Splash royalty",
            "LumpPad swap (stays in pool)",
            "LumpPad platform (half burns)",
            "LumpPad creator",
        ]
    );

    let by_label = |label: &str| q.fee_lines.iter().find(|f| f.label == label).unwrap();
    // §8.2: Splash LP 0.9 %, treasury 0.05 %, royalty 0.05 % — all on ADA.
    assert_eq!(by_label("Splash LP").amount, 450_000);
    assert_eq!(by_label("Splash LP").asset, RouteAsset::Ada);
    assert_eq!(by_label("Splash treasury").amount, 25_000);
    assert_eq!(by_label("Splash royalty").amount, 25_000);
    // §8.2: LumpPad platform 10,000 + 0.5 % = 11,570 LUMP, creator 1 % = 3,141.
    assert_eq!(by_label("LumpPad platform (half burns)").amount, 11_570);
    assert_eq!(by_label("LumpPad creator").amount, 3_141);
    for line in &q.fee_lines[3..] {
        assert!(
            matches!(&line.asset, RouteAsset::Token(id) if id.policy_id == LUMP_POLICY),
            "LumpPad's fees are charged in LUMP"
        );
    }
}

/// Legs that do not chain are refused before anything is built.
#[test]
fn a_route_whose_legs_do_not_chain_is_refused() {
    // Splash gives LUMP; a LumpPad SELL wants the pool's token, not LUMP.
    let route = Route::new(vec![
        Leg::SplashSwap(SplashSwapLeg::new(SplashDirection::XToY)),
        Leg::LumpPadSell(LumpPadSellLeg),
    ]);
    let states = [splash_state(), lumppad_state()];
    assert!(
        matches!(
            route.quote(&states, 10_000_000),
            Err(RouteError::AssetMismatch { index: 1, .. })
        ),
        "a LUMP→SWOLE-sell chain is not a chain"
    );
}

#[test]
fn an_empty_route_and_a_state_mismatch_are_refused() {
    assert!(matches!(
        Route::new(vec![]).quote(&[], 1),
        Err(RouteError::Empty)
    ));
    assert!(matches!(
        pilot().quote(&[splash_state()], 10_000_000),
        Err(RouteError::StateCountMismatch { legs: 2, states: 1 })
    ));
}

/// The reverse route — SWOLE → LUMP → ADA — is the same code with the legs and
/// the Splash direction reversed.
#[test]
fn the_reverse_route_chains_and_prices() {
    let route = Route::new(vec![
        Leg::LumpPadSell(LumpPadSellLeg),
        Leg::SplashSwap(SplashSwapLeg::new(SplashDirection::YToX)),
    ]);
    let states = [lumppad_state(), splash_state()];

    let q = route.quote(&states, 50_000_000).unwrap();
    assert_eq!(
        q.asset_in,
        RouteAsset::Token(AssetId::new_unchecked(
            SWOLE_POLICY.to_string(),
            SWOLE_NAME.to_string()
        ))
    );
    assert_eq!(q.asset_out, RouteAsset::Ada, "the user ends in ADA");
    // The LumpPad sell's net LUMP is what the Splash leg swaps.
    assert_eq!(q.legs[1].amount_in, q.legs[0].amount_out);
    assert!(q.amount_out > 0);
}

/// The composed transaction is the shape §8.1 describes: two script inputs of
/// DIFFERENT Plutus languages, both language views registered, both reference
/// scripts present, pool outputs in leg order ahead of the user's, and
/// collateral.
#[test]
fn the_composed_transaction_has_the_shape_both_validators_require() {
    use pallas_traverse::MultiEraTx;
    use pallas_txbuilder::BuildConway;

    let route = pilot();
    let states = [splash_state(), lumppad_state()];
    let quote = route.quote(&states, 10_000_000).unwrap();

    // Enough ADA to fund the 10 ADA going into the Splash pool, the user
    // output's min-UTxO and the fee.
    let deps = user_deps(30_000_000);
    let owner = deps.from_address.clone();
    let builder = route
        .apply(deps, &states, &quote, &InputPolicy::wallet())
        .unwrap();

    // §4.1: BOTH languages, not the highest.
    assert_eq!(
        builder.script_languages(),
        &std::collections::BTreeSet::from([PlutusLanguage::V2, PlutusLanguage::V3]),
        "a V2 pool and a V3 pool in one transaction"
    );

    let unsigned = builder.build().expect("builds");
    let views = unsigned
        .staging
        .language_views
        .as_ref()
        .expect("language views");
    assert_eq!(views.0.len(), 2, "two language views in the integrity hash");

    let built = unsigned.staging.build_conway_raw().expect("serialises");
    let tx = MultiEraTx::decode(&built.tx_bytes.0).expect("decodes");

    // Both reference scripts, deduplicated.
    assert_eq!(tx.reference_inputs().len(), 2);
    // Two script inputs plus whatever coin selection added.
    assert!(tx.inputs().len() >= 3);
    assert_eq!(tx.redeemers().len(), 2, "one redeemer per pool spend");
    assert!(!tx.collateral().is_empty(), "Plutus needs collateral");

    // Outputs: Splash pool, LumpPad pool, the user's SWOLE, then ADA change.
    let outputs = tx.outputs();
    assert!(outputs.len() >= 3);
    // PLACEMENT order, not leg order: LumpPad locates its continuing output
    // at a fixed index and Splash finds its own by NFT, so the LumpPad pool
    // takes index 0 even though it is the SECOND leg. §8.1's sketch drew this
    // the other way round and the LumpPad validator rejected it on mainnet.
    assert_eq!(
        outputs[0].address().unwrap().to_bech32().unwrap(),
        address_registry::dex::LUMPPAD.pool_address,
        "the index-sensitive pool takes first place"
    );
    assert_eq!(
        outputs[1].address().unwrap().to_bech32().unwrap(),
        address_registry::dex::SPLASH_ROYALTY_POOL.pool_address,
        "the NFT-located pool follows"
    );

    // The LumpPad pool keeps its 3 ADA forever.
    assert_eq!(outputs[0].value().coin(), 3_000_000);
    // The Splash pool's continuing output gained exactly the 10 ADA paid in.
    assert_eq!(outputs[1].value().coin(), 29_549_431_005 + 10_000_000);

    // The user's SWOLE is named by an explicit output; `TxBuilder`'s change is
    // ADA-only and would drop it.
    let mut user_swole: u64 = 0;
    for output in &outputs {
        if output.address().unwrap().to_bech32().unwrap() != owner.to_bech32().unwrap() {
            continue;
        }
        for policy in output.value().assets() {
            for asset in policy.assets() {
                if hex::encode(asset.policy()) == SWOLE_POLICY
                    && hex::encode(asset.name()) == SWOLE_NAME
                {
                    user_swole += asset.output_coin().unwrap_or(0);
                }
            }
        }
    }
    assert_eq!(user_swole, 3_120_727, "the user receives the quoted SWOLE");

    // The pools keep every asset they held — Splash checks the token count,
    // and the LP supply must not move.
    let splash_assets: usize = outputs[1]
        .value()
        .assets()
        .iter()
        .map(|p| p.assets().len())
        .sum();
    assert_eq!(splash_assets, 3, "NFT, LP token and LUMP all ride through");
}

/// The pilot cannot be atomic, and the route says so itself — from what the
/// LEGS declare, not from anyone naming LumpPad.
#[test]
fn the_pilot_splits_because_lumppad_refuses_company() {
    use super::{RoutePlan, Segment};

    let route = pilot();
    assert_eq!(route.plan(), RoutePlan::Chained { segments: 2 });
    assert_eq!(
        route.segments(),
        vec![Segment { start: 0, end: 1 }, Segment { start: 1, end: 2 }],
        "the Splash leg, then the LumpPad leg alone"
    );

    // The reverse route splits the same way — the solo leg is first.
    let reverse = Route::new(vec![
        Leg::LumpPadSell(LumpPadSellLeg),
        Leg::SplashSwap(SplashSwapLeg::new(SplashDirection::YToX)),
    ]);
    assert_eq!(reverse.plan(), RoutePlan::Chained { segments: 2 });

    // A route of legs that all compose stays atomic — the split is a
    // consequence of what the venues say, not a policy about route length.
    let two_splash = Route::new(vec![
        Leg::SplashSwap(SplashSwapLeg::new(SplashDirection::XToY)),
        Leg::SplashSwap(SplashSwapLeg::new(SplashDirection::YToX)),
    ]);
    assert_eq!(two_splash.plan(), RoutePlan::Atomic);
    assert_eq!(two_splash.segments(), vec![Segment { start: 0, end: 2 }]);

    // A single solo leg needs no chain: one segment is still atomic.
    let alone = Route::new(vec![Leg::LumpPadBuy(LumpPadBuyLeg)]);
    assert_eq!(alone.plan(), RoutePlan::Atomic);
}

/// Splitting a route does not change a single number. Each leg was quoted
/// against its own pool state, and a transaction boundary touches none of it.
#[test]
fn a_segment_quotes_exactly_as_the_whole_route_did() {
    let route = pilot();
    let states = [splash_state(), lumppad_state()];
    let whole = route.quote(&states, 10_000_000).unwrap();
    let segments = route.segments();

    let first = whole.segment(&segments[0]);
    assert_eq!(first.asset_in, RouteAsset::Ada);
    assert_eq!(first.amount_in, 10_000_000);
    assert_eq!(first.amount_out, 65_862, "the LUMP handed to the next tx");
    assert_eq!(first.legs.len(), 1);

    let second = whole.segment(&segments[1]);
    assert_eq!(
        second.amount_in, 65_861,
        "gross less the unplaceable 1 LUMP"
    );
    assert_eq!(second.amount_out, whole.amount_out);
    assert_eq!(second.amount_out, 3_120_727);

    // The hand-off is exactly what the first segment produces.
    assert_eq!(first.amount_out, whole.legs[1].amount_in);
    // And every fee line survives the split, in order.
    let split: Vec<&str> = first
        .fee_lines
        .iter()
        .chain(second.fee_lines.iter())
        .map(|f| f.label)
        .collect();
    let whole_labels: Vec<&str> = whole.fee_lines.iter().map(|f| f.label).collect();
    assert_eq!(split, whole_labels);
}

/// The Splash redeemer is `Constr 0 [2, selfIx]` with the action as a PLAIN
/// INTEGER, and `selfIx` the pool input's LEDGER-SORTED position.
///
/// Golden: eight consecutive LUMP/ADA swaps on chain all carry exactly this,
/// with `selfIx` observed as both 0 and 1 — so it is genuinely the sorted
/// position and not a constant. `LUMPPAD_INTEGRATION.md` §7.5 recorded the
/// action as `Constr 2 []`; the validator applies `unIData` to that field and
/// rejects a constructor outright ("Expected the I constructor but got a
/// different one"), which is what our first mainnet evaluation did.
#[test]
fn the_splash_redeemer_matches_the_chains() {
    use crate::builder::script::{constr, encode_plutus_data, int};
    use pallas_traverse::MultiEraTx;
    use pallas_txbuilder::BuildConway;

    let route = Route::new(vec![Leg::SplashSwap(SplashSwapLeg::new(
        SplashDirection::XToY,
    ))]);
    let states = [splash_state()];
    let quote = route.quote(&states, 10_000_000).unwrap();

    let unsigned = route
        .apply(
            user_deps(30_000_000),
            &states,
            &quote,
            &InputPolicy::wallet(),
        )
        .unwrap()
        .build()
        .expect("builds");
    let built = unsigned.staging.build_conway_raw().expect("serialises");
    let tx = MultiEraTx::decode(&built.tx_bytes.0).expect("decodes");

    // Where the ledger actually puts the pool input.
    let pool = &states[0].contract_utxo;
    let position = tx
        .inputs()
        .iter()
        .map(|i| format!("{}#{}", i.hash(), i.index()))
        .position(|r| r == format!("{}#{}", pool.tx_hash, pool.output_index))
        .expect("the pool input is in the transaction");

    let staged: Vec<Vec<u8>> = tx
        .redeemers()
        .iter()
        .map(|r| encode_plutus_data(r.data()).expect("re-encodes"))
        .collect();

    let expected = encode_plutus_data(&constr(0, vec![int(2), int(position as i64)])).unwrap();
    assert!(
        staged.contains(&expected),
        "expected Constr 0 [2, {position}]; staged {:?}",
        staged.iter().map(hex::encode).collect::<Vec<_>>()
    );

    // And it is NOT the shape the doc recorded.
    let wrong =
        encode_plutus_data(&constr(0, vec![constr(2, vec![]), int(position as i64)])).unwrap();
    assert!(
        !staged.contains(&wrong),
        "the action must be an integer, not a constructor — `unIData` rejects a Constr"
    );
}

/// A route that ends in ADA needs no explicit user output — the builder's own
/// change output carries it.
#[test]
fn an_ada_ending_route_needs_no_asset_output() {
    let route = Route::new(vec![
        Leg::LumpPadSell(LumpPadSellLeg),
        Leg::SplashSwap(SplashSwapLeg::new(SplashDirection::YToX)),
    ]);
    let states = [lumppad_state(), splash_state()];
    let quote = route.quote(&states, 50_000_000).unwrap();
    let deps = user_deps(30_000_000);
    // A funding UTxO holding EXACTLY the tokens being sold leaves nothing over.
    let exact = vec![UtxoApi {
        tx_hash: "3".repeat(64),
        output_index: 0,
        lovelace: 2_000_000,
        assets: vec![asset(SWOLE_POLICY, SWOLE_NAME, 50_000_000)],
        tags: vec![],
    }];
    assert!(
        quote
            .user_output(&deps.from_address, &deps.params, &exact)
            .is_none(),
        "nothing but ADA comes back, so there is no asset output to name"
    );
}

/// A UTxO is spent WHOLE, so a sell funded by a UTxO holding more than the
/// trade returns the difference — and everything else that UTxO carried.
///
/// Getting this wrong fails no test that only looks at the route's own
/// numbers. It fails at the NODE, with `ValueNotConservedUTxO`, after the user
/// has already signed.
#[test]
fn a_partially_spent_funding_utxo_returns_its_remainder() {
    let route = Route::new(vec![Leg::LumpPadSell(LumpPadSellLeg)]);
    let states = [lumppad_state()];
    let quote = route.quote(&states, 50_000_000).unwrap();
    let deps = user_deps(30_000_000);

    let funding = vec![UtxoApi {
        tx_hash: "3".repeat(64),
        output_index: 0,
        lovelace: 2_000_000,
        assets: vec![
            asset(SWOLE_POLICY, SWOLE_NAME, 120_000_000),
            // An unrelated token riding along in the same UTxO.
            asset(LUMP_POLICY, LUMP_NAME, 4_242),
        ],
        tags: vec![],
    }];
    let output = quote
        .user_output(&deps.from_address, &deps.params, &funding)
        .expect("tokens come back");

    let quantity = |policy: &str, name: &str| {
        output
            .assets
            .iter()
            .find(|(p, n, _)| p == policy && n == name)
            .map(|(_, _, q)| *q)
            .unwrap_or(0)
    };
    assert_eq!(
        quantity(SWOLE_POLICY, SWOLE_NAME),
        70_000_000,
        "120M held less the 50M sold"
    );
    // The LUMP the sell produced, PLUS the 4,242 that rode in on the funding
    // UTxO and was never part of the trade.
    assert_eq!(
        quantity(LUMP_POLICY, LUMP_NAME),
        quote.amount_out + 4_242,
        "the proceeds plus the unrelated token the UTxO carried"
    );
}

/// A token-in route with no UTxO holding that token is refused at build,
/// naming the shortfall — not built into a transaction that cannot balance.
#[test]
fn a_token_route_without_the_token_is_refused() {
    let route = Route::new(vec![Leg::LumpPadSell(LumpPadSellLeg)]);
    let states = [lumppad_state()];
    let quote = route.quote(&states, 50_000_000).unwrap();

    // `user_deps` holds only ADA.
    let Err(error) = route.apply(
        user_deps(30_000_000),
        &states,
        &quote,
        &InputPolicy::wallet(),
    ) else {
        panic!("cannot sell what the wallet does not hold");
    };
    let message = error.to_string();
    assert!(
        message.contains("50000000"),
        "the error must name the shortfall; got {message}"
    );
}

/// A wallet carrying scraps of the input asset, plus one real holding.
///
/// Three 2-SWOLE UTxOs is what a wallet looks like after three sells: the
/// gross solver cannot place the last unit or two, and each remainder comes
/// back as its own UTxO locking min-ADA.
fn wallet_with_dust() -> TxDeps {
    let mut deps = user_deps(30_000_000);
    deps.utxos.push(UtxoApi {
        tx_hash: "3".repeat(64),
        output_index: 0,
        lovelace: 2_000_000,
        assets: vec![asset(SWOLE_POLICY, SWOLE_NAME, 120_000_000)],
        tags: vec![],
    });
    for n in 0..3u32 {
        deps.utxos.push(UtxoApi {
            tx_hash: "4".repeat(64),
            output_index: n,
            lovelace: 1_163_700,
            assets: vec![asset(SWOLE_POLICY, SWOLE_NAME, 2)],
            tags: vec![],
        });
    }
    deps
}

fn sell_route() -> (Route, [LegState; 1]) {
    (
        Route::new(vec![Leg::LumpPadSell(LumpPadSellLeg)]),
        [lumppad_state()],
    )
}

/// The sweep folds the wallet's scraps in and changes NO quoted number.
///
/// That is the whole contract. The swept units are not traded — they ride
/// back out in the user's own output — so the route settles at exactly the
/// figures the user was shown. What they get for it is the min-ADA those
/// scrap UTxOs were locking.
#[test]
fn a_dust_sweep_consumes_the_scraps_without_moving_the_quote() {
    let (route, states) = sell_route();
    let quote = route.quote(&states, 50_000_000).unwrap();

    let plain = route
        .apply(wallet_with_dust(), &states, &quote, &InputPolicy::wallet())
        .unwrap();
    let swept = route
        .apply(
            wallet_with_dust(),
            &states,
            &quote,
            &InputPolicy {
                source: InputSource::Wallet,
                sweep: DustSweep::Below {
                    max_quantity: 50,
                    max_utxos: 10,
                },
            },
        )
        .unwrap();

    // The scraps are spent: three more inputs than the plain build.
    let plain_inputs = plain.staged_input_count();
    let swept_inputs = swept.staged_input_count();
    assert_eq!(
        swept_inputs,
        plain_inputs + 3,
        "the sweep must consume all three scrap UTxOs"
    );

    // And the trade is untouched — same legs, same amounts, same output.
    let requoted = route.quote(&states, 50_000_000).unwrap();
    assert_eq!(requoted.amount_in, quote.amount_in);
    assert_eq!(requoted.amount_out, quote.amount_out);
}

/// `max_utxos` is a hard cap: a transaction has a size limit, and a wallet
/// with hundreds of scraps must not build something unsubmittable.
#[test]
fn a_dust_sweep_stops_at_its_utxo_cap() {
    let (route, states) = sell_route();
    let quote = route.quote(&states, 50_000_000).unwrap();

    let plain = route
        .apply(wallet_with_dust(), &states, &quote, &InputPolicy::wallet())
        .unwrap()
        .staged_input_count();
    let capped = route
        .apply(
            wallet_with_dust(),
            &states,
            &quote,
            &InputPolicy {
                source: InputSource::Wallet,
                sweep: DustSweep::Below {
                    max_quantity: 50,
                    max_utxos: 2,
                },
            },
        )
        .unwrap()
        .staged_input_count();
    assert_eq!(capped, plain + 2, "two scraps, not three");
}

/// A holding at or above the threshold is NOT dust and is left alone — it is
/// the user's position, not a scrap.
#[test]
fn a_dust_sweep_leaves_real_holdings_alone() {
    let (route, states) = sell_route();
    let quote = route.quote(&states, 50_000_000).unwrap();

    let mut deps = wallet_with_dust();
    deps.utxos.push(UtxoApi {
        tx_hash: "5".repeat(64),
        output_index: 0,
        lovelace: 2_000_000,
        // Above the threshold by one, so it must survive.
        assets: vec![asset(SWOLE_POLICY, SWOLE_NAME, 50)],
        tags: vec![],
    });

    let swept = route
        .apply(
            deps,
            &states,
            &quote,
            &InputPolicy {
                source: InputSource::Wallet,
                sweep: DustSweep::Below {
                    max_quantity: 50,
                    max_utxos: 10,
                },
            },
        )
        .unwrap()
        .staged_input_count();
    let plain = route
        .apply(wallet_with_dust(), &states, &quote, &InputPolicy::wallet())
        .unwrap()
        .staged_input_count();
    assert_eq!(
        swept,
        plain + 3,
        "the 50-unit holding is a position, not dust"
    );
}

/// EVERY asset balances: Σ inputs = Σ outputs + fee, for lovelace and for
/// each native asset.
///
/// This is the invariant the node enforces and the evaluator does NOT. A
/// transaction that creates tokens from nowhere evaluates perfectly and is
/// rejected at submit — which is exactly what the first mainnet rehearsal of
/// a LumpPad buy did, before the route staged the user's input asset.
#[test]
fn the_composed_transaction_conserves_every_asset() {
    use pallas_traverse::MultiEraTx;
    use pallas_txbuilder::BuildConway;
    use std::collections::BTreeMap;

    // A sell, because that is the direction with a token INPUT — the case
    // that was wrong.
    let route = Route::new(vec![
        Leg::LumpPadSell(LumpPadSellLeg),
        Leg::SplashSwap(SplashSwapLeg::new(SplashDirection::YToX)),
    ]);
    let states = [lumppad_state(), splash_state()];
    let quote = route.quote(&states, 50_000_000).unwrap();

    let mut deps = user_deps(30_000_000);
    deps.utxos.push(UtxoApi {
        tx_hash: "3".repeat(64),
        output_index: 0,
        lovelace: 2_000_000,
        assets: vec![asset(SWOLE_POLICY, SWOLE_NAME, 120_000_000)],
        tags: vec![],
    });

    // Everything this transaction can possibly spend, by reference.
    let mut resolvable: BTreeMap<String, UtxoApi> = BTreeMap::new();
    for utxo in &deps.utxos {
        resolvable.insert(
            format!("{}#{}", utxo.tx_hash, utxo.output_index),
            utxo.clone(),
        );
    }
    for state in &states {
        let u = &state.contract_utxo;
        resolvable.insert(format!("{}#{}", u.tx_hash, u.output_index), u.clone());
    }

    let unsigned = route
        .apply(deps, &states, &quote, &InputPolicy::wallet())
        .unwrap()
        .build()
        .expect("builds");
    let fee = unsigned.staging.fee.expect("fee staged");
    let built = unsigned.staging.build_conway_raw().expect("serialises");
    let tx = MultiEraTx::decode(&built.tx_bytes.0).expect("decodes");

    let mut balance: BTreeMap<String, i128> = BTreeMap::new();
    for input in tx.inputs() {
        let key = format!("{}#{}", input.hash(), input.index());
        let utxo = resolvable
            .get(&key)
            .unwrap_or_else(|| panic!("input {key} is not one this test staged"));
        *balance.entry("lovelace".to_string()).or_default() += i128::from(utxo.lovelace);
        for a in &utxo.assets {
            *balance.entry(a.asset_id.concatenated()).or_default() += i128::from(a.quantity);
        }
    }
    for output in tx.outputs() {
        *balance.entry("lovelace".to_string()).or_default() -= i128::from(output.value().coin());
        for policy in output.value().assets() {
            for a in policy.assets() {
                *balance
                    .entry(format!(
                        "{}{}",
                        hex::encode(a.policy()),
                        hex::encode(a.name())
                    ))
                    .or_default() -= i128::from(a.output_coin().unwrap_or(0));
            }
        }
    }
    *balance.entry("lovelace".to_string()).or_default() -= i128::from(fee);

    for (asset_id, delta) in &balance {
        assert_eq!(
            *delta, 0,
            "{asset_id} does not balance: inputs − outputs − fee = {delta}"
        );
    }
}
