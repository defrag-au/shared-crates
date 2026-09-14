//! The Splash royalty-pool golden tape.
//!
//! Fourteen consecutive swaps on the **LUMP/ADA pool itself** — the pool the
//! pilot route's ADA leg spends — in both directions, read off mainnet with
//! Koios on 2026-09-14 (pool NFT
//! `d8eb52ca….9f4408661725f5f141a17e75dd0982bbfe6f6053ae779d1fb74fec800e752b44`).
//! Every one settles at the formula's MAXIMUM exactly, and every treasury and
//! royalty counter moves by exactly `amount_in * rate / feeDen`.
//!
//! `COMPOSED_ROUTES.md` §4.4 nominated a different transaction
//! (`0f2c9623…`, an Aliens/ADA swap) as the golden and asserted it "matches
//! exactly". It does not, and cannot: the validator's rule is an inequality,
//! that transaction's builder took two units under the maximum, and a formula
//! that reproduced 1,696,643 would have to be wrong about the maximum. It is
//! kept below as a separate test of the INEQUALITY, which is what it actually
//! demonstrates.

#![cfg(test)]

use super::datum::RoyaltyPoolDatum;
use super::quote::{PoolReserves, SplashDirection, quote_swap};
use crate::builder::script::{bytes, constr_indef, int, list};
use pallas_primitives::conway::PlutusData;

/// One observed swap.
struct Row {
    tx: &'static str,
    direction: SplashDirection,
    /// Pool UTxO holdings before.
    x: u64,
    y: u64,
    /// Datum counters before.
    treasury_x: u64,
    treasury_y: u64,
    royalty_x: u64,
    royalty_y: u64,
    amount_in: u64,
    amount_out: u64,
    /// Datum counters after.
    after_treasury_x: u64,
    after_treasury_y: u64,
    after_royalty_x: u64,
    after_royalty_y: u64,
}

#[allow(clippy::too_many_arguments)]
const fn row(
    tx: &'static str,
    direction: SplashDirection,
    x: u64,
    y: u64,
    treasury_x: u64,
    treasury_y: u64,
    royalty_x: u64,
    royalty_y: u64,
    amount_in: u64,
    amount_out: u64,
    after_treasury_x: u64,
    after_treasury_y: u64,
    after_royalty_x: u64,
    after_royalty_y: u64,
) -> Row {
    Row {
        tx,
        direction,
        x,
        y,
        treasury_x,
        treasury_y,
        royalty_x,
        royalty_y,
        amount_in,
        amount_out,
        after_treasury_x,
        after_treasury_y,
        after_royalty_x,
        after_royalty_y,
    }
}

const TAPE: &[Row] = &[
    row(
        "0e004c1f628a2c32",
        SplashDirection::XToY,
        39055569413,
        131649352,
        56225517,
        423862,
        56225517,
        423862,
        68310000,
        226750,
        56259672,
        423862,
        56259672,
        423862,
    ),
    row(
        "4895eb0912ab77e5",
        SplashDirection::YToX,
        39780011291,
        129284057,
        56587737,
        423862,
        56587737,
        423862,
        138545,
        42315762,
        56587737,
        423931,
        56587737,
        423931,
    ),
    row(
        "4a9fbe7505a2fb35",
        SplashDirection::YToX,
        41069539229,
        125282429,
        57253658,
        423931,
        57253658,
        423931,
        2037822,
        653405777,
        57253658,
        424949,
        57253658,
        424949,
    ),
    row(
        "7b51ae3bfdf1cf5a",
        SplashDirection::XToY,
        40469916029,
        127112830,
        56953847,
        423931,
        56953847,
        423931,
        599623200,
        1830401,
        57253658,
        423931,
        57253658,
        423931,
    ),
    row(
        "7d917d5f02d18b06",
        SplashDirection::XToY,
        39123879413,
        131422602,
        56259672,
        423862,
        56259672,
        423862,
        606631878,
        1979675,
        56562987,
        423862,
        56562987,
        423862,
    ),
    row(
        "891aa7f7166ff02a",
        SplashDirection::XToY,
        40044216029,
        128445448,
        56740997,
        423931,
        56740997,
        423931,
        297000000,
        932697,
        56889497,
        423931,
        56889497,
        423931,
    ),
    row(
        "907cf784c8976163",
        SplashDirection::XToY,
        39907596029,
        128879125,
        56672687,
        423931,
        56672687,
        423931,
        68310000,
        217208,
        56706842,
        423931,
        56706842,
        423931,
    ),
    row(
        "992c5c61ad12d562",
        SplashDirection::XToY,
        39737695529,
        129422602,
        56587737,
        423931,
        56587737,
        423931,
        169900500,
        543477,
        56672687,
        423931,
        56672687,
        423931,
    ),
    row(
        "9c8a4e16c9061db1",
        SplashDirection::XToY,
        40341216029,
        127512751,
        56889497,
        423931,
        56889497,
        423931,
        128700000,
        399921,
        56953847,
        423931,
        56953847,
        423931,
    ),
    row(
        "9fb329defc03ef73",
        SplashDirection::XToY,
        40416133452,
        127320251,
        57253658,
        424949,
        57253658,
        424949,
        346500000,
        1067391,
        57426908,
        424949,
        57426908,
        424949,
    ),
    row(
        "ab0e7e0f7ed60b5b",
        SplashDirection::YToX,
        40762633452,
        126252860,
        57426908,
        424949,
        57426908,
        424949,
        687687,
        219484392,
        57426908,
        425292,
        57426908,
        425292,
    ),
    row(
        "d59200d56cae7c68",
        SplashDirection::XToY,
        39730511291,
        129442927,
        56562987,
        423862,
        56562987,
        423862,
        49500000,
        158870,
        56587737,
        423862,
        56587737,
        423862,
    ),
    row(
        "e602eb6e46c563d3",
        SplashDirection::YToX,
        39156396778,
        131308165,
        56225517,
        423692,
        56225517,
        423692,
        341187,
        100827365,
        56225517,
        423862,
        56225517,
        423862,
    ),
    row(
        "e8513d045c523216",
        SplashDirection::XToY,
        39975906029,
        128661917,
        56706842,
        423931,
        56706842,
        423931,
        68310000,
        216469,
        56740997,
        423931,
        56740997,
        423931,
    ),
];

