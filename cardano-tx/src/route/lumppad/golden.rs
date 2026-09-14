//! The LumpPad golden tape — every validator transaction on chain.
//!
//! Transcribed from `docs/design/LUMPPAD_INTEGRATION.md` Appendix A/B, which
//! was read off mainnet with Koios on 2026-09-14. These rows are the
//! SPECIFICATION: if a quote function stops reproducing one exactly, the
//! function is wrong, because the validator accepted the row.

#![cfg(test)]

use super::datum::PoolDatum;
use super::quote::{claim_split, quote_buy, quote_buy_from_net, quote_sell, solve_net_from_gross};

/// A pool state as the four datum integers the tape records. Credentials and
/// fee schedule are identical across every pool observed, so the tape only
/// varies these.
#[derive(Debug, Clone, Copy)]
struct State {
    r: u64,
    t: u64,
    a: u64,
    b: u64,
}

impl State {
    fn datum(&self) -> PoolDatum {
        PoolDatum {
            token_policy: vec![0xaa; 28],
            token_name: b"TAPE".to_vec(),
            reserve: self.r,
            tokens: self.t,
            platform_bucket: self.a,
            creator_bucket: self.b,
            creator_cred: vec![0xbb; 28],
            treasury_cred: vec![0xcc; 28],
            creator_fee_bps: 100,
            platform_fee_bps: 50,
            flat_fee: 10_000,
        }
    }
}

/// One buy or sell row: the state before, the state after.
struct Row {
    tx: &'static str,
    before: State,
    after: State,
}

const fn row(tx: &'static str, before: (u64, u64, u64, u64), after: (u64, u64, u64, u64)) -> Row {
    Row {
        tx,
        before: State {
            r: before.0,
            t: before.1,
            a: before.2,
            b: before.3,
        },
        after: State {
            r: after.0,
            t: after.1,
            a: after.2,
            b: after.3,
        },
    }
}

