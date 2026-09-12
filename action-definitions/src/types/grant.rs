//! What a definition yields — schema §2.8: `Grant`, `Mode`, `Effect`.
//!
//! **Every grant fires on every qualifying event.** One burn can enter a
//! raffle, credit a tank and post an announcement; that is what a list of
//! grants is for, and it is why fulfilments are keyed by `(claim, grant
//! ordinal)` rather than by claim alone.

use serde::{Deserialize, Serialize};

use crate::codec::{Bytes, UnknownFields};
use crate::types::scalars::{PolicyId, RouteRef};
use action_definitions_derive::PlutusCodec;

/// One unit of "what this yields": a mode (how it is awarded) and an effect
/// (what is awarded).
#[derive(Debug, Clone, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
pub struct Grant {
    #[plutus(id = 0)]
    pub mode: Mode,
    #[plutus(id = 1)]
    pub effect: Effect,
    /// Overrides the protocol config's cost-table figure for this grant.
    ///
    /// **May only raise it** (protocol §3a) — a publisher paying more for
    /// priority. `validate()` refuses a value below the table, so a
    /// definition cannot quietly underpay for work.
    #[plutus(id = 2)]
    pub fuel_cost: Option<u64>,
    #[plutus(unknown)]
    #[serde(skip)]
    pub unknown: UnknownFields,
}

impl Grant {
    pub fn new(mode: Mode, effect: Effect) -> Self {
        Self {
            mode,
            effect,
            fuel_cost: None,
            unknown: UnknownFields::default(),
        }
    }
}

