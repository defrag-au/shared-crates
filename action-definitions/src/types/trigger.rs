//! What happens on chain — schema §2.8, `Trigger`.
//!
//! The tags mirror `GatewayEvent`'s subjects and the notification kinds the
//! repo already runs: every intake we have is a trigger, and a new intake is
//! a **new tag**, never a reshaped one. Only `Burn` is implemented in the
//! first build; the rest carry their encoding so that a definition written
//! against one of them is inert on today's workers rather than an error.

use serde::{Deserialize, Serialize};

use crate::codec::UnknownFields;
use crate::types::scalars::{Address, PolicyId};
use action_definitions_derive::PlutusCodec;

#[derive(Debug, Clone, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    /// A transfer to an unspendable sink — softburn. The first trigger
    /// built.
    #[plutus(tag = 0)]
    Burn {
        #[plutus(id = 1)]
        sink: Address,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    /// A mint feed is a definition (schema §3.2).
    #[plutus(tag = 1)]
    Mint {
        #[plutus(id = 1)]
        policy: PolicyId,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    #[plutus(tag = 2)]
    Sale {
        #[plutus(id = 1)]
        policy: PolicyId,
        #[plutus(id = 2)]
        venue: Option<Venue>,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    #[plutus(tag = 3)]
    Listed {
        #[plutus(id = 1)]
        policy: PolicyId,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    #[plutus(tag = 4)]
    Unlisted {
        #[plutus(id = 1)]
        policy: PolicyId,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    #[plutus(tag = 5)]
    OfferAccepted {
        #[plutus(id = 1)]
        policy: PolicyId,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    #[plutus(tag = 6)]
    DexTrade {
        #[plutus(id = 1)]
        policy: PolicyId,
        #[plutus(id = 2)]
        min_lovelace: Option<u64>,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    #[plutus(tag = 7)]
    Vesting {
        #[plutus(id = 1)]
        policy: PolicyId,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    #[plutus(tag = 8)]
    Transfer {
        #[plutus(id = 1)]
        policy: PolicyId,
        #[plutus(id = 2)]
        to: Option<Address>,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    /// A trigger kind this build does not know: stored, displayed as
    /// "unsupported trigger N", re-encoded intact, and never acted on.
    #[plutus(unknown)]
    Unknown {
        tag: i64,
        #[serde(skip)]
        fields: UnknownFields,
    },
}

impl Trigger {
    /// A `Burn` trigger on the given sink, with no unknowns — the
    /// constructor the builder and the tests actually want.
    pub fn burn(sink: Address) -> Self {
        Self::Burn {
            sink,
            unknown: UnknownFields::default(),
        }
    }

    pub fn kind(&self) -> TriggerKind {
        match self {
            Self::Burn { .. } => TriggerKind::Burn,
            Self::Mint { .. } => TriggerKind::Mint,
            Self::Sale { .. } => TriggerKind::Sale,
            Self::Listed { .. } => TriggerKind::Listed,
            Self::Unlisted { .. } => TriggerKind::Unlisted,
            Self::OfferAccepted { .. } => TriggerKind::OfferAccepted,
            Self::DexTrade { .. } => TriggerKind::DexTrade,
            Self::Vesting { .. } => TriggerKind::Vesting,
            Self::Transfer { .. } => TriggerKind::Transfer,
            Self::Unknown { .. } => TriggerKind::Unknown,
        }
    }

    /// The policy this trigger watches, where it has one. `Burn` does not —
    /// a sink is an address, and which policies count is the filter's job.
    pub fn policy(&self) -> Option<PolicyId> {
        match self {
            Self::Mint { policy, .. }
            | Self::Sale { policy, .. }
            | Self::Listed { policy, .. }
            | Self::Unlisted { policy, .. }
            | Self::OfferAccepted { policy, .. }
            | Self::DexTrade { policy, .. }
            | Self::Vesting { policy, .. }
            | Self::Transfer { policy, .. } => Some(*policy),
            Self::Burn { .. } | Self::Unknown { .. } => None,
        }
    }
}

/// [`Trigger`] with its payload stripped — for pickers, metrics and match
/// arms that only care which kind it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    Burn,
    Mint,
    Sale,
    Listed,
    Unlisted,
    OfferAccepted,
    DexTrade,
    Vesting,
    Transfer,
    Unknown,
}

impl TriggerKind {
    /// Every kind this build knows, `Unknown` excluded — it is not
    /// something an operator can pick.
    pub const ALL: [TriggerKind; 9] = [
        Self::Burn,
        Self::Mint,
        Self::Sale,
        Self::Listed,
        Self::Unlisted,
        Self::OfferAccepted,
        Self::DexTrade,
        Self::Vesting,
        Self::Transfer,
    ];

    /// Implemented in the first build. The rest encode and decode, but
    /// `validate()` refuses them as `NotYetSupported`.
    pub const fn is_supported(self) -> bool {
        matches!(self, Self::Burn)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Burn => "burn",
            Self::Mint => "mint",
            Self::Sale => "sale",
            Self::Listed => "listed",
            Self::Unlisted => "unlisted",
            Self::OfferAccepted => "offer_accepted",
            Self::DexTrade => "dex_trade",
            Self::Vesting => "vesting",
            Self::Transfer => "transfer",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for TriggerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which marketplace a `Sale` trigger counts. Absent means any.
#[derive(Debug, Clone, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
#[serde(tag = "venue", rename_all = "snake_case")]
pub enum Venue {
    #[plutus(tag = 0)]
    JpgStore,
    #[plutus(tag = 1)]
    Wayup,
    #[plutus(tag = 2)]
    Abandonware,
    #[plutus(unknown)]
    Unknown {
        tag: i64,
        #[serde(skip)]
        fields: UnknownFields,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::PlutusCodec as _;
    use pallas_primitives::Fragment;

    #[test]
    fn every_trigger_kind_round_trips() {
        let policy = PolicyId([1u8; 28]);
        let unknown = UnknownFields::default;
        let triggers = [
            Trigger::burn(Address::from(vec![0x61])),
            Trigger::Mint {
                policy,
                unknown: unknown(),
            },
            Trigger::Sale {
                policy,
                venue: Some(Venue::Wayup),
                unknown: unknown(),
            },
            Trigger::Listed {
                policy,
                unknown: unknown(),
            },
            Trigger::Unlisted {
                policy,
                unknown: unknown(),
            },
            Trigger::OfferAccepted {
                policy,
                unknown: unknown(),
            },
            Trigger::DexTrade {
                policy,
                min_lovelace: Some(1_000_000),
                unknown: unknown(),
            },
            Trigger::Vesting {
                policy,
                unknown: unknown(),
            },
            Trigger::Transfer {
                policy,
                to: None,
                unknown: unknown(),
            },
        ];
        assert_eq!(triggers.len(), TriggerKind::ALL.len());

        for trigger in triggers {
            let data = trigger.to_data();
            let bytes = data.encode_fragment().unwrap();
            let reparsed = pallas_primitives::PlutusData::decode_fragment(&bytes).unwrap();
            assert_eq!(
                Trigger::from_data(&reparsed).unwrap(),
                trigger,
                "{:?} did not survive the ledger's view",
                trigger.kind()
            );
        }
    }

    #[test]
    fn tags_are_the_numbers_the_schema_assigned() {
        // Freezing §2.8's table in a test: a renumbering is a test failure,
        // not a silent compatibility break.
        let expected: [(TriggerKind, i64); 9] = [
            (TriggerKind::Burn, 0),
            (TriggerKind::Mint, 1),
            (TriggerKind::Sale, 2),
            (TriggerKind::Listed, 3),
            (TriggerKind::Unlisted, 4),
            (TriggerKind::OfferAccepted, 5),
            (TriggerKind::DexTrade, 6),
            (TriggerKind::Vesting, 7),
            (TriggerKind::Transfer, 8),
        ];
        let policy = PolicyId::default();
        for (kind, tag) in expected {
            let trigger = match kind {
                TriggerKind::Burn => Trigger::burn(Address::default()),
                TriggerKind::Mint => Trigger::Mint {
                    policy,
                    unknown: UnknownFields::default(),
                },
                TriggerKind::Sale => Trigger::Sale {
                    policy,
                    venue: None,
                    unknown: UnknownFields::default(),
                },
                TriggerKind::Listed => Trigger::Listed {
                    policy,
                    unknown: UnknownFields::default(),
                },
                TriggerKind::Unlisted => Trigger::Unlisted {
                    policy,
                    unknown: UnknownFields::default(),
                },
                TriggerKind::OfferAccepted => Trigger::OfferAccepted {
                    policy,
                    unknown: UnknownFields::default(),
                },
                TriggerKind::DexTrade => Trigger::DexTrade {
                    policy,
                    min_lovelace: None,
                    unknown: UnknownFields::default(),
                },
                TriggerKind::Vesting => Trigger::Vesting {
                    policy,
                    unknown: UnknownFields::default(),
                },
                TriggerKind::Transfer => Trigger::Transfer {
                    policy,
                    to: None,
                    unknown: UnknownFields::default(),
                },
                TriggerKind::Unknown => unreachable!("not in ALL"),
            };
            let data = trigger.to_data();
            let reader = crate::codec::MapReader::new(&data, &[0]).unwrap();
            assert_eq!(reader.tag().unwrap(), tag, "wrong tag for {kind}");
        }
    }

    #[test]
    fn only_burn_is_supported_in_the_first_build() {
        let supported: Vec<_> = TriggerKind::ALL
            .into_iter()
            .filter(|k| k.is_supported())
            .collect();
        assert_eq!(supported, vec![TriggerKind::Burn]);
    }
}