/// Appendix A, `kind = buy`. Launch rows with a folded-in first buy are
/// included as buys against a fresh `(0, 1e9, 0, 0)` pool — the doc's note
/// under the table.
const BUYS: &[Row] = &[
    // #2 LPSMOKE
    row(
        "e562aa360b69e2c4",
        (0, 1_000_000_000, 0, 0),
        (20_000, 998_009_969, 10_100, 200),
    ),
    // #4 AGENT launch with first buy
    row(
        "4555c8cabe93e03a",
        (0, 1_000_000_000, 0, 0),
        (500_000, 952_517_027, 12_500, 5_000),
    ),
    // #5 AGENT
    row(
        "ab404225537d4687",
        (500_000, 952_517_027, 12_500, 5_000),
        (1_475_369, 871_778_545, 27_376, 14_753),
    ),
    // #7 AGENT
    row(
        "ee9b9c95ad4910c2",
        (964_531, 912_517_027, 39_930, 19_861),
        (1_468_707, 872_516_959, 52_450, 24_902),
    ),
    // #11 AGENT launch with first buy
    row(
        "c9e8072591633f81",
        (0, 1_000_000_000, 0, 0),
        (975_369, 911_374_150, 14_876, 9_753),
    ),
    // #13 AGENT
    row(
        "0754317f92956e04",
        (975_369, 911_374_150, 0, 0),
        (1_064_038, 904_092_068, 10_443, 886),
    ),
    // #14 SWOLE launch with first buy
    row(
        "00ba10fad1725ccc",
        (0, 1_000_000_000, 0, 0),
        (482_758, 954_079_305, 12_413, 4_827),
    ),
    // #15 SWOLE
    row(
        "5eb0676d41afefed",
        (482_758, 954_079_305, 12_413, 4_827),
        (1_458_127, 873_086_674, 27_289, 14_580),
    ),
    // #16 SWOLE
    row(
        "0b5ea2ee24829222",
        (1_458_127, 873_086_674, 27_289, 14_580),
        (1_645_319, 859_093_753, 38_224, 16_451),
    ),
    // #17 SWOLE
    row(
        "2d033f91758ae950",
        (1_645_319, 859_093_753, 38_224, 16_451),
        (2_128_077, 824_996_100, 50_637, 21_278),
    ),
    // #18 SWOLE
    row(
        "5fa7f92e7968e661",
        (2_128_077, 824_996_100, 50_637, 21_278),
        (2_512_313, 799_735_295, 62_558, 25_120),
    ),
    // #19 AGENT
    row(
        "aee0c2da9531fbe5",
        (1_064_038, 904_092_068, 10_443, 886),
        (1_546_796, 866_401_764, 22_856, 5_713),
    ),
    // #20 AGENT
    row(
        "b7cd9ba26e42eb89",
        (1_546_796, 866_401_764, 22_856, 5_713),
        (1_931_032, 838_580_545, 34_777, 9_555),
    ),
    // #21 SWOLE
    row(
        "07f8c71d7dc5241e",
        (2_512_313, 799_735_295, 62_558, 25_120),
        (3_487_682, 742_063_043, 77_434, 34_873),
    ),
    // #22 SWOLE
    row(
        "0bfb8c3910676880",
        (3_487_682, 742_063_043, 77_434, 34_873),
        (3_970_440, 716_494_867, 89_847, 39_700),
    ),
    // #23 SWOLE
    row(
        "5f61882f6ece8d8e",
        (3_970_440, 716_494_867, 89_847, 39_700),
        (4_453_198, 692_632_328, 102_260, 44_527),
    ),
    // #24 SWOLE
    row(
        "35dda5ea322b80bc",
        (4_453_198, 692_632_328, 102_260, 44_527),
        (4_689_651, 681_516_264, 113_442, 46_891),
    ),
    // #25 SWOLE
    row(
        "42989512b6b2a3db",
        (4_689_651, 681_516_264, 113_442, 46_891),
        (5_172_409, 659_894_699, 125_855, 51_718),
    ),
    // #26 SWOLE
    row(
        "28b815ab677d496c",
        (5_172_409, 659_894_699, 125_855, 51_718),
        (5_408_862, 649_798_385, 137_037, 54_082),
    ),
    // #31 HOSK-X launch with first buy
    row(
        "97f3d212cf98014d",
        (0, 1_000_000_000, 0, 0),
        (10_827_586, 480_882_447, 64_137, 108_275),
    ),
    // #32 HOSK-X
    row(
        "62e65a6dba4b8a52",
        (10_827_586, 480_882_447, 64_137, 108_275),
        (11_802_955, 459_431_544, 79_013, 118_028),
    ),
    // #33 HOSK-X
    row(
        "286ddbec753cb1ee",
        (11_802_955, 459_431_544, 79_013, 118_028),
        (11_921_181, 456_961_124, 89_604, 119_210),
    ),
    // #34 HOSK-X
    row(
        "dc99abed563ec40a",
        (11_921_181, 456_961_124, 89_604, 119_210),
        (12_403_939, 447_143_488, 102_017, 124_037),
    ),
    // #36 HOSK-X
    row(
        "d14dc4ee9bfca4d3",
        (12_403_939, 447_143_488, 102_017, 124_037),
        (14_340_884, 411_659_971, 121_701, 143_406),
    ),
];

/// Appendix A, `kind = sell`.
const SELLS: &[Row] = &[
    // #6 AGENT
    row(
        "24ae2f0c77eec131",
        (1_475_369, 871_778_545, 27_376, 14_753),
        (964_531, 912_517_027, 39_930, 19_861),
    ),
    // #8 AGENT
    row(
        "a43100b86bcda142",
        (1_468_707, 872_516_959, 52_450, 24_902),
        (508_121, 952_517_027, 67_252, 34_507),
    ),
    // #10 AGENT
    row(
        "1927e5e4718677e4",
        (508_121, 952_517_027, 0, 0),
        (10_591, 1_000_000_000, 12_487, 4_975),
    ),
    // #27 SWOLE
    row(
        "5557395cc0dadc96",
        (5_408_862, 649_798_385, 137_037, 54_082),
        (4_178_424, 706_359_192, 153_189, 66_386),
    ),
    // #28 AGENT
    row(
        "ea24b1ab4b84099a",
        (1_931_032, 838_580_545, 34_777, 9_555),
        (1_313_533, 884_488_477, 47_864, 15_729),
    ),
    // #30 SWOLE
    row(
        "0aca3489a43d669b",
        (4_178_424, 706_359_192, 153_189, 66_386),
        (3_243_770, 756_359_192, 167_862, 75_732),
    ),
    // #35 AGENT
    row(
        "4d6ed73783ff4422",
        (1_313_533, 884_488_477, 0, 0),
        (709_920, 934_488_477, 13_018, 6_036),
    ),
];

