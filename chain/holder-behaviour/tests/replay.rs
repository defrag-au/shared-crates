//! A replay over four real events, asserted against arithmetic done by hand.
//!
//! The fixture is Perp0001's complete history — a mint and three transfers — copied
//! verbatim from the Perps archive stream (`token-ledger walk --profile profiles/perps.toml
//! --events`). Four events is a **closed world** for one asset: every party a movement
//! touched has its whole history of *this asset* in the file, so each expected value below
//! is derivable from the events alone rather than from whatever the fold happened to
//! produce.
//!
//! Two shapes in it are worth knowing. The last event lists its parties with the recipient
//! first, which is a real archive shape and not the order a `(from, to)` parser would
//! assume. And one party is an **enterprise** address with no stake credential, so it keys
//! by itself while the other three key by theirs — the identity rule in one fixture.

use chain_events::{ChainEvent, Coverage};
use holder_behaviour::{
    Assessment, Definition, EarnedSince, MemoryStore, Predicate, Progress, Slot, Store, evaluate,
    rebuild,
};

const FIXTURE: &str = include_str!("fixtures/perps-0001.ndjson");

/// Alice minted it. Keyed by her stake credential, not the payment address she used.
const ALICE: &str = "63459ca012f21a80698a2ea04179bd68d2d92c50a0a066166839a792";
/// Bob bought it from Alice.
const BOB: &str = "a08985cc73958b7f87d146a2068e73cdf99ac59274f530eebcfcb4ed";
/// Carol bought it from Bob, and sold it on within a block or two. An enterprise address —
/// no stake credential — so she keys by the address itself.
const CAROL: &str = "addr1v825u96vkgrr5lke2c36sgvxa72szq4d50wq3jdk4crylncgd0x55";
/// Dave holds it now.
const DAVE: &str = "9d71cce0d29fb8716a6811c3ab53d8cb9804b13739151d20d783c1a4";

/// The archive's tip when the fixture was taken.
const NOW: Slot = 201_677_944;
/// The same reading on the unix axis. The fixture's `occurred_at` is offset from its slot by
/// 1,591,566,291 seconds, so `NOW_UNIX − NOW` is that offset and every hand-computed tenure
/// below stays exactly what it was.
const NOW_UNIX: u64 = 1_793_244_235;

fn fixture() -> Vec<ChainEvent> {
    FIXTURE
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("the fixture is chain-events JSON"))
        .collect()
}

fn store() -> MemoryStore {
    let mut store = MemoryStore::default();
    let report = rebuild(&mut store, fixture()).unwrap();
    assert_eq!(report.events, 4);
    assert_eq!(report.holder, 4);
    assert!(report.anomalies.is_empty(), "{:?}", report.anomalies);
    store
}

fn subject_of(store: &MemoryStore, address: &str) -> holder_behaviour::SubjectId {
    store
        .subjects()
        .unwrap()
        .into_iter()
        .find(|subject| {
            store
                .account_of(*subject)
                .unwrap()
                .is_some_and(|account| account.address == address)
        })
        .unwrap_or_else(|| panic!("{address} is in the fixture"))
}

fn answers(store: &MemoryStore, address: &str) -> Vec<(String, Progress)> {
    let subject = subject_of(store, address);
    let definitions = vec![
        Definition {
            id: "never-sold",
            predicate: Predicate::NeverDisposed,
        },
        Definition {
            id: "mint-to-now",
            predicate: Predicate::HeldSinceMint,
        },
        Definition {
            id: "tenure-30d",
            predicate: Predicate::TotalTenureAtLeast { days: 30 },
        },
        Definition {
            id: "tenure-365d",
            predicate: Predicate::TotalTenureAtLeast { days: 365 },
        },
        Definition {
            id: "round-trip",
            predicate: Predicate::RoundTripsAtLeast { n: 1 },
        },
    ];
    evaluate(
        store,
        subject,
        &Assessment {
            now: NOW,
            now_unix: NOW_UNIX,
            holder_coverage: Coverage::Complete,
        },
        &definitions,
    )
    .unwrap()
    .into_iter()
    .map(|outcome| (outcome.id.to_string(), outcome.progress))
    .collect()
}

fn progress(answers: &[(String, Progress)], id: &str) -> Progress {
    answers
        .iter()
        .find(|(answer, _)| answer == id)
        .unwrap_or_else(|| panic!("no answer {id}"))
        .1
}

#[test]
fn the_fixture_is_four_holder_events_on_one_asset() {
    let events = fixture();
    assert_eq!(events.len(), 4);
    let kinds: Vec<&str> = events.iter().map(|event| event.body.kind()).collect();
    assert_eq!(kinds, vec!["mint", "transfer", "transfer", "transfer"]);
    assert!(events.iter().all(|event| event.asset.is_some()));
    assert_eq!(events[0].coord.unwrap().height, 183_607_300);
    assert_eq!(events[3].coord.unwrap().height, 185_977_296);
    assert_eq!(events[0].coverage, Coverage::Complete);
}

