//! Pluggable coin selection — the heart of the v2 builder
//! (`docs/design/CARDANO_TX_BUILDER_V2.md`).
//!
//! Pure, deterministic, wasm-safe. Selection is abstracted over a [`Selectable`]
//! trait so it can be fed from `cardano_assets::UtxoApi`, the engine's D1 wallet
//! ledger, or CIP-30 wallet UTxOs uniformly, and it has first-class **must-spend**
//! and **exclude** sets — neither of which the old per-builder largest-first loops
//! expressed (the engine had to pre-subtract earmarked parcels by hand at every
//! call site). v1 covers a pure-ADA (lovelace) target; multi-asset `Value` netting
//! is a documented future extension.

use std::collections::HashSet;

/// A spendable UTxO, abstracted so selection isn't bound to one concrete type.
pub trait Selectable {
    fn tx_hash(&self) -> &str;
    fn output_index(&self) -> u32;
    fn lovelace(&self) -> u64;
    /// Carries native assets — skipped by pure-ADA cover (v1) and never funds a
    /// lovelace target on its own.
    fn has_assets(&self) -> bool;
    /// Carries a CIP-33 script reference — never auto-selected from the pool.
    fn has_script_ref(&self) -> bool;
}

impl Selectable for cardano_assets::UtxoApi {
    fn tx_hash(&self) -> &str {
        &self.tx_hash
    }
    fn output_index(&self) -> u32 {
        self.output_index
    }
    fn lovelace(&self) -> u64 {
        self.lovelace
    }
    fn has_assets(&self) -> bool {
        !self.assets.is_empty()
    }
    fn has_script_ref(&self) -> bool {
        self.has_tag(cardano_assets::UtxoTag::HasScriptRef)
    }
}

/// How the pool is drawn down to cover the remaining target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    /// Prefer a single smallest-sufficient pool UTxO; else accumulate smallest
    /// first. Minimises change fragmentation — the default for refunds / float
    /// top-up where you want to disturb the float as little as possible.
    SmallestSufficient,
    /// Accumulate largest-first (fewest inputs) — sweeps / consolidation.
    LargestFirst,
    /// Spend ONLY `must_spend`; never touch the pool. The caller fully decides the
    /// input set (a split source, a mint's parcels). Errors if `must_spend` can't
    /// cover the target — keeps a self-funded build's hash deterministic.
    ManualOnly,
}

/// A selection request: a fixed must-spend set, a candidate pool, an exclude set,
/// and the strategy for drawing from the pool.
pub struct Selection<'a, U: Selectable> {
    /// ALWAYS spent, in this order (a split source, an order's payment, parcels).
    /// Bypasses the `exclude` set — these are deliberately chosen.
    pub must_spend: Vec<&'a U>,
    /// Candidate pool to cover the remaining target after `must_spend`.
    pub pool: &'a [U],
    /// `(tx_hash, output_index)` pairs never drawn from the pool — retires the
    /// engine's `exclude_earmarked_parcels` pre-filter dance.
    pub exclude: &'a HashSet<(String, u32)>,
    pub strategy: Strategy,
}

/// Why selection failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectError {
    /// `must_spend` + the (filtered) pool can't reach `target`. Carries the figures
    /// so the caller can fall back (e.g. to a chain fetch) or defer.
    Insufficient { target: u64, available: u64 },
}

impl std::fmt::Display for SelectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelectError::Insufficient { target, available } => write!(
                f,
                "coin selection insufficient: need {target} lovelace, pool covers {available}"
            ),
        }
    }
}

impl std::error::Error for SelectError {}

