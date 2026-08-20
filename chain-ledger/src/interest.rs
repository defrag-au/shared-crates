//! Interest — attention-directed scoring vocabulary.
//!
//! Design: `cnft.dev-workers/docs/design/PROJECT_LEDGER_INTEREST.md`. The one
//! rule that governs everything here: **interest directs attention; it never
//! asserts facts.** A score is a heuristic and is therefore banned from
//! exported figures; what makes it defensible anyway is that it is always
//! decomposable into named signals, each carrying its own [`Basis`].
//!
//! This module is the shared vocabulary and the combination arithmetic —
//! pure, deterministic, no I/O. Extraction (the SQL that decides which
//! signals fired) lives in `project-ledger score`; display lives in the app.
//! Same split as [`crate::frontier`]: the state machine is here, the walker
//! feeds it.
//!
//! ## The invariant the evidence panel leans on
//!
//! Stored signal rows for a subject **sum exactly to its score** — including
//! the magnitude multiplier, which is stored as the delta it contributed
//! rather than as a factor. "Why is this interesting?" is then answered by
//! rows that visibly add up, and a row hidden from the panel would show up as
//! arithmetic that doesn't.

use serde::{Deserialize, Serialize};

use crate::model::Basis;

/// One reason a transaction or party is (un)interesting.
///
/// A single vocabulary for both subjects: several signals are meaningful on
/// each side (a tx *is* a fund split; a party *received* a share of one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    /// A mint of the policy — the discovery spine. Common, so modest.
    PolicyMint,
    /// Carries (tx) or received (party) part of the mint's fund split.
    /// Weighted by SHARE of total proceeds, never absolute value: a 44 ₳ leg
    /// once seated a bank.
    MintFundSplit,
    /// Pays the CIP-27 royalty address.
    RoyaltyPayment,
    /// ONE funder → many distinct recipients in a single tx — the
    /// airdrop/rewards shape, detectable with zero metadata. Per-TX fan-out;
    /// deliberately distinct from the per-party fan-out RATE that marks a
    /// service.
    FanoutDistribution,
    /// A CIP-20 (674) message is present; the text rides in `detail`,
    /// verbatim. Corroboration and curator gold, never the detector — the
    /// text is free-form and absence proves nothing.
    TxMessage,
    /// Explained by a marketplace event — an ordinary trade. Suppresses.
    VenueSale,
    /// The non-atomic P2P trade shape: asset one way, value back between the
    /// same two parties within the window. SUPPRESSES interest; never books a
    /// sale — `secondary_sale` stays venue-only.
    CounterPayment,
    /// A DEX/aggregator/lending counterparty — the wallet's own money
    /// returning. Suppresses (was 58,150 ₳ of phantom income).
    RoundTripLeg,
    /// A CEX or bridge on either end — where off-chain legs surface.
    BoundaryCrossing,
    /// Policy asset delivered where the receiver did not pay: no within-tx
    /// funding (sound at the mint, which IS atomic), no venue sale, no
    /// counter-payment in window, receiver not a provider. The
    /// payment-in-kind candidate.
    AssetGrant,
    /// Part of a regular pattern between the same pair — the five
    /// synchronised funding dates.
    Recurrence,
    /// Value into a core-role wallet that nothing explains — not mint, not
    /// sale, not a round trip. The 67,938 ₳ detector.
    UnexplainedInbound,
    /// An asserted-core party funds or receives. The user's rule, verbatim:
    /// everything core touches is interesting. Scaled by the assertion's
    /// [`Confidence`].
    CoreTouch,
    /// A high-interest party participates — general propagation, round ≥ 1.
    /// Damped ×[`Weights::provider_damp`] when the participant is a provider:
    /// services are context, not subjects.
    HotPartyTouch,
    /// The within-ledger scale multiplier, stored as the DELTA it contributed
    /// so rows still sum to the score. Percentile-relative, never absolute ₳.
    Magnitude,
    /// Party-level: carries a human core/associate assertion. Pinning is done
    /// as a large additive weight rather than a special case, so the evidence
    /// panel shows it like everything else.
    CoreAssertion,
    /// Party-level: the accrual from its top-K scoring transactions. Top-K,
    /// not total — a wallet in 10,000 boring txs must not outrank one in
    /// three damning ones.
    TopTransactions,
}