/// A LUMP/ADA pool datum with the given counters. Every other field is the
/// pool's real one — the values are lifted from the live datum so the fee
/// schedule under test is the pool's own.
fn datum(treasury_x: u64, treasury_y: u64, royalty_x: u64, royalty_y: u64) -> RoyaltyPoolDatum {
    let asset = |policy: &str, name: &str| {
        constr_indef(
            0,
            vec![
                bytes(hex::decode(policy).unwrap()),
                bytes(hex::decode(name).unwrap()),
            ],
        )
    };
    let fields: Vec<PlutusData> = vec![
        asset(
            "d8eb52caf3289a2880288b23141ce3d2a7025dcf76f26fd5659add06",
            "9f4408661725f5f141a17e75dd0982bbfe6f6053ae779d1fb74fec800e752b44",
        ),
        asset("", ""),
        asset(
            "73797786382c0832b5787a5b306f5308488f14571b7061f79396ad2c",
            "4c756d70",
        ),
        asset(
            "6e917b8b965078a39804a6313e5be73535612421acd70aa83f0ec200",
            "a99ff37ddda0aaa9a1404e403bb605eb000f75cc7484a2bd7470870654c8c890",
        ),
        int(99_100),
        int(50),
        int(50),
        int(treasury_x as i64),
        int(treasury_y as i64),
        int(royalty_x as i64),
        int(royalty_y as i64),
        list(vec![]),
        bytes(hex::decode("75c4570eb625ae881b32a34c52b159f6f3f3f2c7aaabf5bac4688133").unwrap()),
        bytes(
            hex::decode("72c68f905716a5f59a0ee2552ab68559f42287d335396d8f430da98e96c5009c")
                .unwrap(),
        ),
        int(1),
    ];
    RoyaltyPoolDatum::from_data(&constr_indef(0, fields)).expect("well-formed")
}

/// Every observed swap settles at the formula's maximum EXACTLY, in both
/// directions, and moves exactly the counters the chain recorded.
#[test]
fn splash_tape_reproduces_every_swap() {
    for r in TAPE {
        let before = datum(r.treasury_x, r.treasury_y, r.royalty_x, r.royalty_y);
        let q = quote_swap(
            &before,
            PoolReserves { x: r.x, y: r.y },
            r.direction,
            r.amount_in,
        )
        .unwrap_or_else(|e| panic!("{}: quote_swap failed: {e}", r.tx));

        assert_eq!(q.amount_out, r.amount_out, "{}: output", r.tx);
        assert_eq!(
            q.after.treasury_x().unwrap(),
            r.after_treasury_x,
            "{}: treasuryX",
            r.tx
        );
        assert_eq!(
            q.after.treasury_y().unwrap(),
            r.after_treasury_y,
            "{}: treasuryY",
            r.tx
        );
        assert_eq!(
            q.after.royalty_x().unwrap(),
            r.after_royalty_x,
            "{}: royaltyX",
            r.tx
        );
        assert_eq!(
            q.after.royalty_y().unwrap(),
            r.after_royalty_y,
            "{}: royaltyY",
            r.tx
        );
    }
}

