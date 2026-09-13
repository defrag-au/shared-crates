//! Is this definition well-formed? — schema knowledge, so it lives here.
//!
//! Everything it needs arrives as an **argument**, so the crate never
//! depends on `address-registry` or on a worker and stays usable from a
//! macroquad app that only renders a card.
//!
//! **Every reason is returned, not the first.** An owner filling in the
//! admin form should see the whole list rather than play whack-a-mole with
//! one error per submission.
//!
//! What is deliberately *not* here: anything needing chain state. Whether
//! `supersedes` names a closed definition with the same owner, whether the
//! tank has fuel, whether escrow is funded — those are [`crate::recognise`]
//! and the service's job. `validate` is about the datum alone.

use serde::{Deserialize, Serialize};

use crate::types::grant::{Deliverer, Effect, EffectKind, Mode, ModeKind};
use crate::types::scalars::Address;
use crate::types::trigger::{Trigger, TriggerKind};
use crate::types::{Definition, MIN_CONFIRM_DEPTH, ProtocolConfigBody};

/// What `validate` needs from outside the datum.
pub struct ValidateCtx<'a> {
    /// Is this sink in the deployment's registry? Injected so the crate
    /// never depends on `address-registry`.
    pub is_registered_sink: &'a dyn Fn(&Address) -> bool,
    /// The live protocol config, for the cost-table floor on `fuel_cost`
    /// and the currency check on an `Effect::Fuel` rate.
    ///
    /// `None` where a caller has no config in hand — the burn app rendering
    /// a definition card, say. Those two checks are then skipped and
    /// everything else still runs, because a card that cannot be priced is
    /// still a card that can be malformed.
    pub config: Option<&'a ProtocolConfigBody>,
}

impl<'a> ValidateCtx<'a> {
    /// A context that accepts any sink and knows no config — for rendering
    /// and for tests, never for recognition.
    pub fn permissive() -> ValidateCtx<'static> {
        ValidateCtx {
            is_registered_sink: &|_| true,
            config: None,
        }
    }
}

/// Why a definition is not well-formed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum Reason {
    /// A definition that yields nothing is inert by construction.
    NoGrants,
    WindowInverted {
        opens_slot: u64,
        closes_slot: u64,
    },
    ConfirmDepthTooLow {
        depth: u64,
        minimum: u64,
    },
    /// `Trigger::Burn`'s sink is not a registered burn address. Nothing
    /// stops someone posting it; we refuse to recognise a "burn" that is
    /// really a transfer to a spendable address.
    SinkNotRegistered,
    /// A guaranteed grant paying zero per unit.
    ZeroPerUnit {
        grant: u16,
    },
    /// An `Accepts` entry whose unit size is zero — every unit calculation
    /// would divide by it.
    ZeroRawPerUnit {
        accepts_index: usize,
    },
    /// Escrow named but nothing to pay from it.
    EscrowWithoutOnchainGrant,
    /// An on-chain grant with nowhere to pay from.
    OnchainGrantWithoutEscrow {
        grant: u16,
    },
    /// Escrow, but no close.
    ///
    /// The "it expired, give me my prizes back" withdrawal path reads
    /// `closes_slot + settlement_grace`, so an escrow-backed definition
    /// without a close can only ever be reclaimed by spending the
    /// definition itself. That is allowed, but the owner should choose it
    /// knowingly rather than discover it.
    EscrowWithoutClose,
    /// `fuel_cost` below the cost table. It may only ever **raise** the
    /// figure (protocol §3a) — a publisher paying more for priority — so a
    /// definition cannot quietly underpay for the work it asks for.
    GrantUnderpaid {
        grant: u16,
        fuel_cost: u64,
        minimum: u64,
    },
    /// An `Effect::Fuel` naming a token the protocol config does not admit
    /// as currency, or at a rate that disagrees with it.
    FuelRateNotAdmitted {
        grant: u16,
    },
    /// The tag is reserved and its encoding is frozen, but no code
    /// implements it yet.
    NotYetSupported {
        what: Unsupported,
    },
}