impl Signal {
    pub const ALL: [Self; 17] = [
        Self::PolicyMint,
        Self::MintFundSplit,
        Self::RoyaltyPayment,
        Self::FanoutDistribution,
        Self::TxMessage,
        Self::VenueSale,
        Self::CounterPayment,
        Self::RoundTripLeg,
        Self::BoundaryCrossing,
        Self::AssetGrant,
        Self::Recurrence,
        Self::UnexplainedInbound,
        Self::CoreTouch,
        Self::HotPartyTouch,
        Self::Magnitude,
        Self::CoreAssertion,
        Self::TopTransactions,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PolicyMint => "policy_mint",
            Self::MintFundSplit => "mint_fund_split",
            Self::RoyaltyPayment => "royalty_payment",
            Self::FanoutDistribution => "fanout_distribution",
            Self::TxMessage => "tx_message",
            Self::VenueSale => "venue_sale",
            Self::CounterPayment => "counter_payment",
            Self::RoundTripLeg => "round_trip_leg",
            Self::BoundaryCrossing => "boundary_crossing",
            Self::AssetGrant => "asset_grant",
            Self::Recurrence => "recurrence",
            Self::UnexplainedInbound => "unexplained_inbound",
            Self::CoreTouch => "core_touch",
            Self::HotPartyTouch => "hot_party_touch",
            Self::Magnitude => "magnitude",
            Self::CoreAssertion => "core_assertion",
            Self::TopTransactions => "top_transactions",
        }
    }

    /// The epistemic standing of the CLAIM each signal makes. Fixed per
    /// signal — the vocabulary carries its own epistemics, so an extractor
    /// cannot accidentally launder a guess as an observation.
    pub const fn basis(self) -> Basis {
        match self {
            // Read directly off the chain.
            Self::PolicyMint | Self::MintFundSplit | Self::TxMessage | Self::Magnitude => {
                Basis::Observed
            }
            // Inherited from a registry or another ledger's decode.
            Self::VenueSale => Basis::Asserted,
            // A human said so (the signal restates the assertion).
            Self::CoreAssertion => Basis::Asserted,
            // Everything else follows from recorded claims by a stated rule.
            _ => Basis::Derived,
        }
    }

    /// Exempt from provider damping.
    ///
    /// Damping attenuates what a provider RADIATES; structural signals are
    /// facts about the transaction itself and fire at full strength whoever
    /// is involved. This split is what separates a provider's activity ON
    /// THIS PROJECT (policy-anchored: fund splits, distributions, royalties)
    /// from its other business. Without it, damping would suppress the exact
    /// flows under investigation — rewards distribution through a provider.
    pub const fn is_structural(self) -> bool {
        matches!(
            self,
            Self::PolicyMint
                | Self::MintFundSplit
                | Self::RoyaltyPayment
                | Self::FanoutDistribution
                | Self::TxMessage
                | Self::BoundaryCrossing
        )
    }
}

impl std::fmt::Display for Signal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Signal {
    type Err = UnknownSignal;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| UnknownSignal(s.to_string()))
    }
}

/// A stored signal nobody recognises — an ERROR, not a skip. It means the
/// vocabulary moved on and a reader is being shown less than the data holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownSignal(pub String);

impl std::fmt::Display for UnknownSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown interest signal: {}", self.0)
    }
}

impl std::error::Error for UnknownSignal {}

/// How firmly a human holds a tag. An enum, not a float — false precision
/// invites weight-tuning theatre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Tentative,
    Probable,
    /// Reserved for humans. Tool proposals may never carry it.
    Confirmed,
}

