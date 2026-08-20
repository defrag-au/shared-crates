//! Provenance walks — "did *these* units become *those* units".
//!
//! Only meaningful where each input names the output it consumes. The walk
//! unwinds change: a change output is the wallet's own money returning, so
//! terminating on it and naming the payee of the transaction that produced it
//! is backwards. It keeps stepping back until it reaches a transaction where
//! the subject genuinely received value.

use std::collections::BTreeMap;

use crate::attribution::net_deltas;
use crate::model::{Party, TxView};

/// Where a share of the traced value entered the subject wallet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A genuine receipt: the subject's net was positive in this transaction.
    Received {
        from: Party,
        tx_id: String,
        timestamp: i64,
    },
    /// The walk hit its depth ceiling. A real terminal, not a dropped leg.
    BeyondDepth { hops: usize },
    /// The walk hit its transaction budget. Also a real terminal.
    BudgetExhausted,
    /// The parent transaction could not be resolved — typically the fetcher
    /// declined to supply it, or the edge predates the indexed range.
    Unresolved { tx_id: String },
}

/// One attributed share of the amount being traced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkLeg {
    pub origin: Origin,
    pub value: i128,
}

/// The result of a walk.
///
/// [`Self::is_complete`] is the honest-reporting hook: a caller that presents
/// an incomplete walk as a finished trace has to ignore it deliberately, rather
/// than merely fail to notice a silent truncation. An early Mekka distribution
/// scan used a block window ~4× narrower than the real funding-to-payout gap
/// and returned 2 of 10 results with no indication anything was missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkOutcome {
    pub legs: Vec<WalkLeg>,
    pub budget_remaining: usize,
}

impl WalkOutcome {
    /// Whether every leg reached a genuine receipt.
    pub fn is_complete(&self) -> bool {
        self.legs
            .iter()
            .all(|l| matches!(l.origin, Origin::Received { .. }))
    }

    /// Value that reached a genuine receipt.
    pub fn resolved_value(&self) -> i128 {
        self.legs
            .iter()
            .filter(|l| matches!(l.origin, Origin::Received { .. }))
            .map(|l| l.value)
            .sum()
    }

    pub fn total_value(&self) -> i128 {
        self.legs.iter().map(|l| l.value).sum()
    }

    /// Legs merged by origin, largest first — the reporting view.
    pub fn by_origin(&self) -> Vec<WalkLeg> {
        let mut acc: BTreeMap<String, (Origin, i128)> = BTreeMap::new();
        for leg in &self.legs {
            let key = match &leg.origin {
                Origin::Received { from, .. } => format!("r:{}", from.key),
                Origin::BeyondDepth { hops } => format!("d:{hops}"),
                Origin::BudgetExhausted => "b:".into(),
                Origin::Unresolved { tx_id } => format!("u:{tx_id}"),
            };
            let e = acc.entry(key).or_insert((leg.origin.clone(), 0));
            e.1 += leg.value;
        }
        let mut out: Vec<WalkLeg> = acc
            .into_values()
            .map(|(origin, value)| WalkLeg { origin, value })
            .collect();
        out.sort_by_key(|l| -l.value);
        out
    }
}

/// Limits for a walk. Both are reported back rather than applied silently.
///
/// High-turnover wallets pool their UTxOs, so a few hops in, a walk stops being
/// a trace of specific units and becomes a survey of how the wallet is funded
/// in general. `max_depth` is the honest boundary of the claim; `budget` stops
/// a pathological fan-out from issuing thousands of fetches.
#[derive(Debug, Clone, Copy)]
pub struct WalkBudget {
    pub max_depth: usize,
    pub budget: usize,
}

impl Default for WalkBudget {
    fn default() -> Self {
        Self {
            max_depth: 6,
            budget: 64,
        }
    }
}

/// Walk `amount` back from `view` to the receipts that funded it.
///
/// `fetch` resolves a transaction id to its view; returning `None` yields an
/// [`Origin::Unresolved`] leg rather than dropping the value. The walk is
/// caller-driven so this crate stays I/O-free — the adapter crates supply a
/// fetcher backed by a chain API and its cache.
pub fn walk_provenance(
    subject: &Party,
    view: &TxView,
    amount: i128,
    limits: WalkBudget,
    fetch: &mut dyn FnMut(&str) -> Option<TxView>,
) -> WalkOutcome {
    let mut budget = limits.budget;
    let legs = step(
        subject,
        view,
        amount,
        0,
        limits.max_depth,
        &mut budget,
        fetch,
    );
    WalkOutcome {
        legs,
        budget_remaining: budget,
    }
}

