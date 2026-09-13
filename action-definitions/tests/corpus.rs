//! The golden corpus — schema §2.6.
//!
//! **Every encoding shape the crate has ever produced is a fixture here, and
//! a fixture is never edited or deleted.** `cargo test` decodes all of them
//! with the current code; a failure is a compatibility break and blocks the
//! change that caused it.
//!
//! Adding a fixture:
//!
//! ```sh
//! UPDATE_CORPUS=1 nix develop -c cargo test -p action-definitions --test corpus
//! ```
//!
//! That writes **only files that do not exist yet**. It can never overwrite
//! one, because "the bytes for this name changed" is precisely the event the
//! corpus exists to catch — if it could be silenced by re-running the
//! generator, it would be.

use std::path::{Path, PathBuf};

use action_definitions::codec::{Bytes, Cip68Envelope, PlutusCodec, constr_zero, int_data};
use action_definitions::types::definition::Accepts;
use action_definitions::types::fuel::{CostEntry, Credential, Currency};
use action_definitions::types::grant::{
    Deliverer, Effect, EffectKind, EntitlementGrant, Grant, Mode, PolicyFilter, Stacking,
};
use action_definitions::{
    Address, AssetId, ChainAddress, ClaimId, ClaimTag, Definition, Filter, FuelBody, Limits,
    MapWriter, PaymentKeyHash, PolicyId, ProtocolConfigBody, RouteRef, ScriptHash, Trigger, TxHash,
    UnknownFields, Window,
};
use pallas_primitives::{Fragment, PlutusData};

const SCHEMA_VERSION: &str = "v1";

/// A definition's datum sits on a registry UTxO, and min-ADA is charged per
/// byte of it. Current protocol parameter.
const LOVELACE_PER_BYTE: u64 = 4_310;

/// Per-fixture ceiling. Not a protocol limit — a budget, so that "this
/// definition costs 40 ADA to post" is noticed in review rather than on
/// mainnet.
const MAX_FIXTURE_BYTES: usize = 2_048;

struct Case {
    name: &'static str,
    data: PlutusData,
    /// The serde form of the same value, where there is a typed one.
    ///
    /// `None` for hand-built forward-compat fixtures: they carry ids and
    /// tags no Rust type has, so there is nothing to serialise.
    json: Option<String>,
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(SCHEMA_VERSION)
}

// ── the fixtures ───────────────────────────────────────────────────────────

fn unknown() -> UnknownFields {
    UnknownFields::default()
}

fn sink() -> Address {
    Address::from(vec![0x71u8; 29])
}

fn perp() -> Accepts {
    Accepts {
        policy: PolicyId([2u8; 28]),
        name: Some(Bytes::from(b"PERP".to_vec())),
        raw_per_unit: 1_000_000,
        unknown: unknown(),
    }
}

fn tank() -> AssetId {
    AssetId::new(PolicyId([4u8; 28]), b"(222)tank")
}

fn base_definition() -> Definition {
    Definition {
        version: 1,
        owner: PaymentKeyHash([1u8; 28]),
        trigger: Trigger::burn(sink()),
        filter: Filter {
            accepts: vec![perp()],
            ..Filter::default()
        },
        window: Window {
            opens_slot: Some(100_000),
            closes_slot: Some(200_000),
            confirm_depth: 300,
            unknown: unknown(),
        },
        grants: vec![Grant::new(
            Mode::guaranteed(1),
            Effect::Notify {
                route: RouteRef([3u8; 16]),
                unknown: unknown(),
            },
        )],
        limits: Limits::default(),
        title: "Burn $PERP".into(),
        supersedes: None,
        fuel: tank(),
        escrow: None,
        unknown: unknown(),
    }
}

fn typed<T: PlutusCodec + serde::Serialize>(name: &'static str, value: &T) -> Case {
    Case {
        name,
        data: value.to_data(),
        json: Some(serde_json::to_string_pretty(value).expect("serialise")),
    }
}