impl Confidence {
    pub const ALL: [Self; 3] = [Self::Tentative, Self::Probable, Self::Confirmed];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tentative => "tentative",
            Self::Probable => "probable",
            Self::Confirmed => "confirmed",
        }
    }

    /// Parse a stored confidence, degrading DOWNWARD: an unreadable value
    /// must never be promoted toward certainty. Same rule as
    /// [`Basis::parse`].
    pub fn parse(s: &str) -> Self {
        match s {
            "confirmed" => Self::Confirmed,
            "probable" => Self::Probable,
            _ => Self::Tentative,
        }
    }

    /// How much of a signal's full weight this level of conviction carries —
    /// a tentative core seed must not radiate at full strength.
    pub const fn factor(self) -> f64 {
        match self {
            Self::Tentative => 0.4,
            Self::Probable => 0.75,
            Self::Confirmed => 1.0,
        }
    }
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Tunable weights and parameters. Starting points, not truths: a score run
/// records the weights it used, so a re-tune is visible rather than a silent
/// reinterpretation of old evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Weights {
    pub policy_mint: f64,
    /// Multiplied by the SHARE of total mint proceeds.
    pub mint_fund_split: f64,
    pub royalty_payment: f64,
    pub fanout_distribution: f64,
    pub tx_message: f64,
    pub venue_sale: f64,
    /// Multiplied by the match's time-decay confidence.
    pub counter_payment: f64,
    pub round_trip_leg: f64,
    pub boundary_crossing: f64,
    pub asset_grant: f64,
    pub recurrence: f64,
    pub unexplained_inbound: f64,
    /// Multiplied by the assertion's [`Confidence::factor`].
    pub core_touch: f64,
    /// Multiplied by the participant's normalised party score (and damping).
    pub hot_party_touch: f64,
    pub core_assertion: f64,
    /// Multiplied into the sum of a party's top-K transaction scores.
    pub top_transactions: f64,

    /// Distinct recipients before one tx reads as a distribution.
    pub fanout_min_recipients: u32,
    /// Counter-payment window (seconds). Full confidence inside
    /// `counter_payment_full_secs`, linear decay to zero at the window edge.
    pub counter_payment_window_secs: i64,
    pub counter_payment_full_secs: i64,
    /// Recurrence: a pair needs at least this many events…
    pub recurrence_min_events: u32,
    /// …at most this many (beyond it is churn, not a schedule)…
    pub recurrence_max_events: u32,
    /// …with interval coefficient-of-variation below this.
    pub recurrence_max_cv: f64,
    /// How many of a party's best transactions accrue to it.
    pub party_top_k: usize,
    /// What a provider's RADIATED interest is multiplied by. Structural
    /// signals are exempt — see [`Signal::is_structural`].
    pub provider_damp: f64,
    /// Magnitude multiplier range: `0.5 + percentile`, i.e. 0.5–1.5.
    pub magnitude_floor: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            policy_mint: 0.3,
            mint_fund_split: 2.0,
            royalty_payment: 0.4,
            fanout_distribution: 0.6,
            tx_message: 0.2,
            venue_sale: -0.5,
            counter_payment: -0.4,
            round_trip_leg: -0.6,
            boundary_crossing: 0.7,
            asset_grant: 0.8,
            recurrence: 0.5,
            unexplained_inbound: 0.6,
            core_touch: 1.0,
            hot_party_touch: 0.4,
            core_assertion: 2.0,
            top_transactions: 0.2,
            fanout_min_recipients: 10,
            counter_payment_window_secs: 24 * 3600,
            counter_payment_full_secs: 3600,
            recurrence_min_events: 3,
            recurrence_max_events: 50,
            recurrence_max_cv: 0.5,
            party_top_k: 10,
            provider_damp: 0.1,
            magnitude_floor: 0.5,
        }
    }
}

/// Fold the magnitude multiplier into a final score, returning
/// `(score, magnitude_row_weight)` such that **stored rows still sum to the
/// score**: the multiplier is materialised as the delta it contributed.
///
/// A negative additive sum clamps to zero — suppression can silence a
/// transaction but not make it anti-interesting — and the magnitude row
/// carries the balancing delta so the invariant holds even then.
pub fn finalize(additive_sum: f64, magnitude_mult: f64) -> (f64, f64) {
    let score = (additive_sum.max(0.0)) * magnitude_mult;
    (score, score - additive_sum)
}