/// Which reserved thing a definition reached for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "unsupported", rename_all = "snake_case")]
pub enum Unsupported {
    Trigger {
        kind: TriggerKind,
    },
    Mode {
        kind: ModeKind,
        grant: u16,
    },
    Effect {
        kind: EffectKind,
        grant: u16,
    },
    Deliverer {
        grant: u16,
    },
    /// A tag no build in this lineage has ever known — a definition from a
    /// newer writer. Inert, not an error, exactly as §2.3 intends.
    UnknownTag {
        at: UnknownAt,
    },
}

/// Which enum carried a tag this build does not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownAt {
    Trigger,
    Mode,
    Effect,
    Deliverer,
}

impl UnknownAt {
    pub const ALL: [UnknownAt; 4] = [Self::Trigger, Self::Mode, Self::Effect, Self::Deliverer];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trigger => "trigger",
            Self::Mode => "mode",
            Self::Effect => "effect",
            Self::Deliverer => "deliverer",
        }
    }
}

impl std::fmt::Display for UnknownAt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Reason {
    pub const fn kind(&self) -> ReasonKind {
        match self {
            Self::NoGrants => ReasonKind::NoGrants,
            Self::WindowInverted { .. } => ReasonKind::WindowInverted,
            Self::ConfirmDepthTooLow { .. } => ReasonKind::ConfirmDepthTooLow,
            Self::SinkNotRegistered => ReasonKind::SinkNotRegistered,
            Self::ZeroPerUnit { .. } => ReasonKind::ZeroPerUnit,
            Self::ZeroRawPerUnit { .. } => ReasonKind::ZeroRawPerUnit,
            Self::EscrowWithoutOnchainGrant => ReasonKind::EscrowWithoutOnchainGrant,
            Self::OnchainGrantWithoutEscrow { .. } => ReasonKind::OnchainGrantWithoutEscrow,
            Self::EscrowWithoutClose => ReasonKind::EscrowWithoutClose,
            Self::GrantUnderpaid { .. } => ReasonKind::GrantUnderpaid,
            Self::FuelRateNotAdmitted { .. } => ReasonKind::FuelRateNotAdmitted,
            Self::NotYetSupported { .. } => ReasonKind::NotYetSupported,
        }
    }
}

/// [`Reason`] with its payload stripped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonKind {
    NoGrants,
    WindowInverted,
    ConfirmDepthTooLow,
    SinkNotRegistered,
    ZeroPerUnit,
    ZeroRawPerUnit,
    EscrowWithoutOnchainGrant,
    OnchainGrantWithoutEscrow,
    EscrowWithoutClose,
    GrantUnderpaid,
    FuelRateNotAdmitted,
    NotYetSupported,
}

impl ReasonKind {
    pub const ALL: [ReasonKind; 12] = [
        Self::NoGrants,
        Self::WindowInverted,
        Self::ConfirmDepthTooLow,
        Self::SinkNotRegistered,
        Self::ZeroPerUnit,
        Self::ZeroRawPerUnit,
        Self::EscrowWithoutOnchainGrant,
        Self::OnchainGrantWithoutEscrow,
        Self::EscrowWithoutClose,
        Self::GrantUnderpaid,
        Self::FuelRateNotAdmitted,
        Self::NotYetSupported,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoGrants => "no_grants",
            Self::WindowInverted => "window_inverted",
            Self::ConfirmDepthTooLow => "confirm_depth_too_low",
            Self::SinkNotRegistered => "sink_not_registered",
            Self::ZeroPerUnit => "zero_per_unit",
            Self::ZeroRawPerUnit => "zero_raw_per_unit",
            Self::EscrowWithoutOnchainGrant => "escrow_without_onchain_grant",
            Self::OnchainGrantWithoutEscrow => "onchain_grant_without_escrow",
            Self::EscrowWithoutClose => "escrow_without_close",
            Self::GrantUnderpaid => "grant_underpaid",
            Self::FuelRateNotAdmitted => "fuel_rate_not_admitted",
            Self::NotYetSupported => "not_yet_supported",
        }
    }
}