fn handbuilt(name: &'static str, data: PlutusData) -> Case {
    Case {
        name,
        data,
        json: None,
    }
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    cases.push(typed("burn_one_grant", &base_definition()));

    // Three grants on one burn: a prize, an announcement and access. One
    // burn yields every one of them — that is what a grant list is for.
    let mut three = base_definition();
    three.escrow = Some(ScriptHash([5u8; 28]));
    three.grants = vec![
        Grant::new(
            Mode::guaranteed(1),
            Effect::Onchain {
                filter: PolicyFilter {
                    policy: PolicyId([6u8; 28]),
                    names: vec![],
                    unknown: unknown(),
                },
                unknown: unknown(),
            },
        ),
        Grant::new(
            Mode::guaranteed(1),
            Effect::Notify {
                route: RouteRef([3u8; 16]),
                unknown: unknown(),
            },
        ),
        Grant::new(
            Mode::guaranteed(1),
            Effect::Entitlement {
                product: "flow.full".into(),
                grant: EntitlementGrant::Days {
                    days: 30,
                    stacking: Stacking::Extend,
                    unknown: unknown(),
                },
                unknown: unknown(),
            },
        ),
    ];
    cases.push(typed("burn_three_grants", &three));

    // A mint feed IS a definition (schema §3.2).
    let mut mint_feed = base_definition();
    mint_feed.trigger = Trigger::Mint {
        policy: PolicyId([7u8; 28]),
        unknown: unknown(),
    };
    mint_feed.filter = Filter::default();
    mint_feed.title = "Mint feed".into();
    cases.push(typed("mint_feed", &mint_feed));

    // Burn → fuel, self-settled by the burner's own transaction.
    let mut fuel_def = base_definition();
    fuel_def.grants = vec![Grant::new(
        Mode::guaranteed(1),
        Effect::Fuel {
            credits_per_unit: 3,
            unknown: unknown(),
        },
    )];
    fuel_def.title = "Burn $PERP for fuel".into();
    cases.push(typed("fuel_definition", &fuel_def));

    let mut superseding = base_definition();
    superseding.supersedes = Some(TxHash([8u8; 32]));
    cases.push(typed("supersedes_set", &superseding));

    // >64 bytes, so pallas chunks it into an indefinite bytestring.
    let mut long = base_definition();
    long.title = "A very long campaign title that exceeds the sixty-four byte \
                  definite bytestring limit and must therefore be chunked"
        .into();
    assert!(long.title.len() > 64);
    cases.push(typed("long_title", &long));

    // Reserved shapes. `validate()` refuses both, but the ENCODING is frozen
    // now so a raffle written in a year is readable by today's workers.
    let mut raffle = base_definition();
    raffle.escrow = Some(ScriptHash([5u8; 28]));
    raffle.grants = vec![Grant::new(
        Mode::Raffle {
            tickets_per_unit: 1,
            draw_slot: 250_000,
            prizes: 5,
            unknown: unknown(),
        },
        Effect::Onchain {
            filter: PolicyFilter::default(),
            unknown: unknown(),
        },
    )];
    cases.push(typed("reserved_raffle", &raffle));

    let mut marsbirds = base_definition();
    marsbirds.grants = vec![Grant::new(
        Mode::guaranteed(10),
        Effect::Http {
            via: Deliverer::Marsbirds { unknown: unknown() },
            unknown: unknown(),
        },
    )];
    cases.push(typed("reserved_marsbirds", &marsbirds));

    // The tank, in a real CIP-68 datum.
    let fuel_body = FuelBody {
        balance: 1_000,
        reconciled_at: 12_345,
        reconciled_seq: 7,
        receipts_hash: Bytes::from(vec![0xab; 32]),
        scope: None,
        unknown: unknown(),
    };
    cases.push(typed("fuel_body", &fuel_body));

    let mut metadata = MapWriter::new();
    metadata.field(0, &"Defrag Fuel".to_string());
    cases.push(handbuilt(
        "cip68_tank",
        Cip68Envelope {
            metadata: metadata.finish(),
            version: Cip68Envelope::CIP68_VERSION,
            extra: fuel_body.to_data(),
        }
        .to_data(),
    ));

    cases.push(typed(
        "protocol_config",
        &ProtocolConfigBody {
            currencies: vec![Currency {
                policy: PolicyId([2u8; 28]),
                name: Some(Bytes::from(b"PERP".to_vec())),
                credits_per_unit: 3,
                unknown: unknown(),
            }],
            authorized_spenders: vec![PaymentKeyHash([0xaa; 28])],
            cost_table: vec![
                CostEntry::new(EffectKind::Http, 1).unwrap(),
                CostEntry::new(EffectKind::Onchain, 5).unwrap(),
                CostEntry::new(EffectKind::Entitlement, 1).unwrap(),
                CostEntry::new(EffectKind::Notify, 1).unwrap(),
                CostEntry::new(EffectKind::Manual, 1).unwrap(),
                CostEntry::new(EffectKind::Fuel, 0).unwrap(),
            ],
            cost_table_version: 1,
            max_debit_per_day: 1_000,
            ada_per_credit: 500_000,
            posting_cost: 0,
            // Left at their defaults ON PURPOSE. `authorized_updaters` and
            // `updater_threshold` (ids 7-8), then `fee_credential` and
            // `sinks` (ids 9-10), were each added to the struct after this
            // fixture was frozen, and because all four default they are not
            // written — so THE BYTES ON DISK DO NOT MOVE. That is the whole
            // forward-compatibility claim, demonstrated on a real fixture
            // rather than asserted: adding fields to a struct does not
            // change what an existing datum encodes to. Two separate rounds
            // of additions have now passed this test.
            authorized_updaters: Vec::new(),
            updater_threshold: 0,
            fee_credential: None,
            sinks: Vec::new(),
            unknown: unknown(),
        },
    ));

    // The same config WITH an updater set — a new shape, so a new name
    // rather than an edit to the one above.
    cases.push(typed(
        "protocol_config_with_updaters",
        &ProtocolConfigBody {
            currencies: Vec::new(),
            authorized_spenders: vec![PaymentKeyHash([0xaa; 28])],
            cost_table: vec![CostEntry::new(EffectKind::Notify, 1).unwrap()],
            cost_table_version: 1,
            max_debit_per_day: 1_000,
            ada_per_credit: 500_000,
            posting_cost: 0,
            // 1-of-2: the Ledger plus a backup, so a dead device is an
            // inconvenience rather than a permanently frozen config.
            authorized_updaters: vec![PaymentKeyHash([0xc0; 28]), PaymentKeyHash([0xc1; 28])],
            updater_threshold: 1,
            // Same reasoning as above: ids 9-10 arrived later and default,
            // so this fixture's bytes did not move either.
            fee_credential: None,
            sinks: Vec::new(),
            unknown: unknown(),
        },
    ));

    // A config with BOTH top-up paths open — the shape `fuel.ak` reads when
    // it authenticates a payment. Separate fixture because it is a separate
    // shape, never an edit to a frozen one.
    cases.push(typed(
        "protocol_config_with_payment_paths",
        &ProtocolConfigBody {
            currencies: vec![Currency {
                policy: PolicyId([2u8; 28]),
                name: Some(Bytes::from(b"PERP".to_vec())),
                credits_per_unit: 3,
                unknown: unknown(),
            }],
            authorized_spenders: vec![PaymentKeyHash([0xaa; 28])],
            cost_table: vec![CostEntry::new(EffectKind::Fuel, 0).unwrap()],
            cost_table_version: 1,
            max_debit_per_day: 1_000,
            ada_per_credit: 500_000,
            posting_cost: 0,
            authorized_updaters: vec![PaymentKeyHash([0xc0; 28])],
            updater_threshold: 1,
            fee_credential: Some(Credential::key([0xfe; 28])),
            sinks: vec![Credential::script([0x51; 28])],
            unknown: unknown(),
        },
    ));

    cases.push(typed(
        "claim_tag",
        &ClaimTag {
            definition_tx: TxHash([1u8; 32]),
            claim_id: ClaimId([2u8; 16]),
            recipient: ChainAddress::from_bytes(&{
                let mut raw = vec![0x61u8];
                raw.extend_from_slice(&[9u8; 28]);
                raw
            })
            .unwrap(),
            claim_slot: 123_456_789,
        },
    ));

    // ── forward compatibility, hand-built ──────────────────────────────
    //
    // These are what a NEWER writer produces. They must decode, report
    // their unknowns, and re-encode byte-identically on this build.

    let mut body = MapWriter::new();
    body.field(0, &base_definition().trigger);
    body.field(2, &base_definition().window);
    body.field(3, &base_definition().grants);
    body.field(7, &tank());
    // A field id from a future schema.
    body.field(
        99,
        &Bytes::from(b"a field this build has never heard of".to_vec()),
    );
    cases.push(handbuilt(
        "forward_unknown_field",
        constr_zero(vec![
            int_data(1),
            PaymentKeyHash([1u8; 28]).to_data(),
            body.finish(),
        ]),
    ));

    // A trigger kind from a future schema: tag 42 with its own fields.
    let mut future_trigger = MapWriter::new();
    future_trigger.tag(42);
    future_trigger.field(1, &PolicyId([0xcc; 28]));
    let mut body = MapWriter::new();
    body.field(0, &future_trigger.finish());
    body.field(2, &base_definition().window);
    body.field(3, &base_definition().grants);
    body.field(7, &tank());
    cases.push(handbuilt(
        "forward_unknown_tag",
        constr_zero(vec![
            int_data(1),
            PaymentKeyHash([1u8; 28]).to_data(),
            body.finish(),
        ]),
    ));

    cases
}

