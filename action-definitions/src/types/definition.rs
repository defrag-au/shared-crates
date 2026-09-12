//! The definition itself — schema §2.8.
//!
//! A definition's identity is its **creating tx hash**, so it cannot be
//! edited in place: the owner cancels it and posts a successor naming the
//! old one in `supersedes`. Every version stays on chain, which is what lets
//! the verifier replay a chain of supersessions exactly as written.

use serde::{Deserialize, Serialize};

use action_definitions_derive::PlutusCodec;

use crate::codec::{
    Bytes, DecodeError, Envelope, MapReader, MapWriter, PlutusCodec as PlutusCodecTrait,
    UnknownFields,
};
use crate::types::grant::Grant;
use crate::types::scalars::{AssetId, PaymentKeyHash, PolicyId, ScriptHash, TxHash};
use crate::types::trigger::Trigger;
use pallas_primitives::PlutusData;

/// Body field ids. Assigned once (schema §2.8); removing a field reserves
/// its id rather than freeing it.
pub mod ids {
    pub const TRIGGER: i64 = 0;
    pub const FILTER: i64 = 1;
    pub const WINDOW: i64 = 2;
    pub const GRANTS: i64 = 3;
    pub const LIMITS: i64 = 4;
    pub const TITLE: i64 = 5;
    pub const SUPERSEDES: i64 = 6;
    pub const FUEL: i64 = 7;
    pub const ESCROW: i64 = 8;

    pub const ALL: &[i64] = &[
        TRIGGER, FILTER, WINDOW, GRANTS, LIMITS, TITLE, SUPERSEDES, FUEL, ESCROW,
    ];
}

/// The terms, as posted to the registry.
///
/// `version` and `owner` are the **envelope**'s (schema §2.1) rather than
/// body fields — the registry validator reads `owner` and never looks inside
/// the body at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Definition {
    /// Schema version — semantics only, never shape.
    pub version: u64,
    /// The key that may cancel this definition, and that `Register` checked
    /// signed the posting tx.
    pub owner: PaymentKeyHash,
    pub trigger: Trigger,
    #[serde(default)]
    pub filter: Filter,
    pub window: Window,
    /// At least one. Every grant fires on every qualifying event.
    pub grants: Vec<Grant>,
    #[serde(default)]
    pub limits: Limits,
    /// Short. Marketing copy lives off chain — min-ADA is ~4,310 lovelace
    /// per byte of datum.
    #[serde(default)]
    pub title: String,
    /// A closed predecessor with the same owner, whose confirmed claims
    /// this one inherits.
    #[serde(default)]
    pub supersedes: Option<TxHash>,
    /// The tank this definition draws on. Required — the fuel validator's
    /// `Register` redeemer checks it, so "no definition without a tank" is
    /// a ledger fact rather than a service rule.
    pub fuel: AssetId,
    /// The escrow validator its on-chain grants settle from. Required iff
    /// any grant is `Onchain`.
    #[serde(default)]
    pub escrow: Option<ScriptHash>,
    #[serde(skip)]
    pub unknown: UnknownFields,
}

impl Definition {
    /// The body map, without the envelope.
    fn body_to_data(&self) -> PlutusData {
        let mut writer = MapWriter::new();
        writer.field(ids::TRIGGER, &self.trigger);
        writer.with_default(ids::FILTER, &self.filter, &Filter::default());
        writer.field(ids::WINDOW, &self.window);
        writer.field(ids::GRANTS, &self.grants);
        writer.with_default(ids::LIMITS, &self.limits, &Limits::default());
        writer.with_default(ids::TITLE, &self.title, &String::new());
        writer.opt(ids::SUPERSEDES, &self.supersedes);
        writer.field(ids::FUEL, &self.fuel);
        writer.opt(ids::ESCROW, &self.escrow);
        writer.unknown(&self.unknown);
        writer.finish()
    }

    fn body_from_data(
        body: &PlutusData,
        version: u64,
        owner: PaymentKeyHash,
    ) -> Result<Self, DecodeError> {
        let reader = MapReader::new(body, ids::ALL)?;
        Ok(Self {
            version,
            owner,
            trigger: reader.required(ids::TRIGGER)?,
            filter: reader.or_default(ids::FILTER)?,
            window: reader.required(ids::WINDOW)?,
            grants: reader.required(ids::GRANTS)?,
            limits: reader.or_default(ids::LIMITS)?,
            title: reader.or_default(ids::TITLE)?,
            supersedes: reader.optional(ids::SUPERSEDES)?,
            fuel: reader.required(ids::FUEL)?,
            escrow: reader.optional(ids::ESCROW)?,
            unknown: reader.into_unknown(),
        })
    }
}

/// The envelope is hand-written rather than derived: it is the one
/// positional structure in the format, it is frozen at three fields, and the
/// registry validator's decode depends on it staying exactly that.
impl PlutusCodecTrait for Definition {
    fn to_data(&self) -> PlutusData {
        Envelope {
            version: self.version,
            owner: self.owner.0,
            body: self.body_to_data(),
        }
        .to_data()
    }

    fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
        let envelope = Envelope::from_data(data)?;
        Self::body_from_data(
            &envelope.body,
            envelope.version,
            PaymentKeyHash(envelope.owner),
        )
    }
}

/// To what, and by whom. Every field optional — an empty filter is "no
/// constraint".
#[derive(Debug, Clone, Default, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
pub struct Filter {
    /// Which assets count, for value-bearing triggers.
    #[plutus(id = 0, default)]
    #[serde(default)]
    pub accepts: Vec<Accepts>,
    #[plutus(id = 1, default)]
    #[serde(default)]
    pub traits: Vec<TraitPredicate>,
    #[plutus(id = 2)]
    #[serde(default)]
    pub actors: Option<ActorPredicate>,
    #[plutus(id = 3)]
    #[serde(default)]
    pub min_units: Option<u64>,
    #[plutus(unknown)]
    #[serde(skip)]
    pub unknown: UnknownFields,
}

/// One acceptable asset, and what counts as a unit of it.
#[derive(Debug, Clone, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
pub struct Accepts {
    #[plutus(id = 0)]
    pub policy: PolicyId,
    /// Absent means any asset name under the policy.
    #[plutus(id = 1)]
    #[serde(default)]
    pub name: Option<Bytes>,
    /// How many raw token units make **one** unit for `Mode` and `Limits`
    /// arithmetic.
    ///
    /// Stated here rather than looked up, because decimals are a property of
    /// the token registry and a definition must mean the same thing forever:
    /// $SNEK is 0 dp and $USDC is 8 dp, and a rate that silently changed
    /// when a registry entry did would re-price a live campaign.
    #[plutus(id = 2, default = 1)]
    #[serde(default = "one")]
    pub raw_per_unit: u64,
    #[plutus(unknown)]
    #[serde(skip)]
    pub unknown: UnknownFields,
}

fn one() -> u64 {
    1
}

impl Default for Accepts {
    fn default() -> Self {
        Self {
            policy: PolicyId::default(),
            name: None,
            raw_per_unit: 1,
            unknown: UnknownFields::default(),
        }
    }
}

/// A trait requirement. Values under one name are OR-joined; predicates are
/// AND-joined — the semantics `services/quests` already documents.
#[derive(Debug, Clone, Default, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
pub struct TraitPredicate {
    #[plutus(id = 0)]
    pub name: String,
    #[plutus(id = 1, default)]
    #[serde(default)]
    pub values: Vec<String>,
    #[plutus(unknown)]
    #[serde(skip)]
    pub unknown: UnknownFields,
}

/// Who may take part.
#[derive(Debug, Clone, Default, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
pub struct ActorPredicate {
    #[plutus(id = 0, default)]
    #[serde(default)]
    pub allow_stakes: Vec<Bytes>,
    #[plutus(id = 1, default)]
    #[serde(default)]
    pub deny_stakes: Vec<Bytes>,
    #[plutus(id = 2)]
    #[serde(default)]
    pub min_holding: Option<Accepts>,
    #[plutus(unknown)]
    #[serde(skip)]
    pub unknown: UnknownFields,
}

/// When it counts.
#[derive(Debug, Clone, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
pub struct Window {
    #[plutus(id = 0)]
    #[serde(default)]
    pub opens_slot: Option<u64>,
    /// When it stops accepting events. Required in practice for an
    /// escrow-backed definition: the "expired, give me my prizes back"
    /// withdrawal path reads `closes_slot + settlement_grace`, so without
    /// one the only way back to the prizes is to spend the definition.
    #[plutus(id = 1)]
    #[serde(default)]
    pub closes_slot: Option<u64>,
    /// Slots of depth before anything irreversible happens. Default 300
    /// (~5 min, ~15 blocks); `validate()` floor is 60.
    #[plutus(id = 2, default = 300)]
    #[serde(default = "default_confirm_depth")]
    pub confirm_depth: u64,
    #[plutus(unknown)]
    #[serde(skip)]
    pub unknown: UnknownFields,
}

pub const DEFAULT_CONFIRM_DEPTH: u64 = 300;
pub const MIN_CONFIRM_DEPTH: u64 = 60;

fn default_confirm_depth() -> u64 {
    DEFAULT_CONFIRM_DEPTH
}

impl Default for Window {
    fn default() -> Self {
        Self {
            opens_slot: None,
            closes_slot: None,
            confirm_depth: DEFAULT_CONFIRM_DEPTH,
            unknown: UnknownFields::default(),
        }
    }
}

