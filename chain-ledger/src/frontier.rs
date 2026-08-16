//! The expanding frontier — which parties a project walk watches, and why.
//!
//! You do not know a project's ops wallets in advance; you learn them because
//! the treasury paid them. So the watch set is *seeded* (declared wallets, the
//! policy signer, the CIP-27 royalty address) and then *grows during the walk*:
//! any receipt from a member that expands promotes the receiver.
//!
//! What keeps that from eating the chain is the **terminal rule**: some parties
//! are recorded but never expanded — a stakeless (enterprise / off-ramp shaped)
//! address, a party the operator has declared custodial, or one whose observed
//! activity is custodial-scale. Expanding one of those reaches an exchange in
//! two hops. Seeded roles are exempt: a mint treasury takes thousands of receipts
//! from thousands of buyers and would otherwise trip the proxy that exists to
//! catch exchanges.
//!
//! Two properties this module guarantees, because the walker leans on them:
//!
//! - **Determinism.** The state after a sequence of [`Frontier::on_movement`] /
//!   [`Frontier::reevaluate`] calls depends only on that sequence. A walk that
//!   is checkpointed and resumed must arrive at the same frontier as one that
//!   ran straight through — otherwise a resume can expand a wallet the original
//!   would have frozen. Iteration order is `BTreeMap`, counters are exact.
//! - **Freeze, never revoke.** A member that trips a threshold after it has
//!   already promoted others stops expanding from that slot on; the parties it
//!   promoted are kept and flagged [`Member::promoted_via_terminal`] so a reader
//!   sees the edge is suspect. Demotion is a user action. Automatic revocation
//!   would make the frontier non-monotone and the walk non-resumable.
//!
//! [`Member::watched_from_slot`] is not optional: a party promoted at slot N had
//! its earlier history walked past. Its coverage is `[watched_from_slot, cursor]`
//! and a UI must say so; [`Frontier::backfilled`] is the only thing that lowers
//! it. This module is I/O-free — the walker feeds it movements and a global
//! activity count and persists it however it likes (it serialises).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::model::{Movement, Party};

/// Why a party is in the watch set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Named in the registry — an assertion the chain cannot make ("treasury").
    Declared,
    /// The policy's `sig` credential, observed in the mint script.
    Signer,
    /// The CIP-27 `777.addr`, observed in the on-mint metadata.
    Royalty,
    /// Reached by a receipt from an expanding member.
    Promoted,
}

impl Role {
    /// Seeded roles never terminate — see the module doc for why.
    pub fn is_exempt(self) -> bool {
        !matches!(self, Role::Promoted)
    }
}

/// Why a member is recorded but not expanded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalReason {
    /// No staking credential — the shape an off-ramp takes.
    Stakeless,
    /// Receipts observed at or above [`Thresholds::receipts`].
    Receipts,
    /// Distinct inbound counterparties at or above [`Thresholds::counterparties`].
    Counterparties,
    /// Listed in the registry's terminal set (asserted; carries a source there).
    Declared,
}

/// Custodial-scale proxies, computed from the stream. Tunable per case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thresholds {
    pub receipts: u32,
    pub counterparties: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            receipts: 1_000,
            counterparties: 300,
        }
    }
}

/// One watched party.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub party: Party,
    pub role: Role,
    /// Earliest slot from which this party's flows are fully covered.
    pub watched_from_slot: u64,
    pub promoted_by: Option<Party>,
    pub promoted_tx: Option<String>,
    /// Whether receipts from this party promote the receiver.
    pub expand: bool,
    pub terminal_reason: Option<TerminalReason>,
    /// Slot at which an expanding member was frozen (`None` if it never
    /// expanded, or still does).
    pub frozen_at_slot: Option<u64>,
    /// Promoted by a member that was later frozen — the edge is suspect.
    pub promoted_via_terminal: bool,
    /// Inbound receipts observed while watched.
    pub receipts: u32,
    /// Distinct inbound counterparties observed while watched (party keys).
    pub counterparties: BTreeSet<String>,
}