/// Select inputs covering `target_lovelace`. Returns `must_spend` followed by the
/// chosen pool subset. Pure and deterministic: stable ordering with an explicit
/// `(lovelace, tx_hash, output_index)` tiebreak — no clock, no randomness, no map
/// iteration leaking into the result.
pub fn select<'a, U: Selectable>(
    sel: &Selection<'a, U>,
    target_lovelace: u64,
) -> Result<Vec<&'a U>, SelectError> {
    let mut chosen: Vec<&'a U> = sel.must_spend.clone();
    let mut sum: u64 = chosen.iter().map(|u| u.lovelace()).sum();

    // ManualOnly: the must-spend set is the whole answer (or a failure).
    if sel.strategy == Strategy::ManualOnly {
        return if sum >= target_lovelace {
            Ok(chosen)
        } else {
            Err(SelectError::Insufficient {
                target: target_lovelace,
                available: sum,
            })
        };
    }
    if sum >= target_lovelace {
        return Ok(chosen); // must_spend already covers it
    }

    // Pool candidates: pure-ADA, no script ref, not excluded, not already a
    // must-spend input.
    let must_ids: HashSet<(&str, u32)> = sel
        .must_spend
        .iter()
        .map(|u| (u.tx_hash(), u.output_index()))
        .collect();
    let mut cands: Vec<&'a U> = sel
        .pool
        .iter()
        .filter(|u| !u.has_assets() && !u.has_script_ref())
        .filter(|u| {
            !sel.exclude
                .contains(&(u.tx_hash().to_string(), u.output_index()))
        })
        .filter(|u| !must_ids.contains(&(u.tx_hash(), u.output_index())))
        .collect();

    // Deterministic tiebreak helper.
    let by_id = |a: &&U, b: &&U| {
        a.tx_hash()
            .cmp(b.tx_hash())
            .then(a.output_index().cmp(&b.output_index()))
    };

    if sel.strategy == Strategy::SmallestSufficient {
        // A single smallest UTxO that covers the whole remaining gap = the least
        // disruptive choice (one input, one change).
        let need = target_lovelace - sum;
        let mut sufficient: Vec<&'a U> = cands
            .iter()
            .copied()
            .filter(|u| u.lovelace() >= need)
            .collect();
        if !sufficient.is_empty() {
            sufficient.sort_by(|a, b| a.lovelace().cmp(&b.lovelace()).then_with(|| by_id(a, b)));
            chosen.push(sufficient[0]);
            return Ok(chosen);
        }
        // No single cover → accumulate smallest-first.
        cands.sort_by(|a, b| a.lovelace().cmp(&b.lovelace()).then_with(|| by_id(a, b)));
    } else {
        // LargestFirst — fewest inputs.
        cands.sort_by(|a, b| b.lovelace().cmp(&a.lovelace()).then_with(|| by_id(a, b)));
    }

    for u in cands {
        chosen.push(u);
        sum = sum.saturating_add(u.lovelace());
        if sum >= target_lovelace {
            return Ok(chosen);
        }
    }
    Err(SelectError::Insufficient {
        target: target_lovelace,
        available: sum,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal pure-ADA test UTxO (no cardano-assets dependency in the test).
    #[derive(Debug, PartialEq)]
    struct U {
        h: String,
        i: u32,
        l: u64,
        assets: bool,
        script: bool,
    }
    impl U {
        fn ada(h: &str, i: u32, l: u64) -> Self {
            U {
                h: h.into(),
                i,
                l,
                assets: false,
                script: false,
            }
        }
    }
    impl Selectable for U {
        fn tx_hash(&self) -> &str {
            &self.h
        }
        fn output_index(&self) -> u32 {
            self.i
        }
        fn lovelace(&self) -> u64 {
            self.l
        }
        fn has_assets(&self) -> bool {
            self.assets
        }
        fn has_script_ref(&self) -> bool {
            self.script
        }
    }

    fn empty_exclude() -> HashSet<(String, u32)> {
        HashSet::new()
    }
    fn ids(v: &[&U]) -> Vec<(String, u32)> {
        v.iter().map(|u| (u.h.clone(), u.i)).collect()
    }

    #[test]
    fn must_spend_alone_covers_short_circuits() {
        let a = U::ada("aa", 0, 10_000_000);
        let ex = empty_exclude();
        let pool: Vec<U> = vec![U::ada("bb", 0, 5_000_000)];
        let sel = Selection {
            must_spend: vec![&a],
            pool: &pool,
            exclude: &ex,
            strategy: Strategy::SmallestSufficient,
        };
        let out = select(&sel, 8_000_000).unwrap();
        assert_eq!(ids(&out), vec![("aa".into(), 0)]); // pool untouched
    }

    #[test]
    fn smallest_sufficient_picks_single_least_disruptive() {
        let pool = vec![
            U::ada("aa", 0, 3_000_000),
            U::ada("bb", 0, 8_000_000),
            U::ada("cc", 0, 20_000_000),
        ];
        let ex = empty_exclude();
        let sel = Selection {
            must_spend: vec![],
            pool: &pool,
            exclude: &ex,
            strategy: Strategy::SmallestSufficient,
        };
        // need 7M → the single smallest cover is bb (8M), not cc.
        let out = select(&sel, 7_000_000).unwrap();
        assert_eq!(ids(&out), vec![("bb".into(), 0)]);
    }

    #[test]
    fn smallest_sufficient_accumulates_when_no_single_cover() {
        let pool = vec![
            U::ada("aa", 0, 3_000_000),
            U::ada("bb", 0, 4_000_000),
            U::ada("cc", 0, 5_000_000),
        ];
        let ex = empty_exclude();
        let sel = Selection {
            must_spend: vec![],
            pool: &pool,
            exclude: &ex,
            strategy: Strategy::SmallestSufficient,
        };
        // need 11M, no single UTxO covers → smallest-first 3+4+5.
        let out = select(&sel, 11_000_000).unwrap();
        assert_eq!(
            ids(&out),
            vec![("aa".into(), 0), ("bb".into(), 0), ("cc".into(), 0)]
        );
    }

    #[test]
    fn largest_first_minimises_inputs() {
        let pool = vec![
            U::ada("aa", 0, 3_000_000),
            U::ada("bb", 0, 4_000_000),
            U::ada("cc", 0, 9_000_000),
        ];
        let ex = empty_exclude();
        let sel = Selection {
            must_spend: vec![],
            pool: &pool,
            exclude: &ex,
            strategy: Strategy::LargestFirst,
        };
        let out = select(&sel, 8_000_000).unwrap();
        assert_eq!(ids(&out), vec![("cc".into(), 0)]); // one big input
    }

    #[test]
    fn exclude_and_assets_and_scriptref_are_skipped() {
        let pool = vec![
            U {
                h: "ex".into(),
                i: 0,
                l: 50_000_000,
                assets: false,
                script: false,
            }, // excluded
            U {
                h: "as".into(),
                i: 0,
                l: 50_000_000,
                assets: true,
                script: false,
            }, // asset-bearing
            U {
                h: "sc".into(),
                i: 0,
                l: 50_000_000,
                assets: false,
                script: true,
            }, // script ref
            U::ada("ok", 0, 9_000_000),
        ];
        let mut ex = empty_exclude();
        ex.insert(("ex".into(), 0));
        let sel = Selection {
            must_spend: vec![],
            pool: &pool,
            exclude: &ex,
            strategy: Strategy::LargestFirst,
        };
        let out = select(&sel, 8_000_000).unwrap();
        assert_eq!(ids(&out), vec![("ok".into(), 0)]);
    }

    #[test]
    fn manual_only_never_touches_pool() {
        let a = U::ada("aa", 0, 6_000_000);
        let pool = vec![U::ada("bb", 0, 100_000_000)];
        let ex = empty_exclude();
        let sel = Selection {
            must_spend: vec![&a],
            pool: &pool,
            exclude: &ex,
            strategy: Strategy::ManualOnly,
        };
        assert!(select(&sel, 5_000_000).is_ok()); // 6M covers 5M
        assert_eq!(
            select(&sel, 9_000_000),
            Err(SelectError::Insufficient {
                target: 9_000_000,
                available: 6_000_000
            })
        ); // pool NOT used to top up
    }

    #[test]
    fn insufficient_reports_available() {
        let pool = vec![U::ada("aa", 0, 2_000_000), U::ada("bb", 0, 3_000_000)];
        let ex = empty_exclude();
        let sel = Selection {
            must_spend: vec![],
            pool: &pool,
            exclude: &ex,
            strategy: Strategy::LargestFirst,
        };
        assert_eq!(
            select(&sel, 10_000_000),
            Err(SelectError::Insufficient {
                target: 10_000_000,
                available: 5_000_000
            })
        );
    }

    #[test]
    fn deterministic_same_inputs_same_output() {
        // Equal-value UTxOs must break ties by (tx_hash, output_index), stably.
        let pool = vec![
            U::ada("cc", 1, 5_000_000),
            U::ada("aa", 0, 5_000_000),
            U::ada("bb", 0, 5_000_000),
        ];
        let ex = empty_exclude();
        let run = || {
            let sel = Selection {
                must_spend: vec![],
                pool: &pool,
                exclude: &ex,
                strategy: Strategy::LargestFirst,
            };
            ids(&select(&sel, 9_000_000).unwrap())
        };
        let first = run();
        assert_eq!(first, run());
        // largest-first, all equal → ascending id tiebreak: aa#0, bb#0.
        assert_eq!(first, vec![("aa".into(), 0), ("bb".into(), 0)]);
    }
}