// ── the four test families ─────────────────────────────────────────────────

/// Fixtures are written once and then frozen. This never overwrites.
///
/// **Re-freezing is a deliberate act, never an `UPDATE_CORPUS=1`.** Two
/// fixtures have been re-frozen: `claim_tag` and
/// `protocol_config_with_payment_paths`, on 2026-09-13, when a credential
/// stopped being encoded as this format's integer-keyed map and started
/// being encoded as the LEDGER's `Constr 0|1 [hash]` — because `escrow.ak`
/// compares a claim's recipient against a transaction output's address, and
/// the two were not the same shape. Nothing was deployed at the time. The
/// way to do it is to delete the `.cbor` and let this test rewrite it,
/// which makes the change visible as a deletion in review rather than as a
/// silently different byte string.
#[test]
fn corpus_is_present_and_unchanged() {
    let dir = corpus_dir();
    std::fs::create_dir_all(&dir).expect("create corpus dir");
    let updating = std::env::var("UPDATE_CORPUS").is_ok();

    let mut created = Vec::new();
    let mut refreshed: Vec<&str> = Vec::new();
    for case in cases() {
        let cbor_path = dir.join(format!("{}.cbor", case.name));
        let bytes = case.data.encode_fragment().expect("encode");

        if cbor_path.exists() {
            let on_disk = std::fs::read(&cbor_path).expect("read fixture");
            assert_eq!(
                hex::encode(&on_disk),
                hex::encode(&bytes),
                "fixture `{}` changed. A fixture is NEVER edited: either the \
                 encoding regressed, or this is a new shape that needs a new \
                 name.",
                case.name
            );
        } else {
            assert!(
                updating,
                "fixture `{}` is missing. Re-run with UPDATE_CORPUS=1 to add it.",
                case.name
            );
            std::fs::write(&cbor_path, &bytes).expect("write fixture");
            created.push(case.name);
        }

        if let Some(json) = case.json {
            let json_path = dir.join(format!("{}.json", case.name));
            if json_path.exists() {
                let on_disk = std::fs::read_to_string(&json_path).expect("read json");
                // Compared as PARSED values, not as text. The `.cbor` above
                // is the byte-exact guarantee; the sidecar's job is to say
                // what those bytes MEAN, so a serde_json pretty-printer
                // change must not fail the build when nothing semantic
                // moved — especially since a fixture is never edited, which
                // would make a formatting-only failure awkward to resolve.
                let on_disk: serde_json::Value =
                    serde_json::from_str(&on_disk).expect("sidecar is valid json");
                let expected: serde_json::Value =
                    serde_json::from_str(&json).expect("generated json");

                // The `.cbor` is the FROZEN ARTIFACT; the `.json` is a
                // readable projection of it through the CURRENT type, and
                // the two are not frozen alike.
                //
                // When the type gains a defaulted field, the bytes do not
                // move (the encoder omits defaults) but serde does write
                // the field, so the sidecar legitimately gains a line. That
                // is the projection getting MORE accurate, not the fixture
                // changing — and the byte check above has already proven
                // the guarantee is intact by the time we get here.
                //
                // So the sidecar is refreshable under UPDATE_CORPUS while
                // the bytes never are. The git diff is what makes it
                // reviewable.
                if on_disk != expected {
                    assert!(
                        updating,
                        "json sidecar for `{}` no longer matches the type.\n\
                         The CBOR is UNCHANGED — this is the readable \
                         projection drifting, which happens when the type \
                         gains a defaulted field. Re-run with UPDATE_CORPUS=1 \
                         and review the diff; it should show additions at \
                         default values and nothing else.",
                        case.name
                    );
                    std::fs::write(&json_path, format!("{json}\n")).expect("rewrite json");
                    refreshed.push(case.name);
                }
            } else {
                assert!(updating, "json sidecar for `{}` is missing", case.name);
                std::fs::write(&json_path, format!("{json}\n")).expect("write json");
            }
        }
    }

    if !created.is_empty() {
        println!(
            "created {} fixture(s): {}",
            created.len(),
            created.join(", ")
        );
    }
    if !refreshed.is_empty() {
        println!(
            "refreshed {} json sidecar(s) — BYTES UNCHANGED: {}",
            refreshed.len(),
            refreshed.join(", ")
        );
    }
}