#[test]
fn every_party_is_a_subject_and_the_three_who_let_go_are_missed() {
    let store = store();
    assert_eq!(store.subjects().unwrap().len(), 4);

    // Alice minted at 183,607,300 and sold at 184,378,425: 771,125 seconds ≈ 8.9 days.
    // Thirty days is out of reach and nothing is left to accrue.
    let alice = answers(&store, ALICE);
    assert_eq!(progress(&alice, "never-sold"), Progress::Missed);
    assert_eq!(
        progress(&alice, "mint-to-now"),
        Progress::Pending {
            have: 0,
            need: 1,
            eta_unix: None
        }
    );
    assert_eq!(
        progress(&alice, "tenure-30d"),
        Progress::Pending {
            have: 771_125,
            need: 2_592_000,
            eta_unix: None
        }
    );
    // The mint lot is not a round trip.
    assert_eq!(
        progress(&alice, "round-trip"),
        Progress::Pending {
            have: 0,
            need: 1,
            eta_unix: None
        }
    );
}

#[test]
fn a_bought_and_sold_lot_is_one_completed_round_trip() {
    let store = store();

    // Bob bought at 184,378,425 and sold at 185,948,259: 1,569,834 seconds ≈ 18.2 days.
    let bob = answers(&store, BOB);
    assert_eq!(progress(&bob, "never-sold"), Progress::Missed);
    assert_eq!(
        progress(&bob, "round-trip"),
        Progress::Earned {
            since: EarnedSince::AsAt {
                slot: NOW,
                unix: NOW_UNIX
            }
        }
    );
    assert_eq!(
        progress(&bob, "tenure-30d"),
        Progress::Pending {
            have: 1_569_834,
            need: 2_592_000,
            eta_unix: None
        }
    );

    // Carol held for 29,037 seconds — two blocks short of nine hours.
    let carol = answers(&store, CAROL);
    assert_eq!(
        progress(&carol, "tenure-30d"),
        Progress::Pending {
            have: 29_037,
            need: 2_592_000,
            eta_unix: None
        }
    );
    assert_eq!(
        progress(&carol, "round-trip"),
        Progress::Earned {
            since: EarnedSince::AsAt {
                slot: NOW,
                unix: NOW_UNIX
            }
        }
    );
}

#[test]
fn the_holder_has_never_sold_and_a_pending_tenure_with_a_date() {
    let store = store();
    let dave = answers(&store, DAVE);

    // Received at 185,977,296 and still held: the interval is open, so "never disposed" is
    // earned at the acquisition itself rather than at this reading.
    assert_eq!(
        progress(&dave, "never-sold"),
        Progress::Earned {
            since: EarnedSince::Exact(185_977_296)
        }
    );
    // Dave bought it; the mint was Alice's.
    assert_eq!(
        progress(&dave, "mint-to-now"),
        Progress::Pending {
            have: 0,
            need: 1,
            eta_unix: None
        }
    );
    // 1,793,244,235 − 1,777,543,587 = 15,700,648 seconds ≈ 181.7 days.
    assert_eq!(
        progress(&dave, "tenure-30d"),
        Progress::Earned {
            since: EarnedSince::AsAt {
                slot: NOW,
                unix: NOW_UNIX
            }
        }
    );
    // A year needs 31,536,000, so 15,835,352 more seconds of one unit: 1,809,079,587 unix.
    assert_eq!(
        progress(&dave, "tenure-365d"),
        Progress::Pending {
            have: 15_700_648,
            need: 31_536_000,
            eta_unix: Some(1_809_079_587)
        }
    );
    // Buying is not a round trip until it is sold.
    assert_eq!(
        progress(&dave, "round-trip"),
        Progress::Pending {
            have: 0,
            need: 1,
            eta_unix: None
        }
    );
}

#[test]
fn a_partial_coverage_turns_every_span_unavailable_for_the_same_fixture() {
    let store = store();
    let dave = subject_of(&store, DAVE);
    let definitions = vec![
        Definition {
            id: "never-sold",
            predicate: Predicate::NeverDisposed,
        },
        Definition {
            id: "tenure-30d",
            predicate: Predicate::TotalTenureAtLeast { days: 30 },
        },
    ];
    let outcomes = evaluate(
        &store,
        dave,
        &Assessment {
            now: NOW,
            now_unix: NOW_UNIX,
            holder_coverage: Coverage::Partial,
        },
        &definitions,
    )
    .unwrap();
    for outcome in &outcomes {
        assert_eq!(
            outcome.progress,
            Progress::Unavailable(holder_behaviour::Gap::HolderCoverage)
        );
    }
}

#[test]
fn a_second_replay_produces_the_same_store() {
    // Determinism is the property the whole design leans on: a rebuildable projection is
    // only rebuildable if two runs agree.
    let first = store();
    let second = store();
    for subject in first.subjects().unwrap() {
        assert_eq!(
            first.lots(subject).unwrap(),
            second.lots(subject).unwrap(),
            "lots differ for {subject:?}"
        );
    }
}
