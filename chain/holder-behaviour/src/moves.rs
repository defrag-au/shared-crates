//! What a linked identity can do next, priced by the rule.
//!
//! A page that says "hold two more Perps for one more guaranteed mint" must be saying what the
//! allocation would then give, so every [`Move`] here is [`identity_sources`] read before and after
//! the change: the same wallets, the same pool, the same merged holder, the same ceiling. A change
//! this cannot model exactly is modelled so that it under-states: a wallet that grows past the stake
//! leaves the pool and the pool is no longer read; an unlisted Perp comes back as a fresh one.
//!
//! Settled 2026-11-12, with the version 3 page: the next moves are per wallet and per track,
//! because each wallet earns on its own. Bloodline has no move: it is the mint and time held, and
//! nothing bought before the snapshot adds it.

use crate::allocation::{Allocation, Counts, Maturity, Slots, Sources, Track, identity_sources};

/// The most Perps a holding move searches: past it, a rung is not a next move.
pub const HOLD_SEARCH: u32 = 200;

/// An identity's wallets, as [`identity_sources`] reads them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Linked {
    pub wallets: Vec<Counts>,
    /// The wallets under [`Allocation::gtd_min_held`] as one holder, when two or more are.
    pub pool: Option<Counts>,
    /// Every wallet as one holder.
    pub merged: Option<Counts>,
}

impl Linked {
    pub fn sources(&self, policy: Allocation) -> Sources {
        identity_sources(
            &self.wallets,
            self.pool.as_ref(),
            self.merged.as_ref(),
            policy,
        )
    }

    pub fn slots(&self, policy: Allocation) -> Slots {
        self.sources(policy).total()
    }

    /// `change` made on wallet `index`, and on the pool and the merged holder that include it.
    fn with(&self, index: usize, policy: Allocation, change: Change) -> Self {
        let mut out = self.clone();
        let was_small = self.wallets[index].current < policy.gtd_min_held;
        change.apply(&mut out.wallets[index]);
        let still_small = out.wallets[index].current < policy.gtd_min_held;
        if was_small {
            // Grown to the stake, it is scored alone: the pool it leaves cannot be re-read from
            // here (it is a union of earned sets), so it is dropped, which only under-states.
            match still_small {
                true => out.pool.iter_mut().for_each(|pool| change.apply(pool)),
                false => out.pool = None,
            }
        }
        out.merged
            .iter_mut()
            .for_each(|merged| change.apply(merged));
        out
    }

    /// Every wallet `ripe` names, its maturing and new units counted as mature: the identity at the
    /// snapshot if nothing else changes.
    pub fn ripened(&self, policy: Allocation, ripe: &[bool]) -> Self {
        (0..self.wallets.len())
            .filter(|index| ripe.get(*index).copied().unwrap_or(false))
            .fold(self.clone(), |linked, index| {
                let change = Change::ripen(&linked.wallets[index]);
                linked.with(index, policy, change)
            })
    }
}

/// One change to one wallet.
#[derive(Debug, Clone, Copy, Default)]
struct Change {
    /// Units added, at `stage`.
    adopt: u32,
    stage: Option<Maturity>,
    /// New and maturing units now counted as mature.
    ripen_new: u32,
    ripen_maturing: u32,
    clout: u32,
    token: u64,
}

impl Change {
    fn adopt(units: u32, stage: Maturity) -> Self {
        Self {
            adopt: units,
            stage: Some(stage),
            ..Self::default()
        }
    }

    fn ripen(wallet: &Counts) -> Self {
        let held = wallet.maturity();
        Self {
            ripen_new: held.new,
            ripen_maturing: held.maturing,
            ..Self::default()
        }
    }