/// Every buy row reproduces its after-state EXACTLY from the curve amount the
/// chain recorded.
#[test]
fn buy_tape_reproduces_every_after_state() {
    for r in BUYS {
        let before = r.before.datum();
        let net = r.after.r - r.before.r;
        let q = quote_buy_from_net(&before, net)
            .unwrap_or_else(|e| panic!("{}: quote_buy failed: {e}", r.tx));

        assert_eq!(q.after.reserve, r.after.r, "{}: reserve", r.tx);
        assert_eq!(q.after.tokens, r.after.t, "{}: tokens", r.tx);
        assert_eq!(
            q.after.platform_bucket, r.after.a,
            "{}: platform bucket",
            r.tx
        );
        assert_eq!(
            q.after.creator_bucket, r.after.b,
            "{}: creator bucket",
            r.tx
        );
        assert_eq!(
            q.tokens_out,
            r.before.t - r.after.t,
            "{}: tokens to the buyer",
            r.tx
        );
    }
}

/// Every sell row reproduces its after-state EXACTLY from the tokens sold.
#[test]
fn sell_tape_reproduces_every_after_state() {
    for r in SELLS {
        let before = r.before.datum();
        let tokens = r.after.t - r.before.t;
        let q = quote_sell(&before, tokens)
            .unwrap_or_else(|e| panic!("{}: quote_sell failed: {e}", r.tx));

        assert_eq!(q.after.reserve, r.after.r, "{}: reserve", r.tx);
        assert_eq!(q.after.tokens, r.after.t, "{}: tokens", r.tx);
        assert_eq!(
            q.after.platform_bucket, r.after.a,
            "{}: platform bucket",
            r.tx
        );
        assert_eq!(
            q.after.creator_bucket, r.after.b,
            "{}: creator bucket",
            r.tx
        );
        assert_eq!(
            q.gross_out,
            r.before.r - r.after.r,
            "{}: LUMP off the curve",
            r.tx
        );
        // The seller's net is gross less both bucket deltas.
        let delta_a = r.after.a - r.before.a;
        let delta_b = r.after.b - r.before.b;
        assert_eq!(
            q.net_out,
            (r.before.r - r.after.r) - delta_a - delta_b,
            "{}: seller's net",
            r.tx
        );
    }
}

/// Appendix B — every observed claim's three payouts.
#[test]
fn claim_tape_reproduces_every_payout() {
    // (tx, A, B, burn, treasury, creator)
    let rows: &[(&str, u64, u64, u64, u64, u64)] = &[
        ("1789fb5dd78d108e", 10_100, 200, 5_050, 5_050, 200),
        ("47942d670fcc31ce", 67_252, 34_507, 33_626, 33_626, 34_507),
        ("27234b6404bfa70c", 14_876, 9_753, 7_438, 7_438, 9_753),
        ("8da3db37cd7a5758", 47_864, 15_729, 23_932, 23_932, 15_729),
    ];
    for (tx, a, b, burn, treasury, creator) in rows {
        let state = State {
            r: 1,
            t: 1,
            a: *a,
            b: *b,
        }
        .datum();
        let q = claim_split(&state).unwrap_or_else(|e| panic!("{tx}: claim_split failed: {e}"));
        assert_eq!(q.to_burn, *burn, "{tx}: burn payout");
        assert_eq!(q.to_treasury, *treasury, "{tx}: treasury payout");
        assert_eq!(q.to_creator, *creator, "{tx}: creator payout");
        assert_eq!(q.after.platform_bucket, 0, "{tx}: A must be emptied");
        assert_eq!(q.after.creator_bucket, 0, "{tx}: B must be emptied");
        // R and T are untouched by a claim.
        assert_eq!(q.after.reserve, state.reserve, "{tx}: reserve unchanged");
        assert_eq!(q.after.tokens, state.tokens, "{tx}: tokens unchanged");
    }
}

