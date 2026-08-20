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
    /// A destination of mint proceeds, observed in the mint transaction's own
    /// fund split.
    ///
    /// Seeded rather than promoted, because the frontier only ever grows along
    /// OUTBOUND edges from a member: the payer in a mint is the buyer, a
    /// stranger, so no promotion path reaches the wallets the money lands in.
    /// Left unseated they have no row, and a UI cannot draw the project's own
    /// capital coming IN — which is the face the tool exists for.
    ///
    /// Deliberately NOT exempt from the terminal rule, unlike the other seeded
    /// roles. A mint's payees include the minting platform's fee wallet, which
    /// is custodial-scale; seeding those as exempt took the Mekka frontier from
    /// 64 parties to 2,826. Not exempt means an artist wallet is watched and
    /// expands, while a platform wallet is recorded — seated, drawable — and
    /// frozen the moment its scale shows.
    MintPayee,
}

impl Role {
    /// Seeded roles never terminate — see the module doc for why.
    ///
    /// [`Role::MintPayee`] is the exception: it is seeded (nothing can promote
    /// it) but must still be freezable, or one custodial payee recruits the
    /// whole chain.
    pub fn is_exempt(self) -> bool {
        !matches!(self, Role::Promoted | Role::MintPayee)
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
    /// Sustained outbound fan-out — new wallets paid, at
    /// [`Thresholds::payees_per_window`] for [`Thresholds::payee_hot_windows`]
    /// windows. The shape an exchange hot wallet or payout service takes, and
    /// the only one no INBOUND measure can see.
    Payees,
}

/// Custodial-scale proxies, computed from the stream. Tunable per case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thresholds {
    pub receipts: u32,
    pub counterparties: u32,
    /// NEW distinct wallets paid within one window, above which the window is
    /// "hot" — the shape of an exchange hot wallet or a payout service.
    ///
    /// The other two thresholds measure money ARRIVING, and a hot wallet is
    /// invisible to both: customer deposits go to per-user deposit addresses,
    /// never the hot wallet, which is topped up internally from cold storage.
    /// Measured on two real ones — enormous fan-out, almost no fan-in:
    ///
    /// | | paid | distinct recipients | received | distinct senders |
    /// | --- | --- | --- | --- | --- |
    /// | A | 23,724 | **2,390** | 71 | **3** |
    /// | B | 19,230 | **3,081** | 2 | **2** |
    ///
    /// **A RATE, not a total.** A project wallet paying three hundred people
    /// over two years is ordinary; an exchange pays three hundred in a day.
    /// Measured new-payees-per-active-day: exchange B 12.3, `bank.pillar` 11.2,
    /// exchange A 9.4, and an actual project treasury **1.2** with a peak of 3.
    /// Two orders of magnitude of daylight, and nothing in between — which is
    /// why a rate is safe where a total was not.
    ///
    /// A high-fan-out payout service like `bank.pillar` trips this too, and
    /// should: it is not a project wallet either, and expanding it recruits
    /// everyone it ever paid.
    pub payees_per_window: u32,
    /// Window length in slots. One Cardano slot ≈ 1s, so 86,400 ≈ a day.
    pub payee_window_slots: u64,
    /// How many HOT windows before freezing.
    ///
    /// The reason this is not 1: an airdrop is a legitimate one-off burst, and
    /// `bank.pillar` peaked at **1,056** new payees in a single day against a
    /// steady 42–92 for the exchanges. Requiring several windows means a
    /// project that airdrops once keeps expanding, while a wallet that does it
    /// week in week out does not. Windows need not be consecutive — a service
    /// with quiet days is still a service.
    pub payee_hot_windows: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            receipts: 1_000,
            counterparties: 300,
            // ~4x the busiest day a real project treasury had, and at or below
            // the AVERAGE day of all three services measured.
            payees_per_window: 10,
            payee_window_slots: 86_400,
            payee_hot_windows: 3,
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
    /// Distinct wallets this party has PAID while watched — needed only to tell
    /// a NEW payee from a repeat one. See [`Thresholds::payees_per_window`].
    ///
    /// Stops growing once the party is frozen: the set exists to answer "is it
    /// still reaching new wallets", and after freezing the answer cannot change
    /// anything, so accumulating further is unbounded memory for no
    /// information.
    #[serde(default)]
    pub payees: BTreeSet<String>,
    /// Start slot of the fan-out window currently being counted.
    #[serde(default)]
    pub payee_window_start: u64,
    /// New distinct payees seen so far in that window.
    #[serde(default)]
    pub payee_window_new: u32,
    /// How many windows have been hot. Freezes at
    /// [`Thresholds::payee_hot_windows`].
    #[serde(default)]
    pub payee_hot_windows: u32,
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
            payees: BTreeSet::new(),
            payee_window_start: slot,
            payee_window_new: 0,
            payee_hot_windows: 0,
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