    fn apply(&self, counts: &mut Counts) {
        let mut held = counts.maturity();
        let new = self.ripen_new.min(held.new);
        let maturing = self.ripen_maturing.min(held.maturing);
        held.new -= new;
        held.maturing -= maturing;
        held.mature += new + maturing;
        match self.stage {
            Some(Maturity::New) => held.new += self.adopt,
            Some(Maturity::Maturing) => held.maturing += self.adopt,
            Some(Maturity::Mature) => held.mature += self.adopt,
            None => {}
        }
        counts.current += self.adopt;
        counts.peak = counts.peak.max(counts.current);
        counts.enabled |= counts.current > 0;
        counts.matured = held;
        counts.clout = counts.clout.saturating_add(self.clout);
        counts.token = counts.token.saturating_add(self.token);
    }
}

/// What a move asks of the holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Step {
    /// The wallet's maturing and new units, held to the snapshot, count in full.
    Mature { units: u32 },
    /// Hold `more` units on the wallet, beside the `held` it has.
    Hold { held: u32, more: u32 },
    /// Clout: `points` now, and the line `at` that adds a first-come mint.
    Clout { points: u32, at: u32 },
    /// The token: `units` counted now, and the step `at` that adds a guaranteed mint.
    Token { units: u64, at: u64 },
    /// Take the wallet's `units` listed Perps off the market.
    Unlist { units: u32 },
}

/// One thing one wallet can do, and what the identity's allocation is before and after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Move {
    /// Index into the identity's wallets.
    pub wallet: usize,
    pub step: Step,
    pub before: Slots,
    pub after: Slots,
}

impl Move {
    /// Guaranteed mints it adds.
    pub fn gtd(&self) -> u32 {
        self.after.gtd.saturating_sub(self.before.gtd)
    }

    /// First-come mints it adds; zero when it trades them for guaranteed ones.
    pub fn fcfs(&self) -> u32 {
        self.after.fcfs.saturating_sub(self.before.fcfs)
    }

    /// First-come mints it trades for guaranteed ones: a wallet reaching the stake.
    pub fn fcfs_traded(&self) -> u32 {
        self.before.fcfs.saturating_sub(self.after.fcfs)
    }
}

/// What the identity is told it can do: the inputs [`moves`] reads beside the wallets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Horizon {
    /// The stage a unit adopted now has reached at the snapshot.
    pub fresh: Option<Maturity>,
    /// Per wallet: whether every unit it holds now is mature by the snapshot.
    pub ripe: Vec<bool>,
    /// Per wallet: the Perps it has listed for sale, which count for nothing while listed.
    pub listed: Vec<u32>,
}

/// Whether `after` is better for the holder than `before`, strictly.
fn gains(before: Slots, after: Slots) -> bool {
    (after.gtd, after.total()) > (before.gtd, before.total())
}