/// How a qualifying event is turned into an award.
#[derive(Debug, Clone, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    /// Everyone who qualifies gets `per_unit` per unit. The only mode in the
    /// first build.
    #[plutus(tag = 0)]
    Guaranteed {
        #[plutus(id = 1)]
        per_unit: u64,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    /// Tickets into a draw. **Reserved** — the encoding is frozen now so a
    /// raffle definition written later is readable by today's workers, but
    /// `validate()` refuses it as `NotYetSupported`.
    #[plutus(tag = 1)]
    Raffle {
        #[plutus(id = 1)]
        tickets_per_unit: u64,
        #[plutus(id = 2)]
        draw_slot: u64,
        #[plutus(id = 3)]
        prizes: u32,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    /// Fires once a threshold is reached. **Reserved.**
    #[plutus(tag = 2)]
    Accumulate {
        #[plutus(id = 1)]
        threshold: u32,
        #[plutus(id = 2)]
        per_actor: bool,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    #[plutus(unknown)]
    Unknown {
        tag: i64,
        #[serde(skip)]
        fields: UnknownFields,
    },
}

impl Mode {
    pub fn guaranteed(per_unit: u64) -> Self {
        Self::Guaranteed {
            per_unit,
            unknown: UnknownFields::default(),
        }
    }

    pub fn kind(&self) -> ModeKind {
        match self {
            Self::Guaranteed { .. } => ModeKind::Guaranteed,
            Self::Raffle { .. } => ModeKind::Raffle,
            Self::Accumulate { .. } => ModeKind::Accumulate,
            Self::Unknown { .. } => ModeKind::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModeKind {
    Guaranteed,
    Raffle,
    Accumulate,
    Unknown,
}

impl ModeKind {
    pub const ALL: [ModeKind; 3] = [Self::Guaranteed, Self::Raffle, Self::Accumulate];

    pub const fn is_supported(self) -> bool {
        matches!(self, Self::Guaranteed)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Guaranteed => "guaranteed",
            Self::Raffle => "raffle",
            Self::Accumulate => "accumulate",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for ModeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What is awarded.
///
/// **The tag doubles as the cost-table key** (`ProtocolConfigBody.cost_table`
/// entries are keyed by these numbers), so there is one vocabulary for "what
/// kind of work is this" and a cost entry can never name an effect that does
/// not exist.
#[derive(Debug, Clone, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum Effect {
    /// An outbound HTTP delivery through a registered deliverer.
    #[plutus(tag = 0)]
    Http {
        #[plutus(id = 1)]
        via: Deliverer,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    /// An asset paid from the definition's escrow by a settlement tx. The
    /// only effect the ledger enforces.
    #[plutus(tag = 1)]
    Onchain {
        #[plutus(id = 1)]
        filter: PolicyFilter,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    /// Access to a product for a period or a quantity.
    #[plutus(tag = 2)]
    Entitlement {
        #[plutus(id = 1)]
        product: String,
        #[plutus(id = 2)]
        grant: EntitlementGrant,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    /// An announcement through the gateway. `route` is opaque — a Discord
    /// channel id never appears in a datum.
    #[plutus(tag = 3)]
    Notify {
        #[plutus(id = 1)]
        route: RouteRef,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    /// Something an operator does by hand, recorded against the definition.
    #[plutus(tag = 4)]
    Manual {
        #[plutus(id = 1)]
        label: String,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    /// Burn → fuel. Settled by the burner's own transaction, so there is
    /// nothing for a batcher to do and nothing to deliver.
    #[plutus(tag = 5)]
    Fuel {
        #[plutus(id = 1)]
        credits_per_unit: u64,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    /// Metered agentic work. **Reserved**, no fields yet (protocol §3a).
    #[plutus(tag = 6)]
    Agent {
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    #[plutus(unknown)]
    Unknown {
        tag: i64,
        #[serde(skip)]
        fields: UnknownFields,
    },
}

impl Effect {
    pub fn kind(&self) -> EffectKind {
        match self {
            Self::Http { .. } => EffectKind::Http,
            Self::Onchain { .. } => EffectKind::Onchain,
            Self::Entitlement { .. } => EffectKind::Entitlement,
            Self::Notify { .. } => EffectKind::Notify,
            Self::Manual { .. } => EffectKind::Manual,
            Self::Fuel { .. } => EffectKind::Fuel,
            Self::Agent { .. } => EffectKind::Agent,
            Self::Unknown { .. } => EffectKind::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    Http,
    Onchain,
    Entitlement,
    Notify,
    Manual,
    Fuel,
    Agent,
    Unknown,
}

impl EffectKind {
    pub const ALL: [EffectKind; 7] = [
        Self::Http,
        Self::Onchain,
        Self::Entitlement,
        Self::Notify,
        Self::Manual,
        Self::Fuel,
        Self::Agent,
    ];

    pub const fn is_supported(self) -> bool {
        !matches!(self, Self::Agent | Self::Unknown)
    }

    /// The cost-table key — the same number as the PlutusData tag.
    pub const fn cost_key(self) -> Option<u32> {
        match self {
            Self::Http => Some(0),
            Self::Onchain => Some(1),
            Self::Entitlement => Some(2),
            Self::Notify => Some(3),
            Self::Manual => Some(4),
            Self::Fuel => Some(5),
            Self::Agent => Some(6),
            Self::Unknown => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Onchain => "onchain",
            Self::Entitlement => "entitlement",
            Self::Notify => "notify",
            Self::Manual => "manual",
            Self::Fuel => "fuel",
            Self::Agent => "agent",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for EffectKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where an `Http` effect delivers. A new partner is a new tag and nothing
/// else — one drain, one error type, one receipt shape.
#[derive(Debug, Clone, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
#[serde(tag = "deliverer", rename_all = "snake_case")]
pub enum Deliverer {
    /// An endpoint the owner registered with the worker. **A `RouteRef`,
    /// not a URL**: the URL and its HMAC secret are the worker's, a datum is
    /// public forever.
    #[plutus(tag = 0)]
    Webhook {
        #[plutus(id = 1)]
        endpoint: RouteRef,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    /// **Reserved** — the MarsBirds partner API, deferred.
    #[plutus(tag = 1)]
    Marsbirds {
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    #[plutus(unknown)]
    Unknown {
        tag: i64,
        #[serde(skip)]
        fields: UnknownFields,
    },
}

impl Deliverer {
    pub const fn is_supported(&self) -> bool {
        matches!(self, Self::Webhook { .. })
    }
}

/// Which assets an `Onchain` grant may pay out.
#[derive(Debug, Clone, Default, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
pub struct PolicyFilter {
    #[plutus(id = 0)]
    pub policy: PolicyId,
    /// Empty means any asset name under the policy.
    #[plutus(id = 1, default)]
    pub names: Vec<Bytes>,
    #[plutus(unknown)]
    #[serde(skip)]
    pub unknown: UnknownFields,
}

/// What an entitlement grants.
#[derive(Debug, Clone, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
#[serde(tag = "grant", rename_all = "snake_case")]
pub enum EntitlementGrant {
    /// Days of access.
    #[plutus(tag = 0)]
    Days {
        #[plutus(id = 1)]
        days: u32,
        #[plutus(id = 2, default)]
        stacking: Stacking,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    /// A quantity of something countable — the gateway's announcements, for
    /// one (gateway §6.0).
    #[plutus(tag = 1)]
    Units {
        #[plutus(id = 1)]
        units: u64,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    #[plutus(unknown)]
    Unknown {
        tag: i64,
        #[serde(skip)]
        fields: UnknownFields,
    },
}

/// What a second grant does to an entitlement already in force.
#[derive(Debug, Clone, Default, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
#[serde(tag = "stacking", rename_all = "snake_case")]
pub enum Stacking {
    /// Add to the current expiry. The default, and the only one that never
    /// takes time away from someone who topped up early.
    #[default]
    #[plutus(tag = 0)]
    Extend,
    /// Restart from now.
    #[plutus(tag = 1)]
    Refresh,
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
    use crate::codec::{MapReader, PlutusCodec as _};

    fn tag_of<T: crate::codec::PlutusCodec>(value: &T) -> i64 {
        let data = value.to_data();
        MapReader::new(&data, &[0]).unwrap().tag().unwrap()
    }

    #[test]
    fn effect_tags_match_the_schema_and_the_cost_table_key() {
        let u = UnknownFields::default;
        let effects = [
            Effect::Http {
                via: Deliverer::Webhook {
                    endpoint: RouteRef([1u8; 16]),
                    unknown: u(),
                },
                unknown: u(),
            },
            Effect::Onchain {
                filter: PolicyFilter::default(),
                unknown: u(),
            },
            Effect::Entitlement {
                product: "flow.full".into(),
                grant: EntitlementGrant::Days {
                    days: 30,
                    stacking: Stacking::Extend,
                    unknown: u(),
                },
                unknown: u(),
            },
            Effect::Notify {
                route: RouteRef([2u8; 16]),
                unknown: u(),
            },
            Effect::Manual {
                label: "ship a hoodie".into(),
                unknown: u(),
            },
            Effect::Fuel {
                credits_per_unit: 1,
                unknown: u(),
            },
            Effect::Agent { unknown: u() },
        ];
        assert_eq!(effects.len(), EffectKind::ALL.len());

        for effect in &effects {
            let kind = effect.kind();
            let tag = tag_of(effect);
            assert_eq!(
                kind.cost_key(),
                Some(tag as u32),
                "{kind}'s cost key must equal its wire tag"
            );
            assert_eq!(Effect::from_data(&effect.to_data()).unwrap(), *effect);
        }
    }

    #[test]
    fn mode_tags_are_frozen() {
        assert_eq!(tag_of(&Mode::guaranteed(1)), 0);
        assert_eq!(
            tag_of(&Mode::Raffle {
                tickets_per_unit: 1,
                draw_slot: 2,
                prizes: 3,
                unknown: UnknownFields::default()
            }),
            1
        );
        assert_eq!(
            tag_of(&Mode::Accumulate {
                threshold: 5,
                per_actor: true,
                unknown: UnknownFields::default()
            }),
            2
        );
    }

    #[test]
    fn stacking_defaults_to_extend_and_is_absent_on_the_wire() {
        let grant = EntitlementGrant::Days {
            days: 30,
            stacking: Stacking::default(),
            unknown: UnknownFields::default(),
        };
        assert_eq!(Stacking::default(), Stacking::Extend);
        let data = grant.to_data();
        let entries = crate::codec::as_map(&data).unwrap();
        // tag + days only.
        assert_eq!(entries.len(), 2, "a defaulted stacking must not be written");
        assert_eq!(EntitlementGrant::from_data(&data).unwrap(), grant);
    }

    #[test]
    fn a_reserved_variant_still_round_trips() {
        // The whole point of reserving a tag: the encoding is frozen now,
        // even though validate() refuses it.
        let marsbirds = Deliverer::Marsbirds {
            unknown: UnknownFields::default(),
        };
        assert_eq!(
            Deliverer::from_data(&marsbirds.to_data()).unwrap(),
            marsbirds
        );
        assert!(!marsbirds.is_supported());
        assert_eq!(tag_of(&marsbirds), 1);
    }

    #[test]
    fn a_grant_round_trips_with_and_without_a_fuel_cost() {
        let mut grant = Grant::new(
            Mode::guaranteed(1),
            Effect::Notify {
                route: RouteRef([3u8; 16]),
                unknown: UnknownFields::default(),
            },
        );
        assert_eq!(Grant::from_data(&grant.to_data()).unwrap(), grant);

        grant.fuel_cost = Some(5);
        assert_eq!(Grant::from_data(&grant.to_data()).unwrap(), grant);
    }
}
