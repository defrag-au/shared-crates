//! Net attribution — turning transactions into deltas and directed movements.

use std::collections::BTreeMap;

use crate::model::{Movement, Party, TxDelta, TxView};

/// Every party's NET movement in one transaction.
///
/// Parties whose inputs and outputs cancel are dropped: a wallet that appears
/// on both sides of a batched transaction purely as a pass-through did not
/// participate economically, and listing it as a zero row invites someone to
/// read presence as involvement.
pub fn net_deltas(view: &TxView) -> Vec<TxDelta> {
    let mut nets: BTreeMap<usize, i128> = BTreeMap::new();
    for i in &view.inputs {
        *nets.entry(i.party).or_default() -= i.value;
    }
    for o in &view.outputs {
        *nets.entry(o.party).or_default() += o.value;
    }
    nets.into_iter()
        .filter(|(_, d)| *d != 0)
        .filter_map(|(r, delta)| {
            view.party(r).map(|p| TxDelta {
                tx_id: view.tx_id.clone(),
                timestamp: view.timestamp,
                party: p.clone(),
                delta,
            })
        })
        .collect()
}

/// Directed movements for one transaction.
///
/// Each net-negative party is matched against each net-positive one, pro-rata
/// on both sides, which is the only defensible reading when a transaction mixes
/// several payers and payees. The remainder from integer division is given to
/// the largest pair so the movements sum exactly to the amount that moved —
/// dropping it loses value silently, and a reconciliation that is off by a few
/// units is indistinguishable from one that is off by a real amount.
pub fn movements(view: &TxView) -> Vec<Movement> {
    let deltas = net_deltas(view);
    let (senders, receivers): (Vec<_>, Vec<_>) = deltas.iter().partition(|d| d.delta < 0);
    if senders.is_empty() || receivers.is_empty() {
        return Vec::new();
    }

    let out_total: i128 = senders.iter().map(|d| -d.delta).sum();
    let in_total: i128 = receivers.iter().map(|d| d.delta).sum();
    // Fees mean the two sides need not match; the movable amount is the smaller.
    let movable = out_total.min(in_total);
    if movable == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut assigned = 0i128;
    let (mut best, mut best_value) = (0usize, -1i128);
    for s in &senders {
        for r in &receivers {
            let value = (-s.delta) * r.delta / in_total * movable / out_total.max(1);
            // Recompute directly to avoid compounding two truncations.
            let value = if value == 0 {
                (-s.delta) * r.delta * movable / (out_total * in_total)
            } else {
                value
            };
            if value > best_value {
                best_value = value;
                best = out.len();
            }
            assigned += value;
            out.push(Movement {
                tx_id: view.tx_id.clone(),
                timestamp: view.timestamp,
                from: s.party.clone(),
                to: r.party.clone(),
                value,
            });
        }
    }
    if let Some(m) = out.get_mut(best) {
        m.value += movable - assigned;
    }
    out.retain(|m| m.value != 0);
    out
}

/// A counterparty that returned as much as it received — the wallet's own money
/// coming home, not income.
///
/// Worth first-class support because a gross-inflow view books the return leg
/// as revenue, and any percentage computed from that inflated base is wrong in
/// the flattering direction. In the Mekka reward wallet this accounted for
/// 13,726 of 142,132 lifetime "income", including one counterparty that
/// received 4,025.17 and sent back 4,024.83 a fortnight later — the difference
/// being two transaction fees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundTrip {
    pub counterparty: Party,
    pub paid_us: i128,
    pub we_sent: i128,
}

impl RoundTrip {
    /// Inflow from this party that is not genuinely new money.
    pub fn recycled(&self) -> i128 {
        self.paid_us.min(self.we_sent)
    }
}