impl Member {
    fn new(party: Party, role: Role, slot: u64) -> Self {
        Self {
            party,
            role,
            watched_from_slot: slot,
            promoted_by: None,
            promoted_tx: None,
            expand: true,
            terminal_reason: None,
            frozen_at_slot: None,
            promoted_via_terminal: false,
            receipts: 0,
            counterparties: BTreeSet::new(),
        }
    }

    pub fn is_terminal(&self) -> bool {
        !self.expand
    }
}

/// What a movement did to the frontier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Neither side is a member (the walker should not normally send these).
    Ignored,
    /// The receiver was already a member; its counters advanced.
    Counted,
    /// The receiver joined. `terminal` is set when it joined already frozen.
    Promoted { terminal: Option<TerminalReason> },
    /// The receiver was an expanding member and this receipt tipped it over.
    Frozen(TerminalReason),
}

/// The watch set. See the module doc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frontier {
    thresholds: Thresholds,
    /// Registry-declared terminals: recorded on contact, never expanded.
    declared_terminal: BTreeSet<Party>,
    /// Keyed by party in memory; serialised as a list so the checkpoint blob is
    /// readable in JSON as well as postcard (map keys must be strings there).
    #[serde(with = "members_as_list")]
    members: BTreeMap<Party, Member>,
}

mod members_as_list {
    use super::{Member, Party};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    pub fn serialize<S: Serializer>(m: &BTreeMap<Party, Member>, s: S) -> Result<S::Ok, S::Error> {
        m.values().collect::<Vec<_>>().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<BTreeMap<Party, Member>, D::Error> {
        let v: Vec<Member> = Vec::deserialize(d)?;
        Ok(v.into_iter().map(|m| (m.party.clone(), m)).collect())
    }
}

/// Seeding a party the registry also lists as terminal is a contradiction the
/// registry should have caught; refuse rather than pick a winner silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedConflict(pub Party);

impl std::fmt::Display for SeedConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is both a seed and a declared terminal", self.0.key)
    }
}

impl std::error::Error for SeedConflict {}

impl Frontier {
    pub fn new(thresholds: Thresholds, declared_terminal: impl IntoIterator<Item = Party>) -> Self {
        Self {
            thresholds,
            declared_terminal: declared_terminal.into_iter().collect(),
            members: BTreeMap::new(),
        }
    }

    pub fn thresholds(&self) -> Thresholds {
        self.thresholds
    }

    /// Add a seed. Idempotent on the party (an earlier seed keeps its role and
    /// the lower slot). Seeds always expand.
    pub fn seed(&mut self, party: Party, role: Role, slot: u64) -> Result<&Member, SeedConflict> {
        if self.declared_terminal.contains(&party) {
            return Err(SeedConflict(party));
        }
        let key = party.clone();
        let m = self
            .members
            .entry(key.clone())
            .or_insert_with(|| Member::new(party, role, slot));
        if m.watched_from_slot > slot {
            m.watched_from_slot = slot;
        }
        Ok(&self.members[&key])
    }

    pub fn is_member(&self, p: &Party) -> bool {
        self.members.contains_key(p)
    }

    /// Whether receipts from `p` promote the receiver.
    pub fn expands(&self, p: &Party) -> bool {
        self.members.get(p).is_some_and(|m| m.expand)
    }

    pub fn member(&self, p: &Party) -> Option<&Member> {
        self.members.get(p)
    }

