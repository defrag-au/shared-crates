//! Is this definition **in force**? — the registry's recognition rule as one
//! pure function.
//!
//! `validate` asks whether the datum is well-formed. This asks the separate
//! question of whether the thing behind it is funded and owned by whoever
//! posted it. Both are the service's judgement, not the ledger's: no
//! validator reads a holder, and the settlement floor only sees a balance as
//! of the last reconciliation.
//!
//! The chain facts arrive as [`TankView`] / [`EscrowView`], read by the
//! caller. Nothing in here fetches anything.

use serde::{Deserialize, Serialize};

use crate::types::grant::Effect;
use crate::types::scalars::{AssetId, PaymentKeyHash, PolicyId};
use crate::types::{Definition, FuelBody, ProtocolConfigBody};

/// What the service can see of a tank.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TankView {
    pub asset: AssetId,
    /// The policy the `(100)` reference token is **actually** under.
    ///
    /// This field is the look-alike-tank defence. Nothing stops someone
    /// minting a CIP-68 pair under their own keyless policy, parking it at
    /// the fuel validator's address with an invented balance, and naming it
    /// in a definition. The escrow validator refuses such a tank because it
    /// is parameterised by the real fuel policy id; recognition refuses it
    /// here for the same reason, so the definition never goes live in the
    /// first place.
    pub policy: PolicyId,
    pub body: FuelBody,
    /// Credits owed but not yet reconciled — the service's own figure, and
    /// the reason a tank can be out of fuel while the datum still reads
    /// healthy. The on-chain balance lags by up to a day by design.
    pub pending: u64,
    /// Current holder of the `(222)` user token.
    pub holder: PaymentKeyHash,
}

impl TankView {
    /// What is actually available to spend.
    pub fn available(&self) -> u64 {
        self.body.balance.saturating_sub(self.pending)
    }
}

/// What the service can see of a definition's escrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscrowView {
    pub ada_lovelace: u64,
    /// Prizes still owed — what the release reserve is measured against.
    pub outstanding_prizes: u32,
}

/// Whether a definition is offered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Recognition {
    /// Offered: shown in the app, announced, burns built against it.
    Live,
    /// Inert but recoverable — every one of these flips back on its own
    /// when the missing thing arrives. **Nothing is ever stranded by it**:
    /// escrow withdrawal and cancellation never read fuel.
    Unfunded { missing: Vec<UnfundedReason> },
    /// Wrong in a way topping up cannot fix.
    Rejected { reasons: Vec<RejectReason> },
}

impl Recognition {
    pub const fn is_live(&self) -> bool {
        matches!(self, Self::Live)
    }
}

/// Why a definition is inert but recoverable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnfundedReason {
    /// `balance − pending` is zero.
    NoFuel,
    /// The `(222)` token changed hands. `Register` proved ownership at
    /// posting time and does not follow the token, so the new holder
    /// re-registers before the definition draws on their tank again — a
    /// buyer gets a tank with a known balance and nobody else spending it.
    OwnerChanged,
    /// Escrow ADA is below `release_reserve_per_prize × outstanding_prizes`.
    /// An under-reserved escrow simply cannot settle its last prizes until
    /// it is topped up — and can always be withdrawn in full.
    EscrowReserve,
    /// An `Onchain` grant with no escrow UTxO found at all.
    EscrowMissing,
    /// A `Fuel` grant naming a token the protocol config does not admit.
    /// Harmless and inert; admitting the currency is one cold-key update.
    CurrencyNotAdmitted,
}

impl UnfundedReason {
    pub const ALL: [UnfundedReason; 5] = [
        Self::NoFuel,
        Self::OwnerChanged,
        Self::EscrowReserve,
        Self::EscrowMissing,
        Self::CurrencyNotAdmitted,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoFuel => "no_fuel",
            Self::OwnerChanged => "owner_changed",
            Self::EscrowReserve => "escrow_reserve",
            Self::EscrowMissing => "escrow_missing",
            Self::CurrencyNotAdmitted => "currency_not_admitted",
        }
    }
}

impl std::fmt::Display for UnfundedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a definition is refused outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    /// The named tank's `(100)` token is not under the deployment's fuel
    /// policy — a look-alike parked at the right address.
    LookAlikeTank,
    /// The definition names a tank other than the one it was handed.
    TankMismatch,
}

impl RejectReason {
    pub const ALL: [RejectReason; 2] = [Self::LookAlikeTank, Self::TankMismatch];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LookAlikeTank => "look_alike_tank",
            Self::TankMismatch => "tank_mismatch",
        }
    }
}

impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Decide whether a definition is offered.
///
/// `definition_owner` is the **envelope** owner as posted, and
/// `release_reserve_per_prize` comes from the caller's
/// `ActionProtocolDeployment` record rather than a constant here: it is
/// compiled into the escrow validator, so it is a deployment fact that
/// cannot change without a redeploy, and the registry-single-source rule
/// says it lives in exactly one place.
///
/// Assumes [`crate::validate`] already passed — this answers a different
/// question and does not re-check the datum's shape.
pub fn recognise(
    def: &Definition,
    definition_owner: &PaymentKeyHash,
    tank: &TankView,
    fuel_policy: &PolicyId,
    escrow: Option<&EscrowView>,
    config: &ProtocolConfigBody,
    release_reserve_per_prize: u64,
) -> Recognition {
    let mut rejects = Vec::new();

    if tank.policy != *fuel_policy {
        rejects.push(RejectReason::LookAlikeTank);
    }
    if tank.asset != def.fuel {
        rejects.push(RejectReason::TankMismatch);
    }
    if !rejects.is_empty() {
        return Recognition::Rejected { reasons: rejects };
    }

    let mut missing = Vec::new();

    if tank.holder != *definition_owner {
        missing.push(UnfundedReason::OwnerChanged);
    }

    if tank.available() == 0 {
        missing.push(UnfundedReason::NoFuel);
    }

    let has_onchain = def
        .grants
        .iter()
        .any(|g| matches!(g.effect, Effect::Onchain { .. }));

    if has_onchain {
        match escrow {
            None => missing.push(UnfundedReason::EscrowMissing),
            Some(escrow) => {
                let required =
                    u64::from(escrow.outstanding_prizes).saturating_mul(release_reserve_per_prize);
                if escrow.ada_lovelace < required {
                    missing.push(UnfundedReason::EscrowReserve);
                }
            }
        }
    }

    for grant in &def.grants {
        if let Effect::Fuel {
            credits_per_unit, ..
        } = &grant.effect
        {
            let admitted = def.filter.accepts.iter().any(|accepts| {
                let name = accepts.name.as_ref().map(|n| n.as_slice()).unwrap_or(&[]);
                config.currency_rate(accepts.policy, name) == Some(*credits_per_unit)
            });
            if !admitted {
                missing.push(UnfundedReason::CurrencyNotAdmitted);
                break;
            }
        }
    }

    if missing.is_empty() {
        Recognition::Live
    } else {
        Recognition::Unfunded { missing }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{Bytes, UnknownFields};
    use crate::types::definition::Accepts;
    use crate::types::fuel::Currency;
    use crate::types::grant::Effect;
    use crate::types::grant::{Grant, Mode, PolicyFilter};
    use crate::types::{Address, Filter, Limits, RouteRef, ScriptHash, Trigger, Window};

    /// The figure the escrow validator enforces, from the deployment record.
    const RESERVE: u64 = 2_500_000;

    fn fuel_policy() -> PolicyId {
        PolicyId([9u8; 28])
    }

    fn owner() -> PaymentKeyHash {
        PaymentKeyHash([1u8; 28])
    }

    fn tank_asset() -> AssetId {
        AssetId::new(fuel_policy(), b"(222)tank")
    }

    fn tank() -> TankView {
        TankView {
            asset: tank_asset(),
            policy: fuel_policy(),
            body: FuelBody {
                balance: 100,
                ..FuelBody::default()
            },
            pending: 0,
            holder: owner(),
        }
    }

    fn def() -> Definition {
        Definition {
            version: 1,
            owner: owner(),
            trigger: Trigger::burn(Address::from(vec![0x71; 29])),
            filter: Filter {
                accepts: vec![Accepts {
                    policy: PolicyId([2u8; 28]),
                    name: Some(Bytes::from(b"PERP".to_vec())),
                    raw_per_unit: 1,
                    unknown: UnknownFields::default(),
                }],
                ..Filter::default()
            },
            window: Window::default(),
            grants: vec![Grant::new(
                Mode::guaranteed(1),
                Effect::Notify {
                    route: RouteRef([3u8; 16]),
                    unknown: UnknownFields::default(),
                },
            )],
            limits: Limits::default(),
            title: String::new(),
            supersedes: None,
            fuel: tank_asset(),
            escrow: None,
            unknown: UnknownFields::default(),
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
            ..ProtocolConfigBody::default()
        }
    }

    fn run(def: &Definition, tank: &TankView, escrow: Option<&EscrowView>) -> Recognition {
        recognise(
            def,
            &owner(),
            tank,
            &fuel_policy(),
            escrow,
            &config(),
            RESERVE,
        )
    }

    #[test]
    fn a_funded_definition_is_live() {
        assert_eq!(run(&def(), &tank(), None), Recognition::Live);
    }

    #[test]
    fn a_tank_under_a_foreign_policy_is_rejected_not_merely_unfunded() {
        // The look-alike: right address, right shape, invented balance,
        // wrong policy. Topping it up would not help, so it is a rejection.
        let mut tank = tank();
        tank.policy = PolicyId([0xff; 28]);
        assert_eq!(
            run(&def(), &tank, None),
            Recognition::Rejected {
                reasons: vec![RejectReason::LookAlikeTank]
            }
        );
    }

    #[test]
    fn a_definition_naming_a_different_tank_is_rejected() {
        let mut def = def();
        def.fuel = AssetId::new(fuel_policy(), b"someone elses tank");
        assert_eq!(
            run(&def, &tank(), None),
            Recognition::Rejected {
                reasons: vec![RejectReason::TankMismatch]
            }
        );
    }

    #[test]
    fn an_empty_tank_is_unfunded() {
        let mut tank = tank();
        tank.body.balance = 0;
        assert_eq!(
            run(&def(), &tank, None),
            Recognition::Unfunded {
                missing: vec![UnfundedReason::NoFuel]
            }
        );
    }

    #[test]
    fn pending_work_counts_against_the_balance() {
        // The datum still reads 100; the service knows 100 is already spoken
        // for. This is the number the burn builder subtracts, and it is why
        // a tank can be out of fuel a day before the chain says so.
        let mut tank = tank();
        tank.pending = 100;
        assert_eq!(tank.available(), 0);
        assert_eq!(
            run(&def(), &tank, None),
            Recognition::Unfunded {
                missing: vec![UnfundedReason::NoFuel]
            }
        );
    }

    #[test]
    fn a_transferred_tank_unfunds_the_previous_holders_definitions() {
        let mut tank = tank();
        tank.holder = PaymentKeyHash([0xaa; 28]);
        assert_eq!(
            run(&def(), &tank, None),
            Recognition::Unfunded {
                missing: vec![UnfundedReason::OwnerChanged]
            }
        );
    }

    #[test]
    fn an_onchain_grant_needs_escrow_covering_the_release_reserve() {
        let mut def = def();
        def.escrow = Some(ScriptHash([5u8; 28]));
        def.grants.push(Grant::new(
            Mode::guaranteed(1),
            Effect::Onchain {
                filter: PolicyFilter::default(),
                unknown: UnknownFields::default(),
            },
        ));

        // Three prizes need 7.5 ADA of release reserve.
        let short = EscrowView {
            ada_lovelace: 7_499_999,
            outstanding_prizes: 3,
        };
        assert_eq!(
            run(&def, &tank(), Some(&short)),
            Recognition::Unfunded {
                missing: vec![UnfundedReason::EscrowReserve]
            }
        );

        let exact = EscrowView {
            ada_lovelace: 7_500_000,
            outstanding_prizes: 3,
        };
        assert_eq!(run(&def, &tank(), Some(&exact)), Recognition::Live);
    }

    #[test]
    fn an_onchain_grant_with_no_escrow_utxo_at_all_is_unfunded() {
        let mut def = def();
        def.escrow = Some(ScriptHash([5u8; 28]));
        def.grants.push(Grant::new(
            Mode::guaranteed(1),
            Effect::Onchain {
                filter: PolicyFilter::default(),
                unknown: UnknownFields::default(),
            },
        ));
        assert_eq!(
            run(&def, &tank(), None),
            Recognition::Unfunded {
                missing: vec![UnfundedReason::EscrowMissing]
            }
        );
    }

    #[test]
    fn a_settled_out_escrow_needs_no_reserve() {
        let mut def = def();
        def.escrow = Some(ScriptHash([5u8; 28]));
        def.grants.push(Grant::new(
            Mode::guaranteed(1),
            Effect::Onchain {
                filter: PolicyFilter::default(),
                unknown: UnknownFields::default(),
            },
        ));
        let empty = EscrowView {
            ada_lovelace: 0,
            outstanding_prizes: 0,
        };
        assert_eq!(run(&def, &tank(), Some(&empty)), Recognition::Live);
    }

    #[test]
    fn a_fuel_grant_for_an_unadmitted_currency_is_inert_not_rejected() {
        // Protocol §1.5: "A fuel definition for a token not in the config is
        // harmless and inert." Admitting the currency is one cold-key
        // update, and this flips back on its own when it happens.
        let mut def = def();
        def.grants[0].effect = Effect::Fuel {
            credits_per_unit: 99,
            unknown: UnknownFields::default(),
        };
        assert_eq!(
            run(&def, &tank(), None),
            Recognition::Unfunded {
                missing: vec![UnfundedReason::CurrencyNotAdmitted]
            }
        );

        def.grants[0].effect = Effect::Fuel {
            credits_per_unit: 3,
            unknown: UnknownFields::default(),
        };
        assert_eq!(run(&def, &tank(), None), Recognition::Live);
    }

    #[test]
    fn several_problems_are_all_reported() {
        let mut tank = tank();
        tank.body.balance = 0;
        tank.holder = PaymentKeyHash([0xaa; 28]);

        let Recognition::Unfunded { missing } = run(&def(), &tank, None) else {
            panic!("expected Unfunded");
        };
        assert!(missing.contains(&UnfundedReason::NoFuel));
        assert!(missing.contains(&UnfundedReason::OwnerChanged));
    }

    #[test]
    fn reason_strings_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        for reason in UnfundedReason::ALL {
            assert!(seen.insert(reason.as_str()));
        }
        for reason in RejectReason::ALL {
            assert!(seen.insert(reason.as_str()));
        }
    }
}
