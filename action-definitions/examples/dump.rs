//! Print the CBOR of a few datums as hex, for pasting into aiken tests and
//! for seeding the golden corpus.
//!
//! `cargo run -p action-definitions --example dump`

use action_definitions::codec::{Bytes, Cip68Envelope, PlutusCodec};
use action_definitions::types::definition::Accepts;
use action_definitions::types::fuel::{CostEntry, Credential, Currency, ProtocolConfigBody};
use action_definitions::types::grant::EffectKind;
use action_definitions::types::{
    Address, AssetId, Definition, Effect, Filter, FuelBody, Grant, Limits, Mode, PaymentKeyHash,
    PolicyId, RouteRef, Trigger, Window,
};
use pallas_primitives::Fragment;

fn main() {
    let tank = FuelBody {
        balance: 1_000,
        reconciled_at: 12_345,
        reconciled_seq: 7,
        receipts_hash: Bytes::from(vec![0xab; 32]),
        scope: None,
        unknown: Default::default(),
    };
    dump("fuel_body", &tank.to_data());

    let cip68 = Cip68Envelope {
        metadata: pallas_primitives::PlutusData::Map(pallas_codec::utils::KeyValuePairs::Def(
            vec![],
        )),
        version: Cip68Envelope::CIP68_VERSION,
        extra: tank.to_data(),
    };
    dump("cip68_tank", &cip68.to_data());

    let definition = Definition {
        version: 1,
        owner: PaymentKeyHash([1u8; 28]),
        trigger: Trigger::burn(Address::from(vec![0x71u8; 29])),
        filter: Filter {
            accepts: vec![Accepts {
                policy: PolicyId([2u8; 28]),
                name: Some(Bytes::from(b"PERP".to_vec())),
                raw_per_unit: 1_000_000,
                unknown: Default::default(),
            }],
            ..Filter::default()
        },
        window: Window {
            opens_slot: Some(100),
            closes_slot: Some(200),
            confirm_depth: 300,
            unknown: Default::default(),
        },
        grants: vec![Grant::new(
            Mode::guaranteed(1),
            Effect::Notify {
                route: RouteRef([3u8; 16]),
                unknown: Default::default(),
            },
        )],
        limits: Limits::default(),
        title: "Burn $PERP".into(),
        supersedes: None,
        fuel: AssetId::new(PolicyId([4u8; 28]), b"(222)tank"),
        escrow: None,
        unknown: Default::default(),
    };
    dump("definition", &definition.to_data());

    // A fuel definition: burn N of $PERP, get 3N credits. The `TopUp` burn
    // path reads its rate and checks it against the config below.
    let fuel_definition = Definition {
        grants: vec![Grant::new(
            Mode::guaranteed(1),
            Effect::Fuel {
                credits_per_unit: 3,
                unknown: Default::default(),
            },
        )],
        title: "Burn $PERP for fuel".into(),
        ..definition.clone()
    };
    dump("fuel_definition", &fuel_definition.to_data());

    // Byte-identical to the `protocol_config_with_payment_paths` corpus
    // fixture, so `fuel.ak` is tested against the same config the Rust side
    // froze rather than against a hand-written lookalike.
    let config = ProtocolConfigBody {
        currencies: vec![Currency {
            policy: PolicyId([2u8; 28]),
            name: Some(Bytes::from(b"PERP".to_vec())),
            credits_per_unit: 3,
            unknown: Default::default(),
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
        unknown: Default::default(),
    };
    dump("protocol_config_with_payment_paths", &config.to_data());
}

fn dump(name: &str, data: &pallas_primitives::PlutusData) {
    let bytes = data.encode_fragment().expect("encode");
    println!("{name}\t{} bytes\t{}", bytes.len(), hex::encode(&bytes));
}