impl std::fmt::Display for ReasonKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Check a definition against the schema's rules. `Ok(())` or **every**
/// reason it failed.
pub fn validate(def: &Definition, ctx: &ValidateCtx<'_>) -> Result<(), Vec<Reason>> {
    let mut reasons = Vec::new();

    check_trigger(def, ctx, &mut reasons);
    check_window(def, &mut reasons);
    check_filter(def, &mut reasons);
    check_grants(def, ctx, &mut reasons);
    check_escrow(def, &mut reasons);

    if reasons.is_empty() {
        Ok(())
    } else {
        Err(reasons)
    }
}

fn check_trigger(def: &Definition, ctx: &ValidateCtx<'_>, reasons: &mut Vec<Reason>) {
    match &def.trigger {
        Trigger::Burn { sink, .. } => {
            if !(ctx.is_registered_sink)(sink) {
                reasons.push(Reason::SinkNotRegistered);
            }
        }
        Trigger::Unknown { .. } => reasons.push(Reason::NotYetSupported {
            what: Unsupported::UnknownTag {
                at: UnknownAt::Trigger,
            },
        }),
        other => {
            if !other.kind().is_supported() {
                reasons.push(Reason::NotYetSupported {
                    what: Unsupported::Trigger { kind: other.kind() },
                });
            }
        }
    }
}

fn check_window(def: &Definition, reasons: &mut Vec<Reason>) {
    let window = &def.window;
    if let (Some(opens), Some(closes)) = (window.opens_slot, window.closes_slot)
        && opens >= closes
    {
        reasons.push(Reason::WindowInverted {
            opens_slot: opens,
            closes_slot: closes,
        });
    }
    if window.confirm_depth < MIN_CONFIRM_DEPTH {
        reasons.push(Reason::ConfirmDepthTooLow {
            depth: window.confirm_depth,
            minimum: MIN_CONFIRM_DEPTH,
        });
    }
}

fn check_filter(def: &Definition, reasons: &mut Vec<Reason>) {
    for (index, accepts) in def.filter.accepts.iter().enumerate() {
        if accepts.raw_per_unit == 0 {
            reasons.push(Reason::ZeroRawPerUnit {
                accepts_index: index,
            });
        }
    }
}

fn check_grants(def: &Definition, ctx: &ValidateCtx<'_>, reasons: &mut Vec<Reason>) {
    if def.grants.is_empty() {
        reasons.push(Reason::NoGrants);
        return;
    }

    for (ordinal, grant) in def.grants.iter().enumerate() {
        let ordinal = ordinal as u16;

        match &grant.mode {
            Mode::Guaranteed { per_unit, .. } => {
                if *per_unit == 0 {
                    reasons.push(Reason::ZeroPerUnit { grant: ordinal });
                }
            }
            Mode::Unknown { .. } => reasons.push(Reason::NotYetSupported {
                what: Unsupported::UnknownTag {
                    at: UnknownAt::Mode,
                },
            }),
            other => reasons.push(Reason::NotYetSupported {
                what: Unsupported::Mode {
                    kind: other.kind(),
                    grant: ordinal,
                },
            }),
        }

        match &grant.effect {
            Effect::Unknown { .. } => reasons.push(Reason::NotYetSupported {
                what: Unsupported::UnknownTag {
                    at: UnknownAt::Effect,
                },
            }),
            Effect::Agent { .. } => reasons.push(Reason::NotYetSupported {
                what: Unsupported::Effect {
                    kind: EffectKind::Agent,
                    grant: ordinal,
                },
            }),
            Effect::Http { via, .. } => {
                if !via.is_supported() {
                    reasons.push(Reason::NotYetSupported {
                        what: match via {
                            Deliverer::Unknown { .. } => Unsupported::UnknownTag {
                                at: UnknownAt::Deliverer,
                            },
                            _ => Unsupported::Deliverer { grant: ordinal },
                        },
                    });
                }
            }
            Effect::Fuel {
                credits_per_unit, ..
            } => check_fuel_rate(def, ctx, ordinal, *credits_per_unit, reasons),
            Effect::Onchain { .. }
            | Effect::Entitlement { .. }
            | Effect::Notify { .. }
            | Effect::Manual { .. } => {}
        }

        // The cost-table floor. Only checkable with a config in hand.
        if let (Some(config), Some(fuel_cost)) = (ctx.config, grant.fuel_cost)
            && let Some(minimum) = config.cost_of(grant.effect.kind())
            && fuel_cost < minimum
        {
            reasons.push(Reason::GrantUnderpaid {
                grant: ordinal,
                fuel_cost,
                minimum,
            });
        }
    }
}