/// Every move each wallet has, priced. Maturing moves are read from the identity now; every other
/// move from the identity at the snapshot if nothing else changes (each wallet `ripe` names
/// matured), so "two more Perps for one more" is on top of what waiting already brings.
///
/// For each wallet, at most one move of each kind: the smallest that adds a mint. Clout and the
/// token are tried against the wallet's own next line and the merged holder's, because either
/// reading can be the one the identity takes.
pub fn moves(linked: &Linked, policy: Allocation, horizon: &Horizon) -> Vec<Move> {
    let fresh = horizon.fresh.unwrap_or(Maturity::New);
    let now = linked.slots(policy);
    let then_linked = linked.ripened(policy, &horizon.ripe);
    let then = then_linked.slots(policy);
    let mut out = Vec::new();

    for (index, wallet) in linked.wallets.iter().enumerate() {
        let priced = |base: &Linked, before: Slots, change: Change, step: Step| -> Option<Move> {
            let after = base.with(index, policy, change).slots(policy);
            gains(before, after).then_some(Move {
                wallet: index,
                step,
                before,
                after,
            })
        };

        // Maturing: this wallet alone ripened, from now.
        let held = wallet.maturity();
        let unripe = held.new + held.maturing;
        if unripe > 0 && horizon.ripe.get(index).copied().unwrap_or(false) {
            out.extend(priced(
                linked,
                now,
                Change::ripen(wallet),
                Step::Mature { units: unripe },
            ));
        }

        let wallet_then = &then_linked.wallets[index];

        // Unlisting: the listed Perps back, as fresh ones.
        if let Some(units) = horizon.listed.get(index).copied().filter(|n| *n > 0) {
            out.extend(priced(
                &then_linked,
                then,
                Change::adopt(units, fresh),
                Step::Unlist { units },
            ));
        }

        // Holding: the fewest more Perps that add a mint.
        if !policy.holding.is_empty() {
            out.extend((1..=HOLD_SEARCH).find_map(|more| {
                priced(
                    &then_linked,
                    then,
                    Change::adopt(more, fresh),
                    Step::Hold {
                        held: wallet_then.current,
                        more,
                    },
                )
            }));
        }

        // Clout: the wallet's own next line, or the merged holder's.
        let clout_lines = [
            Some(wallet_then.clout),
            then_linked.merged.as_ref().map(|m| m.clout),
        ];
        let mut clout_gaps: Vec<(u32, u32, u32)> = clout_lines
            .into_iter()
            .flatten()
            .filter_map(|points| {
                policy
                    .track_next(Track::Clout, points)
                    .map(|rung| (rung.at_least - points, points, rung.at_least))
            })
            .collect();
        clout_gaps.sort_unstable();
        out.extend(clout_gaps.into_iter().find_map(|(gap, points, at)| {
            priced(
                &then_linked,
                then,
                Change {
                    clout: gap,
                    ..Change::default()
                },
                Step::Clout { points, at },
            )
        }));

        // The token: the wallet's own next step, or the merged holder's.
        if let Some(rule) = policy.token {
            let token_lines = [
                Some(wallet_then.token),
                then_linked.merged.as_ref().map(|m| m.token),
            ];
            let mut token_gaps: Vec<(u64, u64, u64)> = token_lines
                .into_iter()
                .flatten()
                .filter_map(|units| next_token_step(&rule, units).map(|at| (at - units, units, at)))
                .collect();
            token_gaps.sort_unstable();
            out.extend(token_gaps.into_iter().find_map(|(gap, units, at)| {
                priced(
                    &then_linked,
                    then,
                    Change {
                        token: gap,
                        ..Change::default()
                    },
                    Step::Token { units, at },
                )
            }));
        }
    }
    out
}