/// Per-actor caps.
#[derive(Debug, Clone, Default, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
pub struct Limits {
    #[plutus(id = 0)]
    #[serde(default)]
    pub per_actor_max_units: Option<u64>,
    #[plutus(id = 1)]
    #[serde(default)]
    pub total_max_units: Option<u64>,
    #[plutus(id = 2)]
    #[serde(default)]
    pub max_claims_per_actor: Option<u32>,
    #[plutus(unknown)]
    #[serde(skip)]
    pub unknown: UnknownFields,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::grant::{Effect, Mode};
    use crate::types::scalars::{Address, RouteRef};
    use pallas_primitives::Fragment;

    pub fn minimal() -> Definition {
        Definition {
            version: 1,
            owner: PaymentKeyHash([1u8; 28]),
            trigger: Trigger::burn(Address::from(vec![0x71u8; 29])),
            filter: Filter {
                accepts: vec![Accepts {
                    policy: PolicyId([2u8; 28]),
                    name: Some(Bytes::from(b"PERP".to_vec())),
                    raw_per_unit: 1_000_000,
                    unknown: UnknownFields::default(),
                }],
                ..Filter::default()
            },
            window: Window {
                opens_slot: Some(100),
                closes_slot: Some(200),
                confirm_depth: DEFAULT_CONFIRM_DEPTH,
                unknown: UnknownFields::default(),
            },
            grants: vec![Grant::new(
                Mode::guaranteed(1),
                Effect::Notify {
                    route: RouteRef([3u8; 16]),
                    unknown: UnknownFields::default(),
                },
            )],
            limits: Limits::default(),
            title: "Burn $PERP".into(),
            supersedes: None,
            fuel: AssetId::new(PolicyId([4u8; 28]), b"(222)tank"),
            escrow: None,
            unknown: UnknownFields::default(),
        }
    }

    #[test]
    fn a_definition_round_trips_through_the_envelope_and_real_cbor() {
        let def = minimal();
        let data = def.to_data();
        let bytes = data.encode_fragment().unwrap();
        let reparsed = PlutusData::decode_fragment(&bytes).unwrap();
        assert_eq!(data, reparsed);
        assert_eq!(Definition::from_data(&reparsed).unwrap(), def);
    }

    #[test]
    fn the_envelope_exposes_owner_positionally_for_the_validator() {
        let def = minimal();
        let envelope = Envelope::from_data(&def.to_data()).unwrap();
        assert_eq!(envelope.owner, def.owner.0);
        assert_eq!(envelope.version, 1);
        // And the body is a map the validator never has to look inside.
        assert_eq!(crate::codec::shape_of(&envelope.body), "map");
    }

    #[test]
    fn body_ids_are_the_ones_the_schema_assigned() {
        let mut def = minimal();
        def.limits = Limits {
            total_max_units: Some(5),
            ..Limits::default()
        };
        def.supersedes = Some(TxHash([9u8; 32]));
        def.escrow = Some(ScriptHash([8u8; 28]));

        let envelope = Envelope::from_data(&def.to_data()).unwrap();
        let entries = crate::codec::as_map(&envelope.body).unwrap();
        let ids: Vec<i64> = entries
            .iter()
            .map(|(k, _)| crate::codec::as_i128(k).unwrap() as i64)
            .collect();
        assert_eq!(ids, ids::ALL.to_vec());
    }

    #[test]
    fn every_body_id_is_distinct() {
        let mut seen = std::collections::HashSet::new();
        for id in ids::ALL {
            assert!(seen.insert(*id), "duplicate body id {id}");
        }
    }

    #[test]
    fn defaults_keep_a_minimal_definition_small() {
        let def = minimal();
        let envelope = Envelope::from_data(&def.to_data()).unwrap();
        let ids: Vec<i64> = crate::codec::as_map(&envelope.body)
            .unwrap()
            .iter()
            .map(|(k, _)| crate::codec::as_i128(k).unwrap() as i64)
            .collect();
        // limits, supersedes and escrow are all absent.
        assert_eq!(
            ids,
            vec![
                ids::TRIGGER,
                ids::FILTER,
                ids::WINDOW,
                ids::GRANTS,
                ids::TITLE,
                ids::FUEL
            ]
        );
    }

    #[test]
    fn confirm_depth_defaults_to_300_and_is_absent_when_it_is() {
        let def = minimal();
        let data = def.window.to_data();
        let entries = crate::codec::as_map(&data).unwrap();
        assert_eq!(
            entries.len(),
            2,
            "a defaulted confirm_depth must not be written"
        );
        assert_eq!(Window::from_data(&data).unwrap().confirm_depth, 300);
    }

    #[test]
    fn raw_per_unit_defaults_to_one() {
        let accepts = Accepts::default();
        assert_eq!(accepts.raw_per_unit, 1);
        let data = accepts.to_data();
        assert_eq!(Accepts::from_data(&data).unwrap().raw_per_unit, 1);
    }

    #[test]
    fn a_definition_round_trips_through_json_too() {
        let def = minimal();
        let json = serde_json::to_string(&def).unwrap();
        let back: Definition = serde_json::from_str(&json).unwrap();
        assert_eq!(back, def);
    }
}