    /// Seat a declared-terminal party FROM THE START of the walk: recorded,
    /// never expanding.
    ///
    /// `declared_terminal` alone is not enough, because it only governs what
    /// happens once a party is already a member — and [`Frontier::on_movement`]
    /// records the RECEIVER of a movement, never the sender. A wallet that only
    /// ever PAYS the watch set is therefore never seated by contact, no matter
    /// how much it pays or how often.
    ///
    /// Measured: two wallets funding a treasury with 38,870 ₳ across five
    /// synchronised payments. One was seated only by accident, a third of the
    /// way into the walk, because it happened to receive from a member once.
    /// The other was never seated at all, so nothing about it was recorded —
    /// including who funded IT, which was the entire question.
    ///
    /// Seeding it as an ordinary wallet is not the alternative: that makes it
    /// expand, and two active wallets took the same frontier from 180 parties
    /// to 6,424. This gives coverage without recruitment — the party is watched
    /// from `slot`, its flows are booked, and it promotes nobody.
    pub fn seed_terminal(&mut self, party: Party, slot: u64) -> &Member {
        let key = party.clone();
        let m = self.members.entry(key.clone()).or_insert_with(|| {
            let mut m = Member::new(party, Role::Declared, slot);
            m.expand = false;
            m.terminal_reason = Some(TerminalReason::Declared);
            m
        });
        // Idempotent, and never RAISES the coverage floor: re-seeding must not
        // silently narrow the window a reader has already been shown.
        if m.watched_from_slot > slot {
            m.watched_from_slot = slot;
        }
        &self.members[&key]
    }

