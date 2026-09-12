//! Print the CBOR of a few datums as hex, for pasting into aiken tests and
//! for seeding the golden corpus.
//!
//! `cargo run -p action-definitions --example dump`

use action_definitions::codec::{Bytes, Cip68Envelope, PlutusCodec};
use action_definitions::types::{
    definition::Accepts, Address, AssetId, Definition, Effect, Filter, FuelBody, Grant, Limits,
    Mode, PaymentKeyHash, PolicyId, RouteRef, Trigger, Window,
};
use pallas_primitives::Fragment;

fn main() {
    let tank = FuelBody {
        balance: 1_000,
        reconciled_slot: 12_345,
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
}

fn dump(name: &str, data: &pallas_primitives::PlutusData) {
    let bytes = data.encode_fragment().expect("encode");
    println!("{name}\t{} bytes\t{}", bytes.len(), hex::encode(&bytes));
}