/// A fuel grant's rate has to match a currency the protocol config admits,
/// for the token this definition accepts. A rate the config does not know is
/// not a malformed definition — it is one the `TopUp` validator would refuse
/// — so it is reported here rather than discovered on chain.
fn check_fuel_rate(
    def: &Definition,
    ctx: &ValidateCtx<'_>,
    ordinal: u16,
    credits_per_unit: u64,
    reasons: &mut Vec<Reason>,
) {
    let Some(config) = ctx.config else { return };

    let admitted = def.filter.accepts.iter().any(|accepts| {
        let name = accepts.name.as_ref().map(|n| n.as_slice()).unwrap_or(&[]);
        config.currency_rate(accepts.policy, name) == Some(credits_per_unit)
    });

    if !admitted {
        reasons.push(Reason::FuelRateNotAdmitted { grant: ordinal });
    }
}

fn check_escrow(def: &Definition, reasons: &mut Vec<Reason>) {
    let onchain: Vec<u16> = def
        .grants
        .iter()
        .enumerate()
        .filter(|(_, g)| matches!(g.effect, Effect::Onchain { .. }))
        .map(|(i, _)| i as u16)
        .collect();

    match (def.escrow.is_some(), onchain.is_empty()) {
        // Escrow named, nothing to pay from it.
        (true, true) => reasons.push(Reason::EscrowWithoutOnchainGrant),
        // On-chain grants with nowhere to pay from.
        (false, false) => {
            for grant in onchain {
                reasons.push(Reason::OnchainGrantWithoutEscrow { grant });
            }
        }
        _ => {}
    }

    if def.escrow.is_some() && def.window.closes_slot.is_none() {
        reasons.push(Reason::EscrowWithoutClose);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{Bytes, UnknownFields};
    use crate::types::definition::Accepts;
    use crate::types::fuel::{CostEntry, Currency};
    use crate::types::grant::{EntitlementGrant, Grant, PolicyFilter, Stacking};
    use crate::types::{
        AssetId, Filter, Limits, PaymentKeyHash, PolicyId, RouteRef, ScriptHash, Window,
    };

    const SINK: [u8; 29] = [0x71; 29];

    fn sink() -> Address {
        Address::from(SINK.to_vec())
    }

    fn base() -> Definition {
        Definition {
            version: 1,
            owner: PaymentKeyHash([1u8; 28]),
            trigger: Trigger::burn(sink()),
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
                confirm_depth: 300,
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
            fuel: AssetId::new(PolicyId([4u8; 28]), b"tank"),
            escrow: None,
            unknown: UnknownFields::default(),
        }
    }

    fn registered() -> ValidateCtx<'static> {
        ValidateCtx {
            is_registered_sink: &|address: &Address| address.as_slice() == SINK,
            config: None,
        }
    }

    fn config() -> ProtocolConfigBody {
        ProtocolConfigBody {
            currencies: vec![Currency {
                policy: PolicyId([2u8; 28]),
                name: Some(Bytes::from(b"PERP".to_vec())),
                credits_per_unit: 3,
                unknown: UnknownFields::default(),
            }],
            cost_table: vec![
                CostEntry::new(EffectKind::Notify, 1).unwrap(),
                CostEntry::new(EffectKind::Onchain, 5).unwrap(),
            ],
            ..ProtocolConfigBody::default()
        }
    }

    fn reasons(def: &Definition, ctx: &ValidateCtx<'_>) -> Vec<ReasonKind> {
        validate(def, ctx)
            .err()
            .unwrap_or_default()
            .iter()
            .map(Reason::kind)
            .collect()
    }

    #[test]
    fn a_well_formed_definition_passes() {
        assert_eq!(validate(&base(), &registered()), Ok(()));
    }

    #[test]
    fn an_unregistered_sink_is_refused() {
        let mut def = base();
        def.trigger = Trigger::burn(Address::from(vec![0x61; 29]));
        assert_eq!(
            reasons(&def, &registered()),
            vec![ReasonKind::SinkNotRegistered]
        );
    }

    #[test]
    fn every_reason_is_returned_not_just_the_first() {
        let mut def = base();
        def.trigger = Trigger::burn(Address::from(vec![0x61; 29])); // unregistered
        def.window.opens_slot = Some(500); // after the close
        def.window.confirm_depth = 10; // below the floor
        def.filter.accepts[0].raw_per_unit = 0;
        def.grants = Vec::new();

        let kinds = reasons(&def, &registered());
        assert!(kinds.contains(&ReasonKind::SinkNotRegistered));
        assert!(kinds.contains(&ReasonKind::WindowInverted));
        assert!(kinds.contains(&ReasonKind::ConfirmDepthTooLow));
        assert!(kinds.contains(&ReasonKind::ZeroRawPerUnit));
        assert!(kinds.contains(&ReasonKind::NoGrants));
        assert_eq!(kinds.len(), 5, "an owner should see the whole list at once");
    }

    #[test]
    fn a_window_that_closes_before_it_opens_is_refused() {
        let mut def = base();
        def.window.opens_slot = Some(200);
        def.window.closes_slot = Some(100);
        assert_eq!(
            reasons(&def, &registered()),
            vec![ReasonKind::WindowInverted]
        );
    }

    #[test]
    fn an_open_ended_window_is_fine() {
        let mut def = base();
        def.window.opens_slot = None;
        def.window.closes_slot = None;
        assert_eq!(validate(&def, &registered()), Ok(()));
    }

    #[test]
    fn confirm_depth_has_a_floor() {
        let mut def = base();
        def.window.confirm_depth = MIN_CONFIRM_DEPTH - 1;
        assert_eq!(
            reasons(&def, &registered()),
            vec![ReasonKind::ConfirmDepthTooLow]
        );

        def.window.confirm_depth = MIN_CONFIRM_DEPTH;
        assert_eq!(validate(&def, &registered()), Ok(()));
    }

    #[test]
    fn a_guaranteed_grant_paying_nothing_is_refused() {
        let mut def = base();
        def.grants[0].mode = Mode::guaranteed(0);
        assert_eq!(reasons(&def, &registered()), vec![ReasonKind::ZeroPerUnit]);
    }

    #[test]
    fn escrow_and_onchain_grants_must_agree_in_both_directions() {
        // Escrow with nothing to pay from it.
        let mut def = base();
        def.escrow = Some(ScriptHash([5u8; 28]));
        assert_eq!(
            reasons(&def, &registered()),
            vec![ReasonKind::EscrowWithoutOnchainGrant]
        );

        // An on-chain grant with nowhere to pay from.
        let mut def = base();
        def.grants.push(Grant::new(
            Mode::guaranteed(1),
            Effect::Onchain {
                filter: PolicyFilter::default(),
                unknown: UnknownFields::default(),
            },
        ));
        assert_eq!(
            reasons(&def, &registered()),
            vec![ReasonKind::OnchainGrantWithoutEscrow]
        );

        // Both present: fine.
        def.escrow = Some(ScriptHash([5u8; 28]));
        assert_eq!(validate(&def, &registered()), Ok(()));
    }

    #[test]
    fn an_escrow_backed_definition_needs_a_close() {
        let mut def = base();
        def.escrow = Some(ScriptHash([5u8; 28]));
        def.window.closes_slot = None;
        def.grants.push(Grant::new(
            Mode::guaranteed(1),
            Effect::Onchain {
                filter: PolicyFilter::default(),
                unknown: UnknownFields::default(),
            },
        ));
        assert_eq!(
            reasons(&def, &registered()),
            vec![ReasonKind::EscrowWithoutClose]
        );
    }

    #[test]
    fn reserved_variants_are_not_yet_supported_but_still_decode() {
        let mut def = base();
        def.grants[0].mode = Mode::Raffle {
            tickets_per_unit: 1,
            draw_slot: 500,
            prizes: 3,
            unknown: UnknownFields::default(),
        };
        assert_eq!(
            reasons(&def, &registered()),
            vec![ReasonKind::NotYetSupported]
        );

        let mut def = base();
        def.grants[0].effect = Effect::Http {
            via: Deliverer::Marsbirds {
                unknown: UnknownFields::default(),
            },
            unknown: UnknownFields::default(),
        };
        assert_eq!(
            reasons(&def, &registered()),
            vec![ReasonKind::NotYetSupported]
        );

        let mut def = base();
        def.trigger = Trigger::Mint {
            policy: PolicyId([9u8; 28]),
            unknown: UnknownFields::default(),
        };
        assert_eq!(
            reasons(&def, &registered()),
            vec![ReasonKind::NotYetSupported]
        );
    }

    #[test]
    fn an_unknown_tag_is_reported_as_unsupported_rather_than_a_decode_failure() {
        let mut def = base();
        def.trigger = Trigger::Unknown {
            tag: 42,
            fields: UnknownFields::default(),
        };
        let errors = validate(&def, &registered()).unwrap_err();
        assert_eq!(
            errors,
            vec![Reason::NotYetSupported {
                what: Unsupported::UnknownTag {
                    at: UnknownAt::Trigger
                }
            }]
        );
    }

    #[test]
    fn fuel_cost_may_raise_the_cost_table_but_never_lower_it() {
        let config = config();
        let ctx = ValidateCtx {
            is_registered_sink: &|address: &Address| address.as_slice() == SINK,
            config: Some(&config),
        };

        // Notify costs 1. Paying 5 is a publisher buying priority.
        let mut def = base();
        def.grants[0].fuel_cost = Some(5);
        assert_eq!(validate(&def, &ctx), Ok(()));

        // Paying 0 is underpaying for work we will do.
        def.grants[0].fuel_cost = Some(0);
        assert_eq!(
            validate(&def, &ctx).unwrap_err(),
            vec![Reason::GrantUnderpaid {
                grant: 0,
                fuel_cost: 0,
                minimum: 1
            }]
        );
    }

    #[test]
    fn the_cost_floor_is_skipped_when_the_caller_has_no_config() {
        // The burn app renders a card without a config; a definition it
        // cannot price is still one it can check the shape of.
        let mut def = base();
        def.grants[0].fuel_cost = Some(0);
        assert_eq!(validate(&def, &registered()), Ok(()));
    }

    #[test]
    fn a_fuel_grant_must_match_an_admitted_currency() {
        let config = config();
        let ctx = ValidateCtx {
            is_registered_sink: &|address: &Address| address.as_slice() == SINK,
            config: Some(&config),
        };

        let mut def = base();
        def.grants[0].effect = Effect::Fuel {
            credits_per_unit: 3, // matches the config's $PERP rate
            unknown: UnknownFields::default(),
        };
        assert_eq!(validate(&def, &ctx), Ok(()));

        // A rate the config does not admit: the TopUp validator would
        // refuse this burn, so say so before anyone posts it.
        def.grants[0].effect = Effect::Fuel {
            credits_per_unit: 99,
            unknown: UnknownFields::default(),
        };
        assert_eq!(
            validate(&def, &ctx).unwrap_err(),
            vec![Reason::FuelRateNotAdmitted { grant: 0 }]
        );
    }

    #[test]
    fn an_entitlement_grant_is_supported() {
        let mut def = base();
        def.grants[0].effect = Effect::Entitlement {
            product: "flow.full".into(),
            grant: EntitlementGrant::Days {
                days: 30,
                stacking: Stacking::Extend,
                unknown: UnknownFields::default(),
            },
            unknown: UnknownFields::default(),
        };
        assert_eq!(validate(&def, &registered()), Ok(()));
    }

    #[test]
    fn every_reason_kind_has_a_distinct_string() {
        let mut seen = std::collections::HashSet::new();
        for kind in ReasonKind::ALL {
            assert!(seen.insert(kind.as_str()), "duplicate string for {kind}");
        }
    }
}