    pub fn members(&self) -> impl Iterator<Item = &Member> {
        self.members.values()
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Feed one attributed movement at `slot`. `global_receipts_of_to` is the
    /// walker's activity count for the receiver over the whole walk window
    /// (including before it was watched) if it keeps one; `None` falls back to
    /// the receipts observed while watched.
    ///
    /// Semantics: if `from` expands and `to` is not a member, `to` is promoted
    /// (possibly already terminal). If `to` is a member, its counters advance
    /// and the thresholds are checked. Called for every movement in any tx that
    /// touches a member — receipts from non-members still count.
    pub fn on_movement(
        &mut self,
        mv: &Movement,
        slot: u64,
        global_receipts_of_to: Option<u32>,
    ) -> Outcome {
        let from_expands = self.expands(&mv.from);
        if !self.members.contains_key(&mv.to) {
            if !from_expands {
                return Outcome::Ignored;
            }
            let terminal = self.terminal_at_promotion(&mv.to, global_receipts_of_to);
            let mut m = Member::new(mv.to.clone(), Role::Promoted, slot);
            m.promoted_by = Some(mv.from.clone());
            m.promoted_tx = Some(mv.tx_id.clone());
            m.receipts = 1;
            m.counterparties.insert(mv.from.key.clone());
            if let Some(reason) = terminal {
                m.expand = false;
                m.terminal_reason = Some(reason);
            }
            self.members.insert(mv.to.clone(), m);
            return Outcome::Promoted { terminal };
        }

        let thresholds = self.thresholds;
        let tripped = {
            let m = self.members.get_mut(&mv.to).expect("checked above");
            m.receipts = m.receipts.saturating_add(1);
            m.counterparties.insert(mv.from.key.clone());
            let receipts = global_receipts_of_to.map_or(m.receipts, |g| g.max(m.receipts));
            if m.expand && !m.role.is_exempt() {
                check(thresholds, receipts, m.counterparties.len())
            } else {
                None
            }
        };
        match tripped {
            Some(reason) => {
                self.freeze(&mv.to, reason, slot);
                Outcome::Frozen(reason)
            }
            None => Outcome::Counted,
        }
    }

    /// Re-check every expanding, non-exempt member against the thresholds using
    /// the walker's global activity counts. Call at each checkpoint so a party
    /// promoted while the counter was cold is still frozen once its scale shows.
    /// Returns what was frozen, in party order.
    pub fn reevaluate(
        &mut self,
        slot: u64,
        global_receipts: &dyn Fn(&Party) -> Option<u32>,
    ) -> Vec<(Party, TerminalReason)> {
        let candidates: Vec<(Party, TerminalReason)> = self
            .members
            .values()
            .filter(|m| m.expand && !m.role.is_exempt())
            .filter_map(|m| {
                let receipts = global_receipts(&m.party).map_or(m.receipts, |g| g.max(m.receipts));
                check(self.thresholds, receipts, m.counterparties.len())
                    .map(|r| (m.party.clone(), r))
            })
            .collect();
        for (p, r) in &candidates {
            self.freeze(p, *r, slot);
        }
        candidates
    }

    /// Record that `p` has been backfilled to `floor`: its coverage now starts
    /// there. Never raises the slot.
    pub fn backfilled(&mut self, p: &Party, floor: u64) -> bool {
        match self.members.get_mut(p) {
            Some(m) => {
                if floor < m.watched_from_slot {
                    m.watched_from_slot = floor;
                }
                true
            }
            None => false,
        }
    }

    /// Operator override: let a frozen member expand again (e.g. a false
    /// terminal on a genuine team wallet). Clears the reason; the flag on the
    /// parties it promoted earlier is left as-is because their edge really was
    /// made while it was considered suspect.
    pub fn force_expand(&mut self, p: &Party) -> bool {
        match self.members.get_mut(p) {
            Some(m) => {
                m.expand = true;
                m.terminal_reason = None;
                m.frozen_at_slot = None;
                true
            }
            None => false,
        }
    }

    fn terminal_at_promotion(
        &self,
        p: &Party,
        global_receipts: Option<u32>,
    ) -> Option<TerminalReason> {
        if p.is_stakeless() {
            return Some(TerminalReason::Stakeless);
        }
        if self.declared_terminal.contains(p) {
            return Some(TerminalReason::Declared);
        }
        // One receipt, one counterparty at promotion — only the global count
        // can already be over.
        check(self.thresholds, global_receipts.unwrap_or(1).max(1), 1)
    }

    fn freeze(&mut self, p: &Party, reason: TerminalReason, slot: u64) {
        if let Some(m) = self.members.get_mut(p) {
            m.expand = false;
            m.terminal_reason = Some(reason);
            m.frozen_at_slot = Some(slot);
        }
        for m in self.members.values_mut() {
            if m.promoted_by.as_ref() == Some(p) {
                m.promoted_via_terminal = true;
            }
        }
    }
}

/// The threshold test, shared by promotion, per-receipt and checkpoint checks
/// so the three can never disagree.
fn check(t: Thresholds, receipts: u32, counterparties: usize) -> Option<TerminalReason> {
    if receipts >= t.receipts {
        Some(TerminalReason::Receipts)
    } else if counterparties as u32 >= t.counterparties {
        Some(TerminalReason::Counterparties)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stake(k: &str) -> Party {
        Party::cardano_stake(k)
    }

    fn mv(from: &Party, to: &Party, tx: &str) -> Movement {
        Movement {
            tx_id: tx.into(),
            timestamp: 0,
            from: from.clone(),
            to: to.clone(),
            value: 1_000_000,
        }
    }

    fn frontier() -> Frontier {
        Frontier::new(
            Thresholds {
                receipts: 5,
                counterparties: 3,
            },
            [],
        )
    }

    #[test]
    fn receipt_from_expanding_member_promotes_receiver() {
        let mut f = frontier();
        let t = stake("treasury");
        let ops = stake("ops");
        f.seed(t.clone(), Role::Declared, 100).unwrap();
        assert_eq!(
            f.on_movement(&mv(&t, &ops, "tx1"), 150, None),
            Outcome::Promoted { terminal: None }
        );
        let m = f.member(&ops).unwrap();
        assert_eq!(m.role, Role::Promoted);
        assert_eq!(m.watched_from_slot, 150);
        assert_eq!(m.promoted_by.as_ref(), Some(&t));
        assert_eq!(m.promoted_tx.as_deref(), Some("tx1"));
        assert!(m.expand);
    }

    #[test]
    fn receipt_from_non_member_or_terminal_does_not_promote() {
        let mut f = frontier();
        let t = stake("treasury");
        let stranger = stake("stranger");
        let x = stake("x");
        f.seed(t.clone(), Role::Declared, 0).unwrap();
        assert_eq!(
            f.on_movement(&mv(&stranger, &x, "a"), 1, None),
            Outcome::Ignored
        );
        // Promote a stakeless off-ramp: recorded, terminal.
        let ramp = Party::cardano_enterprise("addr1v…");
        assert_eq!(
            f.on_movement(&mv(&t, &ramp, "b"), 2, None),
            Outcome::Promoted {
                terminal: Some(TerminalReason::Stakeless)
            }
        );
        // The off-ramp's own payments reach nobody.
        assert_eq!(
            f.on_movement(&mv(&ramp, &x, "c"), 3, None),
            Outcome::Ignored
        );
        assert!(!f.is_member(&x));
    }

    #[test]
    fn declared_terminal_is_recorded_not_expanded_and_cannot_be_seeded() {
        let cex = stake("cex");
        let mut f = Frontier::new(Thresholds::default(), [cex.clone()]);
        let t = stake("treasury");
        f.seed(t.clone(), Role::Declared, 0).unwrap();
        assert_eq!(
            f.on_movement(&mv(&t, &cex, "a"), 1, None),
            Outcome::Promoted {
                terminal: Some(TerminalReason::Declared)
            }
        );
        assert!(!f.expands(&cex));
        assert_eq!(
            f.seed(cex.clone(), Role::Declared, 0),
            Err(SeedConflict(cex))
        );
    }

    #[test]
    fn threshold_freezes_and_flags_earlier_promotions() {
        let mut f = frontier();
        let t = stake("treasury");
        let hub = stake("hub");
        let child = stake("child");
        f.seed(t.clone(), Role::Declared, 0).unwrap();
        f.on_movement(&mv(&t, &hub, "p"), 10, None);
        // hub promotes child while still expanding.
        f.on_movement(&mv(&hub, &child, "q"), 11, None);
        assert!(f.is_member(&child));
        assert!(!f.member(&child).unwrap().promoted_via_terminal);
        // Three distinct inbound counterparties trips `counterparties: 3`.
        f.on_movement(&mv(&stake("a"), &hub, "r"), 12, None);
        let out = f.on_movement(&mv(&stake("b"), &hub, "s"), 13, None);
        assert_eq!(out, Outcome::Frozen(TerminalReason::Counterparties));
        let h = f.member(&hub).unwrap();
        assert!(!h.expand);
        assert_eq!(h.frozen_at_slot, Some(13));
        assert!(f.member(&child).unwrap().promoted_via_terminal);
        // Frozen: further receipts from hub promote nobody, and child stays.
        assert_eq!(
            f.on_movement(&mv(&hub, &stake("z"), "t"), 14, None),
            Outcome::Ignored
        );
        assert!(f.is_member(&child));
    }

    #[test]
    fn exempt_roles_never_freeze() {
        let mut f = frontier();
        let t = stake("treasury");
        f.seed(t.clone(), Role::Declared, 0).unwrap();
        for i in 0..50 {
            let buyer = stake(&format!("buyer{i}"));
            assert_eq!(
                f.on_movement(&mv(&buyer, &t, &format!("tx{i}")), i, Some(10_000)),
                Outcome::Counted
            );
        }
        assert!(f.expands(&t));
        assert!(f.reevaluate(100, &|_| Some(10_000)).is_empty());
    }

    #[test]
    fn global_count_terminates_at_promotion_and_on_reevaluate() {
        let mut f = frontier();
        let t = stake("treasury");
        let busy = stake("busy");
        let quiet = stake("quiet");
        f.seed(t.clone(), Role::Declared, 0).unwrap();
        // Known busy at promotion.
        assert_eq!(
            f.on_movement(&mv(&t, &busy, "a"), 1, Some(5)),
            Outcome::Promoted {
                terminal: Some(TerminalReason::Receipts)
            }
        );
        // Quiet at promotion, cold counter; later the counter shows scale.
        assert_eq!(
            f.on_movement(&mv(&t, &quiet, "b"), 2, Some(1)),
            Outcome::Promoted { terminal: None }
        );
        let frozen = f.reevaluate(50, &|p| (p == &quiet).then_some(999));
        assert_eq!(frozen, vec![(quiet.clone(), TerminalReason::Receipts)]);
        assert_eq!(f.member(&quiet).unwrap().frozen_at_slot, Some(50));
    }

    #[test]
    fn backfill_lowers_coverage_and_force_expand_reopens() {
        let mut f = frontier();
        let t = stake("treasury");
        let ops = stake("ops");
        f.seed(t.clone(), Role::Declared, 100).unwrap();
        f.on_movement(&mv(&t, &ops, "a"), 500, None);
        assert!(f.backfilled(&ops, 100));
        assert_eq!(f.member(&ops).unwrap().watched_from_slot, 100);
        assert!(f.backfilled(&ops, 200)); // never raises
        assert_eq!(f.member(&ops).unwrap().watched_from_slot, 100);
        assert!(!f.backfilled(&stake("nobody"), 0));

        f.reevaluate(600, &|p| (p == &ops).then_some(999));
        assert!(!f.expands(&ops));
        assert!(f.force_expand(&ops));
        assert!(f.expands(&ops));
        assert_eq!(f.member(&ops).unwrap().terminal_reason, None);
    }

    #[test]
    fn seed_is_idempotent_and_keeps_lowest_slot() {
        let mut f = frontier();
        let t = stake("treasury");
        f.seed(t.clone(), Role::Declared, 100).unwrap();
        f.seed(t.clone(), Role::Signer, 50).unwrap();
        let m = f.member(&t).unwrap();
        assert_eq!(m.role, Role::Declared);
        assert_eq!(m.watched_from_slot, 50);
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn deterministic_and_serialisable() {
        let script = |f: &mut Frontier| {
            let t = stake("treasury");
            f.seed(t.clone(), Role::Declared, 0).unwrap();
            for i in 0..10u64 {
                let p = stake(&format!("p{i}"));
                f.on_movement(&mv(&t, &p, &format!("t{i}")), i, Some(i as u32));
                f.on_movement(
                    &mv(&p, &stake(&format!("q{i}")), &format!("u{i}")),
                    i + 1,
                    None,
                );
            }
            f.reevaluate(20, &|_| Some(4));
        };
        let mut a = frontier();
        let mut b = frontier();
        script(&mut a);
        script(&mut b);
        assert_eq!(a, b);
        let json = serde_json::to_string(&a).unwrap();
        let back: Frontier = serde_json::from_str(&json).unwrap();
        assert_eq!(a, back);
    }
}
