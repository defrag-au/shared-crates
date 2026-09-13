//! The derive macro, exercised as a consumer sees it.
//!
//! These sample types are not the schema — they are the smallest shapes that
//! cover every codegen path: required, optional, defaulted, list, nested
//! struct, tagged variant, unit variant, and the unknown blocks on both.

use action_definitions::codec::Bytes;
use action_definitions::{MapWriter, PlutusCodec, UnknownFields};
use pallas_primitives::{Fragment, PlutusData};

#[derive(Debug, Clone, PartialEq, Eq, PlutusCodec)]
struct Sample {
    #[plutus(id = 0)]
    required: u64,
    #[plutus(id = 1)]
    maybe: Option<u64>,
    #[plutus(id = 2, default)]
    list: Vec<u64>,
    #[plutus(id = 3, default = 300)]
    depth: u64,
    #[plutus(id = 4)]
    name: String,
    #[plutus(id = 5)]
    hash: [u8; 28],
    #[plutus(id = 6)]
    blob: Bytes,
    #[plutus(id = 7, default)]
    nested: Nested,
    #[plutus(id = 8)]
    choice: Choice,
    #[plutus(unknown)]
    unknown: UnknownFields,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PlutusCodec)]
struct Nested {
    #[plutus(id = 0, default)]
    count: u32,
    #[plutus(unknown)]
    unknown: UnknownFields,
}

#[derive(Debug, Clone, PartialEq, Eq, PlutusCodec)]
enum Choice {
    #[plutus(tag = 0)]
    Plain,
    #[plutus(tag = 1)]
    Carrying {
        // Ids start at 1 — key 0 is the tag.
        #[plutus(id = 1)]
        amount: u64,
        #[plutus(id = 2)]
        label: Option<String>,
        #[plutus(unknown)]
        unknown: UnknownFields,
    },
    #[plutus(unknown)]
    Unknown { tag: i64, fields: UnknownFields },
}

fn sample() -> Sample {
    Sample {
        required: 7,
        maybe: Some(9),
        list: vec![1, 2, 3],
        depth: 300,
        name: "a title".into(),
        hash: [3u8; 28],
        blob: Bytes::from(vec![0xaa, 0xbb]),
        nested: Nested {
            count: 2,
            unknown: UnknownFields::default(),
        },
        choice: Choice::Carrying {
            amount: 5,
            label: None,
            unknown: UnknownFields::default(),
        },
        unknown: UnknownFields::default(),
    }
}

fn ledger_round_trip(data: &PlutusData) -> PlutusData {
    let bytes = data.encode_fragment().expect("encode");
    PlutusData::decode_fragment(&bytes).expect("the ledger must accept what we emit")
}

#[test]
fn a_derived_struct_round_trips_through_real_cbor() {
    let value = sample();
    let data = value.to_data();
    let reparsed = ledger_round_trip(&data);
    assert_eq!(data, reparsed);
    assert_eq!(Sample::from_data(&reparsed).unwrap(), value);
}

#[test]
fn encoding_is_canonical() {
    let a = sample().to_data().encode_fragment().unwrap();
    let b = sample().to_data().encode_fragment().unwrap();
    assert_eq!(a, b, "two encodes of one value must be byte-identical");
}

#[test]
fn defaults_are_absent_on_the_wire_and_come_back() {
    let mut value = sample();
    value.maybe = None;
    value.list = Vec::new();
    value.depth = 300; // the declared default
    value.nested = Nested::default();

    let data = value.to_data();
    let entries = action_definitions::codec::as_map(&data).unwrap();
    let ids: Vec<i128> = entries
        .iter()
        .map(|(k, _)| action_definitions::codec::as_i128(k).unwrap())
        .collect();
    assert_eq!(
        ids,
        vec![0, 4, 5, 6, 8],
        "defaulted fields must not be written"
    );

    assert_eq!(Sample::from_data(&data).unwrap(), value);
}

#[test]
fn a_non_default_value_is_written() {
    let mut value = sample();
    value.depth = 60;
    let data = value.to_data();
    assert_eq!(Sample::from_data(&data).unwrap().depth, 60);
}

#[test]
fn a_missing_required_field_is_an_error_not_a_zero() {
    let mut writer = MapWriter::new();
    writer.field(4, &"only a name".to_string());
    let data = writer.finish();
    assert!(matches!(
        Sample::from_data(&data),
        Err(action_definitions::DecodeError::MissingField { id: 0 })
    ));
}

#[test]
fn an_unknown_field_id_survives_a_decode_encode_cycle_byte_for_byte() {
    // A newer writer adds id 99 to the struct and id 3 to the variant.
    let newer_choice = {
        let mut writer = MapWriter::new();
        writer.tag(1);
        writer.field(1, &5u64);
        writer.field(3, &"from the future".to_string());
        writer.finish()
    };
    let newer = {
        let value = sample();
        let mut writer = MapWriter::new();
        writer.field(0, &value.required);
        writer.opt(1, &value.maybe);
        writer.with_default(2, &value.list, &Vec::new());
        writer.field(4, &value.name);
        writer.field(5, &value.hash);
        writer.field(6, &value.blob);
        writer.with_default(7, &value.nested, &Nested::default());
        writer.field(8, &newer_choice);
        writer.field(99, &vec![7u64, 8]);
        writer.finish()
    };
    let original_bytes = newer.encode_fragment().unwrap();

    let decoded = Sample::from_data(&newer).expect("an old reader must still read it");
    assert_eq!(decoded.unknown.len(), 1, "id 99 should be preserved");
    match &decoded.choice {
        Choice::Carrying {
            amount, unknown, ..
        } => {
            assert_eq!(*amount, 5);
            assert_eq!(unknown.len(), 1, "the variant's id 3 should be preserved");
        }
        other => panic!("expected Carrying, got {other:?}"),
    }

    let re_encoded = decoded.to_data().encode_fragment().unwrap();
    assert_eq!(
        original_bytes, re_encoded,
        "an old reader must not silently delete what it did not understand"
    );
}

#[test]
fn an_unknown_variant_tag_is_inert_not_an_error() {
    let mut writer = MapWriter::new();
    writer.tag(42);
    writer.field(1, &"a trigger kind from next year".to_string());
    let future = writer.finish();
    let original = future.encode_fragment().unwrap();

    let decoded = Choice::from_data(&future).expect("an unknown tag must decode");
    let Choice::Unknown { tag, fields } = &decoded else {
        panic!("expected Unknown, got {decoded:?}");
    };
    assert_eq!(*tag, 42);
    assert_eq!(fields.len(), 1);

    assert_eq!(
        decoded.to_data().encode_fragment().unwrap(),
        original,
        "an unknown variant must re-encode byte-identically"
    );
}

#[test]
fn a_unit_variant_is_just_its_tag() {
    let data = Choice::Plain.to_data();
    let entries = action_definitions::codec::as_map(&data).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(Choice::from_data(&data).unwrap(), Choice::Plain);
}

#[test]
fn a_wrong_shape_is_reported_rather_than_guessed() {
    let not_a_map = 5u64.to_data();
    assert!(matches!(
        Sample::from_data(&not_a_map),
        Err(action_definitions::DecodeError::WrongShape { .. })
    ));
}