    /// Record that `from` paid `to`, and report a freeze if its fan-out RATE
    /// has been sustained for [`Thresholds::payee_hot_windows`] windows.
    ///
    /// Only novelty counts. A wallet paying the same fifty contributors every
    /// week has a payee set that SATURATES; an exchange's never does, because
    /// its customers are always new. That difference is the signal, and it is
    /// why the window counts NEW payees rather than payments.
    fn note_payee(&mut self, from: &Party, to: &Party, slot: u64) -> Option<TerminalReason> {
        let t = self.thresholds;
        let m = self.members.get_mut(from)?;
        // Nothing to learn from a party that already cannot expand.
        if !m.expand {
            return None;
        }
        // Close out any windows that have elapsed. A window is hot if it met
        // the rate; quiet windows do NOT reset the count, because a service
        // with idle days is still a service — they simply do not add evidence.
        while slot >= m.payee_window_start.saturating_add(t.payee_window_slots) {
            if m.payee_window_new >= t.payees_per_window {
                m.payee_hot_windows = m.payee_hot_windows.saturating_add(1);
            }
            m.payee_window_start = m.payee_window_start.saturating_add(t.payee_window_slots);
            m.payee_window_new = 0;
        }
        if m.payees.insert(to.key.clone()) {
            m.payee_window_new = m.payee_window_new.saturating_add(1);
        }
        // The window in progress counts toward the verdict as soon as it meets
        // the rate — waiting for it to close would let a wallet recruit for a
        // whole extra window after the evidence was already in.
        let hot = m.payee_hot_windows + u32::from(m.payee_window_new >= t.payees_per_window);
        (hot >= t.payee_hot_windows).then_some(TerminalReason::Payees)
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
        // SENDER-SIDE FAN-OUT, before anything else — the only place the
        // frontier looks at `from` as a subject rather than a source. An
        // exchange hot wallet is invisible to every inbound measure, so if this
        // is not counted here it is not counted anywhere.
        if let Some(reason) = self.note_payee(&mv.from, &mv.to, slot) {
            self.freeze(&mv.from, reason, slot);
        }

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
                // Deliberately unreachable in the tests that are not ABOUT
                // fan-out, so adding this rule cannot silently change what they
                // were asserting.
                payees_per_window: u32::MAX,
                payee_window_slots: 86_400,
                payee_hot_windows: u32::MAX,
            },
            [],
        )
    }

    /// A frontier tuned to catch fan-out quickly, for the tests that are about
    /// exactly that: 2 new payees in a window makes it hot, 2 hot windows freeze.
    fn fanout_frontier() -> Frontier {
        Frontier::new(
            Thresholds {
                receipts: u32::MAX,
                counterparties: u32::MAX,
                payees_per_window: 2,
                payee_window_slots: 100,
                payee_hot_windows: 2,
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

    /// A mint payee is seeded because nothing can promote it — the payer is the
    /// buyer, a stranger — but unlike the other seeded roles it must stay
    /// freezable, or the minting platform's fee wallet recruits the chain.
    #[test]
    fn mint_payee_is_seated_but_freezes_at_custodial_scale() {
        let mut f = frontier();
        let artist = stake("artist");
        let platform = stake("platform-fee");
        f.seed(artist.clone(), Role::MintPayee, 100).unwrap();
        f.seed(platform.clone(), Role::MintPayee, 100).unwrap();

        // Seated, and therefore drawable, from the moment the mint is seen.
        assert!(f.is_member(&artist));
        assert!(f.is_member(&platform));
        assert!(f.expands(&artist));

        // NOT exempt: enough distinct counterparties and it freezes.
        for (i, payer) in ["b1", "b2", "b3", "b4"].iter().enumerate() {
            let out = f.on_movement(&mv(&stake(payer), &platform, &format!("tx{i}")), 200, None);
            if let Outcome::Frozen(reason) = out {
                assert_eq!(reason, TerminalReason::Counterparties);
            }
        }
        assert!(
            !f.expands(&platform),
            "custodial payee must stop recruiting"
        );
        // Still a member — recorded and drawable, just not expanding.
        assert!(f.is_member(&platform));
        // The quiet payee is untouched and still expands.
        assert!(f.expands(&artist));
    }

    /// An exchange hot wallet is invisible to every INBOUND measure — deposits
    /// go to per-user addresses, so it receives from almost nobody while paying
    /// thousands. Sustained fan-out is the only signal that identifies it.
    #[test]
    fn sustained_fan_out_freezes_a_hot_wallet() {
        let mut f = fanout_frontier();
        let hot = stake("exchange-hot-wallet");
        f.seed(hot.clone(), Role::Declared, 0).unwrap();

        // Window 1 (slots 0–99): two new payees — hot, but one window is not
        // evidence.
        f.on_movement(&mv(&hot, &stake("cust1"), "t1"), 10, None);
        f.on_movement(&mv(&hot, &stake("cust2"), "t2"), 20, None);
        assert!(f.expands(&hot), "one hot window must not be enough");

        // Window 2: two more NEW payees — sustained, so it freezes.
        f.on_movement(&mv(&hot, &stake("cust3"), "t3"), 110, None);
        f.on_movement(&mv(&hot, &stake("cust4"), "t4"), 120, None);
        assert!(!f.expands(&hot), "sustained fan-out freezes");
        assert_eq!(
            f.member(&hot).unwrap().terminal_reason,
            Some(TerminalReason::Payees)
        );
        // Recorded, not removed — freezing costs reach, never data.
        assert!(f.is_member(&hot));
    }

    /// A ONE-OFF airdrop is legitimate. `bank.pillar` peaked at 1,056 new
    /// payees in a single day against a steady 42–92 for the exchanges, so a
    /// single burst must not be treated as custodial behaviour.
    #[test]
    fn a_single_burst_does_not_freeze() {
        let mut f = fanout_frontier();
        let airdropper = stake("airdrop-wallet");
        f.seed(airdropper.clone(), Role::Declared, 0).unwrap();
        for i in 0..50 {
            f.on_movement(
                &mv(&airdropper, &stake(&format!("holder{i}")), "t"),
                10,
                None,
            );
        }
        assert!(f.expands(&airdropper), "one burst is not a pattern");
    }

    /// Only NOVELTY counts. A wallet paying the same people repeatedly has a
    /// payee set that saturates; an exchange's never does.
    #[test]
    fn repeat_payments_to_the_same_wallets_never_freeze() {
        let mut f = fanout_frontier();
        let payroll = stake("payroll");
        f.seed(payroll.clone(), Role::Declared, 0).unwrap();
        for w in 0..20 {
            for who in ["alice", "bob"] {
                f.on_movement(&mv(&payroll, &stake(who), "t"), w * 100 + 10, None);
            }
        }
        assert!(
            f.expands(&payroll),
            "the same two payees forever is not fan-out"
        );
    }

    /// A wallet that only ever PAYS the watch set is never seated by contact,
    /// because `on_movement` records the receiver. `seed_terminal` is the only
    /// way it becomes visible — and it must not thereby start recruiting.
    #[test]
    fn a_terminal_seed_is_watched_from_the_floor_and_promotes_nobody() {
        let mut f = frontier();
        let funder = stake("pays-us-and-never-receives");
        let stranger = stake("stranger");
        f.seed_terminal(funder.clone(), 100);

        let m = f.member(&funder).expect("seated at seed time");
        assert_eq!(
            m.watched_from_slot, 100,
            "covered from the floor, not on contact"
        );
        assert!(m.is_terminal());
        assert_eq!(m.terminal_reason, Some(TerminalReason::Declared));
        assert!(!f.expands(&funder), "recorded, never expanding");

        // Its own payments must NOT recruit — that is what took a real frontier
        // from 180 parties to 6,424.
        assert_eq!(
            f.on_movement(&mv(&funder, &stranger, "tx1"), 150, None),
            Outcome::Ignored
        );
        assert!(!f.is_member(&stranger));
    }

    /// Re-seeding must not narrow coverage a reader has already been shown.
    #[test]
    fn re_seeding_a_terminal_keeps_the_earliest_slot() {
        let mut f = frontier();
        let p = stake("funder");
        f.seed_terminal(p.clone(), 500);
        f.seed_terminal(p.clone(), 900);
        assert_eq!(f.member(&p).unwrap().watched_from_slot, 500);
        f.seed_terminal(p.clone(), 100);
        assert_eq!(f.member(&p).unwrap().watched_from_slot, 100);
    }

    /// The other seeded roles keep their exemption — this is a carve-out for
    /// `MintPayee`, not a change of policy for `Declared`/`Signer`/`Royalty`.
    #[test]
    fn only_promoted_and_mint_payee_are_freezable() {
        assert!(Role::Declared.is_exempt());
        assert!(Role::Signer.is_exempt());
        assert!(Role::Royalty.is_exempt());
        assert!(!Role::Promoted.is_exempt());
        assert!(!Role::MintPayee.is_exempt());
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