/// The counter-payment time-decay: 1.0 inside `full_secs`, linear to 0.0 at
/// `window_secs`, 0.0 outside. The design's stance in arithmetic: a value
/// transfer minutes later is strong evidence of a trade; one 23 hours later
/// barely counts.
pub fn counter_payment_confidence(gap_secs: i64, full_secs: i64, window_secs: i64) -> f64 {
    let gap = gap_secs.abs();
    if gap <= full_secs {
        1.0
    } else if gap >= window_secs {
        0.0
    } else {
        1.0 - (gap - full_secs) as f64 / (window_secs - full_secs) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signals_round_trip_and_reject_the_unknown() {
        for s in Signal::ALL {
            assert_eq!(s.as_str().parse::<Signal>().unwrap(), s);
        }
        assert!("vibes".parse::<Signal>().is_err());
    }

    /// The vocabulary carries its epistemics: an extractor cannot store a
    /// guess as an observation because the signal, not the caller, owns the
    /// basis.
    #[test]
    fn bases_are_fixed_per_signal() {
        assert_eq!(Signal::TxMessage.basis(), Basis::Observed);
        assert_eq!(Signal::VenueSale.basis(), Basis::Asserted);
        assert_eq!(Signal::AssetGrant.basis(), Basis::Derived);
        assert_eq!(Signal::CoreAssertion.basis(), Basis::Asserted);
        assert_eq!(Signal::HotPartyTouch.basis(), Basis::Derived);
    }

    /// The structural exemption IS the mint/airdrop guarantee: damping must
    /// not suppress a provider's on-project activity.
    #[test]
    fn structural_exemption_covers_mints_and_distributions() {
        assert!(Signal::MintFundSplit.is_structural());
        assert!(Signal::FanoutDistribution.is_structural());
        assert!(Signal::TxMessage.is_structural());
        assert!(Signal::RoyaltyPayment.is_structural());
        assert!(Signal::BoundaryCrossing.is_structural());
        // Radiated interest is exactly what damping exists to attenuate.
        assert!(!Signal::HotPartyTouch.is_structural());
        assert!(!Signal::CoreTouch.is_structural());
    }

    /// The evidence-panel invariant: rows sum to the score, magnitude
    /// included, clamping included.
    #[test]
    fn finalize_keeps_rows_summing_to_the_score() {
        for (sum, mult) in [(1.8, 1.3), (0.4, 0.5), (2.0, 1.0), (-0.7, 1.2), (0.0, 1.5)] {
            let (score, mag_row) = finalize(sum, mult);
            assert!(
                (sum + mag_row - score).abs() < 1e-12,
                "rows must sum to score: {sum} + {mag_row} != {score}"
            );
            assert!(score >= 0.0, "suppression silences, never inverts");
        }
    }

    #[test]
    fn confidence_degrades_downward_and_scales() {
        for c in Confidence::ALL {
            assert_eq!(Confidence::parse(c.as_str()), c);
        }
        assert_eq!(Confidence::parse("certain?!"), Confidence::Tentative);
        assert!(Confidence::Tentative.factor() < Confidence::Probable.factor());
        assert!(Confidence::Probable.factor() < Confidence::Confirmed.factor());
    }

    #[test]
    fn counter_payment_decay_is_full_then_linear_then_zero() {
        let (f, w) = (3600, 24 * 3600);
        assert_eq!(counter_payment_confidence(600, f, w), 1.0);
        assert_eq!(counter_payment_confidence(-600, f, w), 1.0, "direction-agnostic");
        assert_eq!(counter_payment_confidence(w + 1, f, w), 0.0);
        let mid = counter_payment_confidence((w + f) / 2, f, w);
        assert!(mid > 0.45 && mid < 0.55, "roughly half at the midpoint: {mid}");
    }
}