/// The gross solver reproduces what a typed 1,000,000 LUMP actually did on
/// chain: `n = 975,369`, gross 999,998, remainder 2.
///
/// Note the largest `n` that would FIT under 1,000,000 is 975,371. LumpPad
/// does not take it, and neither do we — see `solve_net_from_gross`.
#[test]
fn gross_solver_matches_the_venue_not_the_optimum() {
    let fresh = State {
        r: 0,
        t: 1_000_000_000,
        a: 0,
        b: 0,
    }
    .datum();
    let (n, remainder) = solve_net_from_gross(&fresh, 1_000_000).unwrap();
    assert_eq!(n, 975_369);
    assert_eq!(remainder, 2);

    let q = quote_buy(&fresh, 1_000_000).unwrap();
    assert_eq!(q.net_curve_in, 975_369);
    assert_eq!(q.gross_in, 999_998);
    assert_eq!(q.remainder, 2);
    // …and it reproduces AGENT #11's launch state exactly.
    assert_eq!(q.after.reserve, 975_369);
    assert_eq!(q.after.tokens, 911_374_150);
    assert_eq!(q.after.platform_bucket, 14_876);
    assert_eq!(q.after.creator_bucket, 9_753);
}

/// The remainder a gross buy leaves is always 0–2 LUMP for this fee schedule.
#[test]
fn gross_solver_remainder_stays_tiny() {
    let fresh = State {
        r: 3_243_770,
        t: 756_359_192,
        a: 167_862,
        b: 75_732,
    }
    .datum();
    for gross in [10_002_u64, 20_000, 30_000, 123_457, 999_999, 7_777_777] {
        let (n, remainder) = solve_net_from_gross(&fresh, gross).unwrap();
        assert!(remainder <= 2, "gross {gross}: remainder {remainder} > 2");
        assert!(n > 0, "gross {gross}: solver gave up");
    }
}

/// A gross input at or below the flat fee cannot be priced at all.
#[test]
fn a_buy_under_the_flat_fee_is_refused() {
    let fresh = State {
        r: 0,
        t: 1_000_000_000,
        a: 0,
        b: 0,
    }
    .datum();
    // 10,001 is refused too: one LUMP of budget floors to a curve amount of
    // zero (1 × 10000 / 10150 = 0), so there is nothing to put on the curve.
    // The first priceable gross is 10,002.
    for gross in [0_u64, 1, 9_999, 10_000, 10_001] {
        assert!(
            matches!(
                solve_net_from_gross(&fresh, gross),
                Err(crate::route::leg::RouteError::FlatFeeExceedsInput { .. })
            ),
            "gross {gross} must be refused"
        );
    }
    assert_eq!(
        solve_net_from_gross(&fresh, 10_002).unwrap(),
        (1, 1),
        "the first priceable gross puts 1 LUMP on the curve and returns 1"
    );
}

/// A sell too small to cover its own fees returns `NotSellable` rather than a
/// negative net — what LumpPad's UI shows as "Not sellable at this size".
#[test]
fn a_tiny_sell_is_not_sellable() {
    let state = State {
        r: 3_243_770,
        t: 756_359_192,
        a: 167_862,
        b: 75_732,
    }
    .datum();
    assert!(matches!(
        quote_sell(&state, 1_000),
        Err(crate::route::leg::RouteError::NotSellable)
    ));
    // A big enough sell is fine.
    assert!(quote_sell(&state, 50_000_000).is_ok());
}