/// Round-trip, in both the senses §2.6 asks for.
#[test]
fn every_fixture_round_trips() {
    for case in cases() {
        let bytes = case.data.encode_fragment().expect("encode");

        // decode(encode(decode(bytes))) == decode(bytes)
        let once = PlutusData::decode_fragment(&bytes).expect("decode");
        let twice = PlutusData::decode_fragment(&once.encode_fragment().expect("re-encode"))
            .expect("re-decode");
        assert_eq!(once, twice, "{} did not round-trip", case.name);

        // encode(decode(bytes)) == bytes — the canonical-form claim.
        assert_eq!(
            hex::encode(once.encode_fragment().expect("re-encode")),
            hex::encode(&bytes),
            "{} is not canonical",
            case.name
        );
    }
}

/// Everything we emit re-parses as the ledger sees it. This is what proves
/// the chunking and tag rules, not an assertion about them.
#[test]
fn every_fixture_is_ledger_valid_plutus_data() {
    for case in cases() {
        let bytes = case.data.encode_fragment().expect("encode");
        let reparsed =
            PlutusData::decode_fragment(&bytes).expect("the ledger must accept what we emit");
        assert_eq!(reparsed, case.data, "{} changed shape", case.name);
    }
}

/// The property the whole format exists for: an old reader on a new datum
/// keeps what it could not read.
#[test]
fn forward_compatible_fixtures_decode_report_and_re_encode_identically() {
    for name in ["forward_unknown_field", "forward_unknown_tag"] {
        let case = cases().into_iter().find(|c| c.name == name).expect("case");
        let bytes = case.data.encode_fragment().expect("encode");

        let definition = Definition::from_data(&case.data)
            .unwrap_or_else(|e| panic!("{name} must decode on an older build, got {e}"));

        match name {
            "forward_unknown_field" => assert_eq!(
                definition.unknown.len(),
                1,
                "the future field id should have been preserved"
            ),
            "forward_unknown_tag" => {
                assert!(
                    matches!(definition.trigger, Trigger::Unknown { tag: 42, .. }),
                    "a future trigger kind should be inert, not an error: {:?}",
                    definition.trigger
                );
            }
            _ => unreachable!(),
        }

        assert_eq!(
            hex::encode(definition.to_data().encode_fragment().expect("re-encode")),
            hex::encode(&bytes),
            "{name} must re-encode byte-identically — an old reader that eats \
             new fields is the failure this format exists to prevent"
        );
    }
}