#[allow(clippy::too_many_arguments)]
fn step(
    subject: &Party,
    view: &TxView,
    amount: i128,
    depth: usize,
    max_depth: usize,
    budget: &mut usize,
    fetch: &mut dyn FnMut(&str) -> Option<TxView>,
) -> Vec<WalkLeg> {
    if depth >= max_depth {
        return vec![WalkLeg {
            origin: Origin::BeyondDepth { hops: max_depth },
            value: amount,
        }];
    }
    if *budget == 0 {
        return vec![WalkLeg {
            origin: Origin::BudgetExhausted,
            value: amount,
        }];
    }
    *budget -= 1;

    // Only the subject's own inputs carry its money; anyone else's inputs in
    // the same transaction are somebody else's funds passing through.
    let parents: Vec<(String, i128)> = view
        .find(subject)
        .map(|r| {
            view.inputs
                .iter()
                .filter(|i| i.party == r)
                .filter_map(|i| i.source.clone().map(|s| (s, i.value)))
                .collect()
        })
        .unwrap_or_default();

    let total: i128 = parents.iter().map(|(_, v)| *v).sum();
    if total == 0 {
        return vec![WalkLeg {
            origin: Origin::Unresolved {
                tx_id: view.tx_id.clone(),
            },
            value: amount,
        }];
    }

    let mut out = Vec::new();
    let mut assigned = 0i128;
    let last = parents.len() - 1;
    for (idx, (tx_id, value)) in parents.iter().enumerate() {
        // Give the remainder to the final leg so shares sum to `amount`.
        let share = if idx == last {
            amount - assigned
        } else {
            amount * value / total
        };
        assigned += share;

        let Some(parent) = fetch(tx_id) else {
            out.push(WalkLeg {
                origin: Origin::Unresolved {
                    tx_id: tx_id.clone(),
                },
                value: share,
            });
            continue;
        };

        let delta = net_deltas(&parent)
            .into_iter()
            .find(|d| &d.party == subject)
            .map(|d| d.delta)
            .unwrap_or(0);

        if delta > 0 {
            // A genuine receipt — the largest counterparty on the paying side.
            let from = net_deltas(&parent)
                .into_iter()
                .filter(|d| d.delta < 0)
                .min_by_key(|d| d.delta)
                .map(|d| d.party);
            out.push(WalkLeg {
                origin: match from {
                    Some(from) => Origin::Received {
                        from,
                        tx_id: parent.tx_id.clone(),
                        timestamp: parent.timestamp,
                    },
                    None => Origin::Unresolved {
                        tx_id: parent.tx_id.clone(),
                    },
                },
                value: share,
            });
        } else {
            // Change: the subject was paying. Keep going back.
            out.extend(step(
                subject,
                &parent,
                share,
                depth + 1,
                max_depth,
                budget,
                fetch,
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Chain, TxInput, TxOutput};
    use std::collections::HashMap;

    fn tx(
        id: &str,
        ts: i64,
        parties: Vec<Party>,
        inputs: Vec<(usize, i128, Option<&str>)>,
        outputs: Vec<(usize, i128)>,
    ) -> TxView {
        TxView {
            chain: Chain::Cardano,
            tx_id: id.into(),
            timestamp: ts,
            parties,
            inputs: inputs
                .into_iter()
                .map(|(party, value, source)| TxInput {
                    party,
                    value,
                    source: source.map(String::from),
                })
                .collect(),
            outputs: outputs
                .into_iter()
                .map(|(party, value)| TxOutput { party, value })
                .collect(),
        }
    }

    fn p(n: &str) -> Party {
        Party::cardano_stake(n)
    }

    /// The shape that mattered in the Mekka trace: the payout spent one UTxO
    /// received directly from a funder, and one CHANGE UTxO left over from an
    /// earlier payment. The change leg must resolve to whoever funded it, not
    /// to the payee of the transaction that created it.
    #[test]
    fn change_leg_resolves_past_the_payee() {
        // t1: FUNDER pays us 1_000.
        let t1 = tx(
            "t1",
            10,
            vec![p("us"), p("FUNDER")],
            vec![(1, 1_000, None)],
            vec![(0, 1_000)],
        );
        // t2: we spend that 1_000 paying VENDOR 400, change 600.
        let t2 = tx(
            "t2",
            20,
            vec![p("us"), p("VENDOR")],
            vec![(0, 1_000, Some("t1"))],
            vec![(1, 400), (0, 600)],
        );
        // t3: the payout spends the 600 change.
        let t3 = tx(
            "t3",
            30,
            vec![p("us"), p("HOLDERS")],
            vec![(0, 600, Some("t2"))],
            vec![(1, 600)],
        );

        let store: HashMap<String, TxView> =
            [("t1".to_string(), t1), ("t2".to_string(), t2)].into();
        let mut fetch = |id: &str| store.get(id).cloned();

        let out = walk_provenance(&p("us"), &t3, 600, WalkBudget::default(), &mut fetch);
        assert!(out.is_complete(), "every leg should reach a receipt");
        assert_eq!(out.total_value(), 600);
        let legs = out.by_origin();
        assert_eq!(legs.len(), 1);
        match &legs[0].origin {
            Origin::Received { from, .. } => assert_eq!(
                from,
                &p("FUNDER"),
                "must attribute to the funder, not VENDOR"
            ),
            other => panic!("expected a receipt, got {other:?}"),
        }
        assert_eq!(legs[0].value, 600);
    }

    /// Hitting the ceiling must show up as a terminal that still sums, and the
    /// walk must not claim completeness.
    #[test]
    fn depth_limit_is_reported_not_hidden() {
        let t2 = tx(
            "t2",
            20,
            vec![p("us"), p("X")],
            vec![(0, 1_000, Some("t1"))],
            vec![(1, 400), (0, 600)],
        );
        let t3 = tx(
            "t3",
            30,
            vec![p("us"), p("Y")],
            vec![(0, 600, Some("t2"))],
            vec![(1, 600)],
        );
        let store: HashMap<String, TxView> = [("t2".to_string(), t2)].into();
        let mut fetch = |id: &str| store.get(id).cloned();

        let limits = WalkBudget {
            max_depth: 1,
            budget: 64,
        };
        let out = walk_provenance(&p("us"), &t3, 600, limits, &mut fetch);
        assert!(!out.is_complete());
        assert_eq!(out.total_value(), 600, "value is never dropped");
        assert_eq!(out.resolved_value(), 0);
        assert!(matches!(
            out.by_origin()[0].origin,
            Origin::BeyondDepth { hops: 1 }
        ));
    }

    /// An unfetchable parent yields a leg, not a hole.
    #[test]
    fn unresolved_parent_keeps_its_value() {
        let t3 = tx(
            "t3",
            30,
            vec![p("us"), p("Y")],
            vec![(0, 600, Some("missing"))],
            vec![(1, 600)],
        );
        let mut fetch = |_: &str| None;
        let out = walk_provenance(&p("us"), &t3, 600, WalkBudget::default(), &mut fetch);
        assert!(!out.is_complete());
        assert_eq!(out.total_value(), 600);
    }

    /// Shares across several parent UTxOs must sum to the traced amount.
    #[test]
    fn multi_parent_shares_conserve_value() {
        let mk = |id: &str, v: i128| {
            tx(
                id,
                1,
                vec![p("us"), p("F")],
                vec![(1, v, None)],
                vec![(0, v)],
            )
        };
        let payout = tx(
            "payout",
            50,
            vec![p("us"), p("HOLDERS")],
            vec![
                (0, 333, Some("a")),
                (0, 333, Some("b")),
                (0, 334, Some("c")),
            ],
            vec![(1, 1_000)],
        );
        let store: HashMap<String, TxView> = [
            ("a".to_string(), mk("a", 333)),
            ("b".to_string(), mk("b", 333)),
            ("c".to_string(), mk("c", 334)),
        ]
        .into();
        let mut fetch = |id: &str| store.get(id).cloned();
        let out = walk_provenance(&p("us"), &payout, 1_000, WalkBudget::default(), &mut fetch);
        assert!(out.is_complete());
        assert_eq!(out.total_value(), 1_000);
    }

    #[test]
    fn account_chains_report_no_utxo_provenance() {
        assert!(Chain::Cardano.has_utxo_provenance());
        assert!(!Chain::Solana.has_utxo_provenance());
    }
}