/// The next balance past `units` that the token ladder pays more at.
fn next_token_step(rule: &crate::allocation::TokenRule, units: u64) -> Option<u64> {
    if let Some(step) = rule.steps().iter().find(|step| step.at_least > units) {
        return Some(step.at_least);
    }
    let (tail, last) = (rule.tail?, rule.steps().last()?);
    let past = units.saturating_sub(last.at_least);
    Some(last.at_least + (past / tail.every + 1) * tail.every)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allocation::{Matured, Rung, Rungs, Seasoning, TokenStep, TokenTail};

    fn rung(at_least: u32, mints: u32) -> Rung {
        Rung { at_least, mints }
    }

    /// The settled Perps rule, cut to what these tests read: one per five Perps, clout at 6,000
    /// and 12,000, $PERP at 3M and 10M then every 10M, five to be guaranteed.
    fn policy() -> Allocation {
        Allocation {
            holding: Rungs::new(&[
                rung(1, 0),
                rung(5, 1),
                rung(10, 2),
                rung(15, 3),
                rung(20, 4),
            ])
            .unwrap(),
            holding_tail: Some(crate::allocation::Tail { every: 5, mints: 1 }),
            clout: Rungs::new(&[rung(6_000, 1), rung(12_000, 1)]).unwrap(),
            gtd_min_held: 5,
            seasoning: Some(Seasoning {
                maturing_days: 4,
                mature_days: 8,
                new_share: 0.5,
                maturing_share: 0.75,
            }),
            token: Some(
                crate::allocation::TokenRule::new(
                    &[
                        TokenStep {
                            at_least: 3_000_000,
                            mints: 1,
                        },
                        TokenStep {
                            at_least: 10_000_000,
                            mints: 2,
                        },
                    ],
                    Some(TokenTail {
                        every: 10_000_000,
                        mints: 1,
                    }),
                    50,
                )
                .unwrap(),
            ),
            ..Allocation::default()
        }
    }

    fn wallet(mature: u32, maturing: u32, new: u32) -> Counts {
        let current = mature + maturing + new;
        Counts {
            current,
            peak: current,
            enabled: current > 0,
            matured: Matured {
                new,
                maturing,
                mature,
            },
            ..Counts::default()
        }
    }

    fn merged(wallets: &[Counts]) -> Counts {
        wallets.iter().fold(Counts::default(), |sum, w| Counts {
            current: sum.current + w.current,
            peak: sum.peak + w.peak,
            enabled: sum.enabled || w.enabled,
            matured: sum.matured.plus(&w.matured),
            clout: sum.clout + w.clout,
            token: sum.token + w.token,
            ..sum
        })
    }

    fn linked(wallets: Vec<Counts>) -> Linked {
        Linked {
            merged: Some(merged(&wallets)),
            wallets,
            pool: None,
        }
    }

    fn find(moves: &[Move], wallet: usize, kind: fn(&Step) -> bool) -> Option<Move> {
        moves
            .iter()
            .find(|m| m.wallet == wallet && kind(&m.step))
            .copied()
    }

    #[test]
    fn two_more_mature_perps_reach_the_next_rung() {
        let linked = linked(vec![wallet(8, 0, 0)]);
        let horizon = Horizon {
            fresh: Some(Maturity::Mature),
            ripe: vec![true],
            listed: vec![0],
        };
        let moves = moves(&linked, policy(), &horizon);
        let hold = find(&moves, 0, |s| matches!(s, Step::Hold { .. })).expect("a rung is left");
        assert_eq!(hold.step, Step::Hold { held: 8, more: 2 });
        assert_eq!((hold.gtd(), hold.before.gtd, hold.after.gtd), (1, 1, 2));
    }

    #[test]
    fn a_fresh_perp_counts_half_so_the_rung_takes_twice_the_gap() {
        let linked = linked(vec![wallet(8, 0, 0)]);
        let horizon = Horizon {
            fresh: Some(Maturity::New),
            ripe: vec![true],
            listed: vec![0],
        };
        let hold = find(&moves(&linked, policy(), &horizon), 0, |s| {
            matches!(s, Step::Hold { .. })
        })
        .unwrap();
        assert_eq!(hold.step, Step::Hold { held: 8, more: 4 });
    }

    #[test]
    fn maturing_perps_held_to_the_snapshot_are_priced_from_now() {
        // Six held, four of them new: 2 + 4 × 0.5 = 4 units now, one short of the guarantee.
        let linked = linked(vec![wallet(2, 0, 4)]);
        let horizon = Horizon {
            fresh: Some(Maturity::Mature),
            ripe: vec![true],
            listed: vec![0],
        };
        let moves = moves(&linked, policy(), &horizon);
        let mature = find(&moves, 0, |s| matches!(s, Step::Mature { .. })).unwrap();
        assert_eq!(mature.step, Step::Mature { units: 4 });
        assert_eq!((mature.before.gtd, mature.after.gtd), (0, 1));
        // The holding move is on top of that: from six mature, four more to ten.
        let hold = find(&moves, 0, |s| matches!(s, Step::Hold { .. })).unwrap();
        assert_eq!(hold.step, Step::Hold { held: 6, more: 4 });
        assert_eq!(hold.before.gtd, 1);
    }

    #[test]
    fn a_wallet_not_ripe_by_the_snapshot_has_no_maturing_move() {
        let linked = linked(vec![wallet(2, 0, 4)]);
        let horizon = Horizon {
            ripe: vec![false],
            ..Horizon::default()
        };
        let moves = moves(&linked, policy(), &horizon);
        assert!(find(&moves, 0, |s| matches!(s, Step::Mature { .. })).is_none());
    }

    #[test]
    fn clout_takes_the_merged_line_when_the_merged_holder_is_the_reading() {
        // Two wallets of 3,000 and 3,500 clout: apart neither reaches 6,000; merged, 6,500 does.
        let mut a = wallet(6, 0, 0);
        a.clout = 3_000;
        let mut b = wallet(6, 0, 0);
        b.clout = 3_500;
        let linked = linked(vec![a, b]);
        let horizon = Horizon {
            fresh: Some(Maturity::Mature),
            ripe: vec![true, true],
            listed: vec![0, 0],
        };
        let moves = moves(&linked, policy(), &horizon);
        let clout = find(&moves, 1, |s| matches!(s, Step::Clout { .. })).unwrap();
        // Wallet b to 6,000 on its own (2,500) only matches the merged reading's one first-come
        // mint, so it adds nothing. The merged holder's next line, 12,000, is the move.
        assert_eq!(
            clout.step,
            Step::Clout {
                points: 6_500,
                at: 12_000
            }
        );
        assert_eq!(
            (clout.before, clout.after.fcfs),
            (Slots { gtd: 2, fcfs: 1 }, 2)
        );
    }

    #[test]
    fn the_token_needs_no_perp_and_climbs_to_its_next_step() {
        let coin = Counts {
            token: 2_000_000,
            ..Counts::default()
        };
        let linked = linked(vec![coin]);
        let moves = moves(&linked, policy(), &Horizon::default());
        let token = find(&moves, 0, |s| matches!(s, Step::Token { .. })).unwrap();
        assert_eq!(
            token.step,
            Step::Token {
                units: 2_000_000,
                at: 3_000_000
            }
        );
        assert_eq!(token.gtd(), 1);
        // Holding: the first Perp opens the gate but the ladder's first rung is worth nothing
        // here; five are the first guaranteed mint.
        let hold = find(&moves, 0, |s| matches!(s, Step::Hold { .. })).unwrap();
        assert_eq!(hold.step, Step::Hold { held: 0, more: 10 });
    }

    #[test]
    fn past_the_last_token_step_the_tail_is_the_next() {
        let rule = policy().token.unwrap();
        assert_eq!(next_token_step(&rule, 12_000_000), Some(20_000_000));
        assert_eq!(next_token_step(&rule, 20_000_000), Some(30_000_000));
        assert_eq!(next_token_step(&rule, 0), Some(3_000_000));
    }

    #[test]
    fn unlisting_brings_perps_back_as_fresh_ones() {
        let linked = linked(vec![wallet(9, 0, 0)]);
        let horizon = Horizon {
            fresh: Some(Maturity::New),
            ripe: vec![true],
            listed: vec![2],
        };
        let unlist = find(&moves(&linked, policy(), &horizon), 0, |s| {
            matches!(s, Step::Unlist { .. })
        })
        .unwrap();
        // 9 + 2 × 0.5 = 10: the next rung.
        assert_eq!(unlist.step, Step::Unlist { units: 2 });
        assert_eq!(unlist.gtd(), 1);
    }

    #[test]
    fn a_small_wallet_reaching_the_stake_trades_first_come_for_guaranteed() {
        // Under five Perps, the holding rung is first come; at five it is guaranteed.
        let policy = Allocation {
            holding: Rungs::new(&[rung(1, 1), rung(5, 2)]).unwrap(),
            ..policy()
        };
        let linked = linked(vec![wallet(4, 0, 0)]);
        let horizon = Horizon {
            fresh: Some(Maturity::Mature),
            ripe: vec![true],
            listed: vec![0],
        };
        let hold = find(&moves(&linked, policy, &horizon), 0, |s| {
            matches!(s, Step::Hold { .. })
        })
        .unwrap();
        assert_eq!(hold.step, Step::Hold { held: 4, more: 1 });
        assert_eq!((hold.gtd(), hold.fcfs_traded()), (2, 1));
    }
}