/// Size, and the min-ADA it implies. Prints the table the hand-over quotes.
#[test]
fn every_fixture_is_affordable() {
    println!("\n{:<26} {:>7}  {:>12}", "fixture", "bytes", "min-ADA");
    for case in cases() {
        let bytes = case.data.encode_fragment().expect("encode");
        let lovelace = bytes.len() as u64 * LOVELACE_PER_BYTE;
        println!(
            "{:<26} {:>7}  {:>9}.{:02} ₳",
            case.name,
            bytes.len(),
            lovelace / 1_000_000,
            (lovelace % 1_000_000) / 10_000
        );
        assert!(
            bytes.len() <= MAX_FIXTURE_BYTES,
            "{} is {} bytes, over the {MAX_FIXTURE_BYTES}-byte budget",
            case.name,
            bytes.len()
        );
    }
}

/// The `.json` sidecar is the same value, so a human can read a fixture.
///
/// Only asserted for fixtures with no unknown fields: `UnknownFields` is
/// `#[serde(skip)]` by design (§2.7), so a JSON round-trip of a datum from a
/// newer writer cannot reproduce the parts this build did not understand.
#[test]
fn json_sidecars_decode_to_the_same_definition() {
    for name in [
        "burn_one_grant",
        "burn_three_grants",
        "mint_feed",
        "fuel_definition",
        "supersedes_set",
        "long_title",
        "reserved_raffle",
        "reserved_marsbirds",
    ] {
        let case = cases().into_iter().find(|c| c.name == name).expect("case");
        let from_cbor = Definition::from_data(&case.data).expect("decode cbor");
        let from_json: Definition =
            serde_json::from_str(&case.json.expect("typed fixture")).expect("decode json");
        assert_eq!(from_cbor, from_json, "{name}: the two codecs disagree");
    }
}