/// Round trips against `subject`, over any set of deltas.
///
/// `deltas` should be the subject's own deltas plus the counterparty attribution
/// already resolved — i.e. built from [`movements`] — so this operates on
/// directed value rather than guessing a counterparty per transaction.
pub fn round_trips(subject: &Party, movements: &[Movement]) -> Vec<RoundTrip> {
    let mut inn: BTreeMap<&Party, i128> = BTreeMap::new();
    let mut out: BTreeMap<&Party, i128> = BTreeMap::new();
    for m in movements {
        if &m.to == subject {
            *inn.entry(&m.from).or_default() += m.value;
        } else if &m.from == subject {
            *out.entry(&m.to).or_default() += m.value;
        }
    }
    let mut trips: Vec<RoundTrip> = inn
        .into_iter()
        .filter_map(|(p, paid_us)| {
            let we_sent = out.get(p).copied().unwrap_or(0);
            (we_sent > 0).then(|| RoundTrip {
                counterparty: p.clone(),
                paid_us,
                we_sent,
            })
        })
        .collect();
    trips.sort_by_key(|t| -t.recycled());
    trips
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Chain, TxInput, TxOutput, TxView};

    fn view(parties: Vec<Party>, inputs: &[(usize, i128)], outputs: &[(usize, i128)]) -> TxView {
        TxView {
            chain: Chain::Cardano,
            tx_id: "tx".into(),
            timestamp: 0,
            parties,
            inputs: inputs
                .iter()
                .map(|(party, value)| TxInput {
                    party: *party,
                    value: *value,
                    source: None,
                })
                .collect(),
            outputs: outputs
                .iter()
                .map(|(party, value)| TxOutput {
                    party: *party,
                    value: *value,
                })
                .collect(),
        }
    }

    fn p(n: &str) -> Party {
        Party::cardano_stake(n)
    }

    /// The change-output trap: A spends a 4,000 UTxO to pay B 400, and gets
    /// 3,600 back. A's net is -400, not -4,000, and B's is +400.
    #[test]
    fn change_output_does_not_inflate_the_payment() {
        let v = view(vec![p("A"), p("B")], &[(0, 4_000)], &[(1, 400), (0, 3_600)]);
        let d = net_deltas(&v);
        assert_eq!(d.len(), 2);
        assert_eq!(d.iter().find(|x| x.party == p("A")).unwrap().delta, -400);
        assert_eq!(d.iter().find(|x| x.party == p("B")).unwrap().delta, 400);
    }

    /// The 203,000,000 error: our wallet is a minor input in a batch whose
    /// outputs belong to other people. Gross attribution books all 10,000 as
    /// ours; the net is 100.
    #[test]
    fn batched_tx_attributes_only_our_net() {
        let v = view(
            vec![p("us"), p("other"), p("dest1"), p("dest2")],
            &[(0, 1_000), (1, 9_000)],
            &[(2, 5_000), (3, 4_100), (0, 900)],
        );
        let d = net_deltas(&v);
        assert_eq!(d.iter().find(|x| x.party == p("us")).unwrap().delta, -100);
    }

    /// A pure pass-through is not a participant.
    #[test]
    fn zero_net_parties_are_dropped() {
        let v = view(vec![p("A"), p("B")], &[(0, 500)], &[(0, 500)]);
        assert!(net_deltas(&v).is_empty());
        let v = view(vec![p("A"), p("B")], &[(0, 500)], &[(1, 500)]);
        assert_eq!(net_deltas(&v).len(), 2);
    }

    /// Movements must sum to what actually moved — no value lost to rounding.
    #[test]
    fn movements_conserve_value_across_many_parties() {
        let v = view(
            vec![p("s1"), p("s2"), p("r1"), p("r2"), p("r3")],
            &[(0, 1_000), (1, 333)],
            &[(2, 444), (3, 444), (4, 445)],
        );
        let ms = movements(&v);
        let moved: i128 = ms.iter().map(|m| m.value).sum();
        assert_eq!(moved, 1_333, "movements must sum to the amount that moved");
        assert!(ms.iter().all(|m| m.value > 0));
    }

    #[test]
    fn single_payer_single_payee_is_exact() {
        let v = view(vec![p("A"), p("B")], &[(0, 4_000)], &[(1, 400), (0, 3_600)]);
        let ms = movements(&v);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].from, p("A"));
        assert_eq!(ms[0].to, p("B"));
        assert_eq!(ms[0].value, 400);
    }

    /// Money sent out and returned is not income.
    #[test]
    fn round_trip_is_detected_and_nets_to_recycled() {
        let sent = Movement {
            tx_id: "t1".into(),
            timestamp: 1,
            from: p("us"),
            to: p("parking"),
            value: 4_025,
        };
        let back = Movement {
            tx_id: "t2".into(),
            timestamp: 2,
            from: p("parking"),
            to: p("us"),
            value: 4_024,
        };
        let real = Movement {
            tx_id: "t3".into(),
            timestamp: 3,
            from: p("payer"),
            to: p("us"),
            value: 10_000,
        };
        let trips = round_trips(&p("us"), &[sent, back, real]);
        assert_eq!(trips.len(), 1, "only the parked counterparty round-trips");
        assert_eq!(trips[0].counterparty, p("parking"));
        assert_eq!(trips[0].recycled(), 4_024);
    }

    #[test]
    fn stakeless_shape_is_reported_not_asserted() {
        assert!(Party::cardano_enterprise("addr1v…").is_stakeless());
        assert!(!Party::cardano_stake("stake1u…").is_stakeless());
    }
}