/// The counters on the side the input did NOT arrive on are byte-identical.
#[test]
fn only_the_input_sides_counters_move() {
    for r in TAPE {
        match r.direction {
            SplashDirection::XToY => {
                assert_eq!(r.treasury_y, r.after_treasury_y, "{}: Y treasury", r.tx);
                assert_eq!(r.royalty_y, r.after_royalty_y, "{}: Y royalty", r.tx);
            }
            SplashDirection::YToX => {
                assert_eq!(r.treasury_x, r.after_treasury_x, "{}: X treasury", r.tx);
                assert_eq!(r.royalty_x, r.after_royalty_x, "{}: X royalty", r.tx);
            }
        }
    }
}

/// `0f2c96233a322ec4c103fa898a51587002c4310d0ee4179682ce00f283647b21` — the
/// direct Aliens/ADA swap `COMPOSED_ROUTES.md` §4.4 nominates.
///
/// It demonstrates the INEQUALITY, not the maximum: 51,000,000 lovelace
/// against the recorded before-state has a maximum of **1,696,645** Aliens,
/// and that transaction's builder took 1,696,643 — two under. The validator
/// accepted it because its rule is `<=`. Our builder takes the maximum.
#[test]
fn the_doc_s_sample_tx_took_two_under_the_maximum() {
    let before = {
        let asset = |policy: &str, name: &str| {
            constr_indef(
                0,
                vec![
                    bytes(hex::decode(policy).unwrap()),
                    bytes(hex::decode(name).unwrap()),
                ],
            )
        };
        let fields: Vec<PlutusData> = vec![
            asset(
                "d8eb52caf3289a2880288b23141ce3d2a7025dcf76f26fd5659add06",
                "835c345cfdee0f40c5661636b46ca944d3e267b8a41129af6e7d6d4497eb20a7",
            ),
            asset("", ""),
            asset(
                "16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb7",
                "416c69656e73",
            ),
            asset(
                "6e917b8b965078a39804a6313e5be73535612421acd70aa83f0ec200",
                "f965e16a7ffc22ed72f1dc5bf2945fdfb7889c974863dbabf103dda22e2667ee",
            ),
            int(99_100),
            int(50),
            int(50),
            int(5_114_222),
            int(163_304),
            int(3_426_289),
            int(123_618),
            list(vec![]),
            bytes(hex::decode("e67b02322e98f8f622980042a69508d67e2750afc92f4f8956188573").unwrap()),
            bytes(
                hex::decode("72c68f905716a5f59a0ee2552ab68559f42287d335396d8f430da98e96c5009c")
                    .unwrap(),
            ),
            int(4),
        ];
        RoyaltyPoolDatum::from_data(&constr_indef(0, fields)).unwrap()
    };

    let q = quote_swap(
        &before,
        PoolReserves {
            x: 12_940_798_457,
            y: 436_554_009,
        },
        SplashDirection::XToY,
        51_000_000,
    )
    .unwrap();

    const OBSERVED: u64 = 1_696_643;
    assert_eq!(q.amount_out, 1_696_645, "the validator's maximum");
    assert!(
        OBSERVED < q.amount_out,
        "the recorded transaction took less than the maximum, which the \
         inequality permits"
    );
    // The counters the transaction did move are exactly what we compute.
    assert_eq!(q.treasury_fee, 25_500);
    assert_eq!(q.royalty_fee, 25_500);
    assert_eq!(q.after.treasury_x().unwrap(), 5_139_722);
    assert_eq!(q.after.royalty_x().unwrap(), 3_451_789);
    assert_eq!(q.after.treasury_y().unwrap(), 163_304, "Y side untouched");
    assert_eq!(q.after.royalty_y().unwrap(), 123_618, "Y side untouched");
}

/// The §8.2 snapshot: 10 ADA and 50 ADA into the live LUMP/ADA pool.
#[test]
fn the_pilot_snapshot_prices_as_the_plan_says() {
    let before = RoyaltyPoolDatum::from_cbor(
        &hex::decode(super::datum::tests::LUMP_ADA_POOL_DATUM_CBOR).unwrap(),
    )
    .unwrap();
    let reserves = PoolReserves {
        x: 29_549_431_005,
        y: 196_800_921,
    };

    let ten = quote_swap(&before, reserves, SplashDirection::XToY, 10_000_000).unwrap();
    assert_eq!(ten.amount_out, 65_862, "10 ADA → LUMP");
    assert_eq!(ten.treasury_fee, 5_000);
    assert_eq!(ten.royalty_fee, 5_000);

    let fifty = quote_swap(&before, reserves, SplashDirection::XToY, 50_000_000).unwrap();
    assert_eq!(fifty.amount_out, 328_869, "50 ADA → LUMP");
    assert_eq!(fifty.treasury_fee, 25_000);
    assert_eq!(fifty.royalty_fee, 25_000);
}
