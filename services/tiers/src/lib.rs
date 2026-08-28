//! Configurable holding-based access tiers.
//!
//! What a wallet may do, decided from what it holds — with the ladder in
//! configuration rather than in a `match`. Three surfaces share one answer:
//! the gate that mints a session, the backend that enforces a window, and the
//! frontend that tells a holder what more would buy.
//!
//! # The split that makes it reusable
//!
//! **Measuring is the caller's job; deciding is this crate's.** Flow-explorer
//! measures a wallet's native assets with one Koios `account_assets` call;
//! meme-builder measures CrowdLocked tokens by parsing script UTxO datums.
//! Those have nothing in common, and a crate that tried to own both would
//! drag an HTTP client and a datum decoder into every consumer. So callers
//! hand in a [`Measurements`] bag of already-counted amounts, and everything
//! here is pure arithmetic over it — which is also why the whole model is
//! testable without a network.
//!
//! # The shape: OR of ANDs
//!
//! A tier lists [`Qualifier`]s and needs **any one** of them ([`TierSpec::any_of`]).
//! A qualifier lists [`Condition`]s and needs **all** of them
//! ([`Qualifier::all_of`]). That is the smallest shape covering both real
//! cases we have:
//!
//! - "1,000,000 $Aliens **or** 500 $PERP grants the 12-month tier" — two
//!   qualifiers, one condition each.
//! - "50,000 locked **and** at least 90 days left on the lock" — one
//!   qualifier, two conditions.
//!
//! # Rank is explicit, and that is not incidental
//!
//! meme-builder's predecessor picked a winner with
//! `max_by_key(|t| t.min_locked_tokens)` — the ordering was *derived* from
//! the threshold. That works only while every tier is a threshold on the same
//! quantity. The moment a tier can be reached by a different asset the
//! comparison is meaningless: 500 $PERP is not "less than" 1,000,000 $Aliens, and
//! whichever happened to be the larger integer would win. Ranks are declared,
//! so adding an asset can never silently reorder the ladder.
//!
//! # Failing closed
//!
//! An unknown tier id resolves to the floor ([`TierSet::floor`]), never to an
//! error and never to the top — a tampered or stale claim can only ever
//! reduce access. [`TierSet::validate`] is the other half: it rejects a
//! malformed ladder at boot, so a typo'd policy id is a startup failure
//! rather than a gate that silently measures zero and quietly denies
//! everyone.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ─── Sources ─────────────────────────────────────────────────────────────────

/// Prefix marking a [`Condition::source`] as a native-asset holding.
///
/// `asset:<policy_id>` counts every asset under the policy (right for an NFT
/// collection, where each token is one unit, and for a single-asset CNT
/// policy). `asset:<policy_id>.<asset_name_hex>` counts one asset, for a
/// policy that mints more than one.
pub const ASSET_PREFIX: &str = "asset:";

/// Build the source key for a native asset holding.
pub fn asset_source(policy_id: &str, asset_name_hex: Option<&str>) -> String {
    match asset_name_hex {
        Some(name) => format!("{ASSET_PREFIX}{policy_id}.{name}"),
        None => format!("{ASSET_PREFIX}{policy_id}"),
    }
}

/// The `(policy_id, asset_name)` an asset source names, or `None` if the key
/// is app-supplied rather than an asset.
pub fn parse_asset_source(source: &str) -> Option<(&str, Option<&str>)> {
    let rest = source.strip_prefix(ASSET_PREFIX)?;
    match rest.split_once('.') {
        Some((policy, name)) => Some((policy, Some(name))),
        None => Some((rest, None)),
    }
}

// ─── Config model ────────────────────────────────────────────────────────────

/// One thing that must be true, expressed as a floor on a measured quantity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    /// What to measure. `asset:<policy_id>[.<asset_name_hex>]` for a wallet
    /// holding (see [`ASSET_PREFIX`]); any other string is a quantity the
    /// caller supplies itself, e.g. `crowdlock.days_remaining`.
    pub source: String,
    /// The floor, inclusive, in whole units.
    ///
    /// **Whole tokens, never raw on-chain units.** A ladder written in raw
    /// units silently means something different the moment the gate token is
    /// swapped for one with different decimals, and nobody re-reads a
    /// threshold when changing a policy id. [`measure_assets`] does the
    /// normalisation, using the decimals the indexer reports rather than any
    /// assumption from the ticker.
    pub min: u128,
}

/// One complete way to qualify for a tier. Every condition must hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Qualifier {
    /// How to name this route in upsell copy — "$Aliens", "$PERP", "CrowdLock".
    /// Shown to holders, so it is the ticker they recognise, not a policy id.
    pub label: String,
    pub all_of: Vec<Condition>,
}

impl Qualifier {
    /// Is every condition met?
    pub fn satisfied_by(&self, held: &Measurements) -> bool {
        self.all_of.iter().all(|c| held.get(&c.source) >= c.min)
    }

    /// This qualifier's conditions paired with what is actually held —
    /// the material for "you have 400 of the 500 $PERP you'd need".
    pub fn progress(&self, held: &Measurements) -> Vec<ConditionProgress> {
        self.all_of
            .iter()
            .map(|c| {
                let have = held.get(&c.source);
                ConditionProgress {
                    source: c.source.clone(),
                    have,
                    need: c.min,
                    met: have >= c.min,
                }
            })
            .collect()
    }
}

/// One rung: an id, what reaching it grants, and every way to reach it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierSpec {
    /// Stable id. This is the JWT claim value, so changing it invalidates
    /// every live session's tier — which fails closed to the floor, but is
    /// still a logout-shaped event for holders. Rename `label`, not `id`.
    pub id: String,
    /// Human-readable name — "12 months", "Full chain".
    pub label: String,
    /// Ordering. Highest satisfied rank wins; ties are a config error.
    pub rank: u8,
    /// Ways to qualify; any one suffices. **Empty means unconditional** —
    /// that is how the floor tier is declared, and exactly one tier should
    /// have it.
    #[serde(default)]
    pub any_of: Vec<Qualifier>,
    /// `authorizations` entitlement ids this tier grants, minted into the
    /// session's `ent` scope claim. Boolean capabilities live here rather
    /// than in `limits` so one vocabulary answers "may I do X", whether the
    /// grant came from a Discord role or a token holding.
    #[serde(default)]
    pub grants: Vec<String>,
    /// Named numeric limits — `history_days`, `daily_scans`. **An absent key
    /// means unlimited**, which is why the top tier declares no
    /// `history_days` rather than a sentinel like `-1` or `0` that every
    /// reader would have to remember the meaning of.
    #[serde(default)]
    pub limits: BTreeMap<String, i64>,
}

impl TierSpec {
    /// Does this holding reach this tier? Unconditional tiers always do.
    pub fn satisfied_by(&self, held: &Measurements) -> bool {
        self.any_of.is_empty() || self.any_of.iter().any(|q| q.satisfied_by(held))
    }

    /// A named limit, or `None` for unlimited.
    pub fn limit(&self, name: &str) -> Option<i64> {
        self.limits.get(name).copied()
    }
}

/// The configured ladder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierSet {
    pub tiers: Vec<TierSpec>,
}

/// Why a ladder was rejected.
///
/// Loud on purpose. Every one of these would otherwise degrade into a gate
/// that quietly measures nothing and puts every holder on the floor — a
/// failure that looks exactly like "nobody qualifies yet".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    Empty,
    DuplicateId(String),
    DuplicateRank(u8),
    NoFloor,
    /// A policy id that is not 56 hex characters. Almost always a truncated
    /// paste, and it would match no asset a wallet could ever hold.
    MalformedPolicyId {
        tier: String,
        source: String,
    },
    /// A condition with `min: 0` — satisfied by everyone, including wallets
    /// holding none of the asset, so it silently grants the tier to all.
    ZeroThreshold {
        tier: String,
        source: String,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "tier config declares no tiers"),
            Self::DuplicateId(id) => write!(f, "two tiers share the id {id:?}"),
            Self::DuplicateRank(r) => write!(f, "two tiers share rank {r}"),
            Self::NoFloor => write!(
                f,
                "no unconditional tier — a wallet holding nothing would qualify for none"
            ),
            Self::MalformedPolicyId { tier, source } => write!(
                f,
                "tier {tier:?} references {source:?}, whose policy id is not 56 hex characters"
            ),
            Self::ZeroThreshold { tier, source } => write!(
                f,
                "tier {tier:?} sets min 0 on {source:?}, which every wallet satisfies"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

impl TierSet {
    /// Reject a ladder that cannot mean what its author intended.
    ///
    /// Call at startup and refuse to serve on failure. The alternative is a
    /// worker that boots happily and gates everyone to the floor.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.tiers.is_empty() {
            return Err(ConfigError::Empty);
        }
        let mut seen_ids: Vec<&str> = Vec::new();
        let mut seen_ranks: Vec<u8> = Vec::new();
        for tier in &self.tiers {
            if seen_ids.contains(&tier.id.as_str()) {
                return Err(ConfigError::DuplicateId(tier.id.clone()));
            }
            seen_ids.push(&tier.id);
            if seen_ranks.contains(&tier.rank) {
                return Err(ConfigError::DuplicateRank(tier.rank));
            }
            seen_ranks.push(tier.rank);

            for condition in tier.any_of.iter().flat_map(|q| &q.all_of) {
                if condition.min == 0 {
                    return Err(ConfigError::ZeroThreshold {
                        tier: tier.id.clone(),
                        source: condition.source.clone(),
                    });
                }
                if let Some((policy, _)) = parse_asset_source(&condition.source) {
                    let well_formed =
                        policy.len() == 56 && policy.chars().all(|c| c.is_ascii_hexdigit());
                    if !well_formed {
                        return Err(ConfigError::MalformedPolicyId {
                            tier: tier.id.clone(),
                            source: condition.source.clone(),
                        });
                    }
                }
            }
        }
        if !self.tiers.iter().any(|t| t.any_of.is_empty()) {
            return Err(ConfigError::NoFloor);
        }
        Ok(())
    }

    /// The unconditional tier — what a wallet holding nothing gets, and what
    /// an unrecognised claim falls back to.
    ///
    /// Lowest-ranked unconditional tier, so a config with more than one
    /// (which `validate` permits, since it is not dangerous) still fails
    /// closed rather than picking whichever came first in the file.
    pub fn floor(&self) -> Option<&TierSpec> {
        self.tiers
            .iter()
            .filter(|t| t.any_of.is_empty())
            .min_by_key(|t| t.rank)
    }

    /// Look up by id, falling back to the floor.
    ///
    /// **Never returns the top tier for an unknown id.** This reads a claim
    /// out of a token, and a token can be stale (a tier renamed under it) or
    /// forged; both must land on the least privilege available.
    pub fn resolve(&self, id: &str) -> Option<&TierSpec> {
        self.tiers
            .iter()
            .find(|t| t.id == id)
            .or_else(|| self.floor())
    }

    /// The best tier this holding reaches.
    pub fn evaluate(&self, held: &Measurements) -> Option<&TierSpec> {
        self.tiers
            .iter()
            .filter(|t| t.satisfied_by(held))
            .max_by_key(|t| t.rank)
    }

    /// The next rung above `rank`, if any — "what would more buy me".
    pub fn next_above(&self, rank: u8) -> Option<&TierSpec> {
        self.tiers
            .iter()
            .filter(|t| t.rank > rank)
            .min_by_key(|t| t.rank)
    }

    /// Every asset source the ladder mentions, deduplicated — what a caller
    /// has to measure. Lets a gate ask the config what it needs rather than
    /// hardcoding a policy id beside it, which is how the two drift.
    pub fn asset_sources(&self) -> Vec<(String, Option<String>)> {
        let mut out: Vec<(String, Option<String>)> = Vec::new();
        for tier in &self.tiers {
            for condition in tier.any_of.iter().flat_map(|q| &q.all_of) {
                if let Some((policy, name)) = parse_asset_source(&condition.source) {
                    let entry = (policy.to_string(), name.map(str::to_string));
                    if !out.contains(&entry) {
                        out.push(entry);
                    }
                }
            }
        }
        out
    }
}

// ─── Measurements ────────────────────────────────────────────────────────────

/// What a wallet actually has, keyed by source, in whole units.
///
/// An absent source reads as zero rather than as an error: a wallet holding
/// none of an asset is the ordinary case, not a failure, and forcing callers
/// to enumerate zeroes would make "I forgot to measure this" and "they hold
/// none" indistinguishable at the call site instead of at the config.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Measurements(pub BTreeMap<String, u128>);

impl Measurements {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, source: &str) -> u128 {
        self.0.get(source).copied().unwrap_or(0)
    }

    /// Record a measured amount, in whole units.
    pub fn set(&mut self, source: impl Into<String>, whole_units: u128) -> &mut Self {
        self.0.insert(source.into(), whole_units);
        self
    }

    /// Add to a source, for callers accumulating across many rows.
    pub fn add(&mut self, source: impl Into<String>, whole_units: u128) -> &mut Self {
        *self.0.entry(source.into()).or_default() += whole_units;
        self
    }
}

/// One asset row as an indexer reports it.
///
/// `decimals` is the asset's own declared precision, carried alongside the
/// raw quantity because it is the only safe way to reach whole tokens — a
/// ticker never implies decimals, and this codebase has been bitten by
/// assuming otherwise (SNEK is 0 dp; Cardano USDC is 8).
#[derive(Debug, Clone)]
pub struct AssetRow<'a> {
    pub policy_id: &'a str,
    /// Asset name, hex.
    pub asset_name: &'a str,
    /// Raw on-chain quantity, before decimals are applied.
    pub quantity: u128,
    pub decimals: u32,
}

/// Fold indexer rows into the asset measurements a ladder asks for.
///
/// Rows for policies the ladder never mentions are ignored, so a caller can
/// pass a wallet's entire holdings — which is what `account_assets` returns
/// anyway, meaning a ladder over ten assets costs no more chain calls than
/// one over a single asset.
pub fn measure_assets(config: &TierSet, rows: &[AssetRow<'_>]) -> Measurements {
    let mut out = Measurements::new();
    for (policy, name) in config.asset_sources() {
        let source = asset_source(&policy, name.as_deref());
        let total: u128 = rows
            .iter()
            .filter(|r| r.policy_id == policy)
            .filter(|r| name.as_deref().is_none_or(|n| n == r.asset_name))
            // Normalise per row: two assets under one policy may declare
            // different decimals, so a sum-then-divide would be wrong.
            .map(|r| r.quantity / 10u128.pow(r.decimals))
            .sum();
        out.set(source, total);
    }
    out
}

// ─── Outcome ─────────────────────────────────────────────────────────────────

/// One condition measured against what is held.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionProgress {
    pub source: String,
    pub have: u128,
    pub need: u128,
    pub met: bool,
}

/// How a qualifier stands — its label, its conditions, and whether it is met.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualifierProgress {
    pub label: String,
    pub met: bool,
    pub conditions: Vec<ConditionProgress>,
}

/// The full verdict: what was granted, on what evidence, and what is next.
///
/// Carries the *evidence*, not just the verdict, because a tier on its own is
/// an assertion. A holder told "basic" with no numbers has no way to tell a
/// gate that is working from one pointed at the wrong policy — and that is a
/// support ticket rather than something they can check themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub tier_id: String,
    pub label: String,
    pub rank: u8,
    pub limits: BTreeMap<String, i64>,
    pub grants: Vec<String>,
    /// How the granted tier was reached. Empty for the floor.
    pub met_by: Vec<QualifierProgress>,
    /// Every route to the next rung, with progress against each. Empty at the
    /// top.
    pub next: Vec<QualifierProgress>,
    /// The next rung's display name, for copy like "…for 24 months".
    pub next_label: Option<String>,
}

impl Outcome {
    pub fn limit(&self, name: &str) -> Option<i64> {
        self.limits.get(name).copied()
    }

    /// The `ent` scope string for the session token — space-delimited, the
    /// format `authorizations::EntitlementSet` parses.
    pub fn scope_string(&self) -> String {
        self.grants.join(" ")
    }
}

/// Evaluate a holding against a ladder and describe the result in full.
///
/// Returns `None` only for a ladder with no floor, which [`TierSet::validate`]
/// rejects — so a validated config always yields an outcome.
pub fn assess(config: &TierSet, held: &Measurements) -> Option<Outcome> {
    let tier = config.evaluate(held).or_else(|| config.floor())?;
    let next = config.next_above(tier.rank);

    let describe = |qualifiers: &[Qualifier]| -> Vec<QualifierProgress> {
        qualifiers
            .iter()
            .map(|q| QualifierProgress {
                label: q.label.clone(),
                met: q.satisfied_by(held),
                conditions: q.progress(held),
            })
            .collect()
    };

    Some(Outcome {
        tier_id: tier.id.clone(),
        label: tier.label.clone(),
        rank: tier.rank,
        limits: tier.limits.clone(),
        grants: tier.grants.clone(),
        // Only the qualifiers that actually fired — listing the unmet routes
        // to a tier you already hold is noise.
        met_by: describe(&tier.any_of)
            .into_iter()
            .filter(|q| q.met)
            .collect(),
        next: next.map(|t| describe(&t.any_of)).unwrap_or_default(),
        next_label: next.map(|t| t.label.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALIENS: &str = "16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb7";
    const PERP: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c";

    fn cond(source: &str, min: u128) -> Condition {
        Condition {
            source: source.to_string(),
            min,
        }
    }

    fn qual(label: &str, conditions: Vec<Condition>) -> Qualifier {
        Qualifier {
            label: label.to_string(),
            all_of: conditions,
        }
    }

    fn tier(id: &str, rank: u8, any_of: Vec<Qualifier>) -> TierSpec {
        TierSpec {
            id: id.to_string(),
            label: id.to_string(),
            rank,
            any_of,
            grants: Vec::new(),
            limits: BTreeMap::new(),
        }
    }

    /// The ladder from the user's example: $Aliens is the primary ramp, and a
    /// $PERP holding reaches 12m by a different route entirely.
    fn ladder() -> TierSet {
        TierSet {
            tiers: vec![
                tier("basic", 0, vec![]),
                tier(
                    "6m",
                    1,
                    vec![qual("$Aliens", vec![cond(&asset_source(ALIENS, None), 1_000_000)])],
                ),
                tier(
                    "12m",
                    2,
                    vec![
                        qual("$Aliens", vec![cond(&asset_source(ALIENS, None), 2_000_000)]),
                        qual("$PERP", vec![cond(&asset_source(PERP, None), 500)]),
                    ],
                ),
                tier(
                    "full",
                    3,
                    vec![qual("$Aliens", vec![cond(&asset_source(ALIENS, None), 25_000_000)])],
                ),
            ],
        }
    }

    fn held(pairs: &[(&str, u128)]) -> Measurements {
        let mut m = Measurements::new();
        for (source, amount) in pairs {
            m.set(*source, *amount);
        }
        m
    }

    #[test]
    fn a_wallet_holding_nothing_lands_on_the_floor() {
        let outcome = assess(&ladder(), &Measurements::new()).unwrap();
        assert_eq!(outcome.tier_id, "basic");
        assert!(
            outcome.met_by.is_empty(),
            "the floor is not 'met' by anything"
        );
    }

    /// The whole point of the change: a second asset reaches a rung the first
    /// asset's ladder would never have granted.
    #[test]
    fn a_different_asset_reaches_the_same_tier() {
        let by_ns = assess(&ladder(), &held(&[(&asset_source(ALIENS, None), 2_000_000)])).unwrap();
        let by_perp = assess(&ladder(), &held(&[(&asset_source(PERP, None), 500)])).unwrap();
        assert_eq!(by_ns.tier_id, "12m");
        assert_eq!(by_perp.tier_id, "12m");
        assert_eq!(by_perp.met_by.len(), 1);
        assert_eq!(by_perp.met_by[0].label, "$PERP");
    }

    /// Rank decides, not the size of the threshold. 500 $PERP beats
    /// 1,000,000 $Aliens because rank 2 beats rank 1 — the comparison the
    /// predecessor's `max_by_key(min_locked_tokens)` got backwards.
    #[test]
    fn rank_decides_not_the_larger_number() {
        let outcome = assess(
            &ladder(),
            &held(&[
                (&asset_source(ALIENS, None), 1_000_000),
                (&asset_source(PERP, None), 500),
            ]),
        )
        .unwrap();
        assert_eq!(outcome.tier_id, "12m");
    }

    #[test]
    fn just_below_a_threshold_does_not_qualify() {
        let outcome = assess(&ladder(), &held(&[(&asset_source(ALIENS, None), 999_999)])).unwrap();
        assert_eq!(outcome.tier_id, "basic");
    }

    #[test]
    fn the_threshold_itself_qualifies() {
        let outcome = assess(&ladder(), &held(&[(&asset_source(ALIENS, None), 1_000_000)])).unwrap();
        assert_eq!(outcome.tier_id, "6m");
    }

    /// meme-builder's shape: an amount AND a duration, both required.
    #[test]
    fn a_qualifier_needs_every_one_of_its_conditions() {
        let config = TierSet {
            tiers: vec![
                tier("free", 0, vec![]),
                tier(
                    "pro",
                    1,
                    vec![qual(
                        "CrowdLock",
                        vec![
                            cond("crowdlock.locked", 50_000),
                            cond("crowdlock.days_remaining", 90),
                        ],
                    )],
                ),
            ],
        };
        let enough_but_expiring = held(&[
            ("crowdlock.locked", 50_000),
            ("crowdlock.days_remaining", 30),
        ]);
        assert_eq!(
            assess(&config, &enough_but_expiring).unwrap().tier_id,
            "free"
        );

        let both = held(&[
            ("crowdlock.locked", 50_000),
            ("crowdlock.days_remaining", 90),
        ]);
        assert_eq!(assess(&config, &both).unwrap().tier_id, "pro");
    }

    // ── Upsell ──────────────────────────────────────────────────────────────

    /// Every route up is offered, not just the one the holder is already on.
    #[test]
    fn the_next_rung_lists_every_route_with_progress() {
        let outcome = assess(&ladder(), &held(&[(&asset_source(ALIENS, None), 1_000_000)])).unwrap();
        assert_eq!(outcome.next_label.as_deref(), Some("12m"));

        let labels: Vec<&str> = outcome.next.iter().map(|q| q.label.as_str()).collect();
        assert_eq!(labels, vec!["$Aliens", "$PERP"]);

        let ns_route = &outcome.next[0].conditions[0];
        assert_eq!(ns_route.have, 1_000_000);
        assert_eq!(ns_route.need, 2_000_000);
        assert!(!ns_route.met);
    }

    #[test]
    fn the_top_tier_has_nothing_to_upsell() {
        let outcome = assess(&ladder(), &held(&[(&asset_source(ALIENS, None), 25_000_000)])).unwrap();
        assert_eq!(outcome.tier_id, "full");
        assert!(outcome.next.is_empty());
        assert!(outcome.next_label.is_none());
    }

    // ── Limits and grants ───────────────────────────────────────────────────

    /// An absent limit means unlimited — the top tier declares no
    /// `history_days` rather than a sentinel nobody remembers the meaning of.
    #[test]
    fn an_absent_limit_is_unlimited() {
        let mut basic = tier("basic", 0, vec![]);
        basic.limits.insert("history_days".into(), 90);
        let full = tier("full", 1, vec![]);

        assert_eq!(basic.limit("history_days"), Some(90));
        assert_eq!(full.limit("history_days"), None, "None is the whole chain");
    }

    #[test]
    fn grants_become_a_scope_string() {
        let mut spec = tier("full", 1, vec![]);
        spec.grants = vec!["flow.excavate".into(), "flow.export".into()];
        let outcome = assess(
            &TierSet {
                tiers: vec![spec, tier("basic", 0, vec![])],
            },
            &Measurements::new(),
        )
        .unwrap();
        // Both tiers are unconditional here, so rank picks the winner.
        assert_eq!(outcome.tier_id, "full");
        assert_eq!(outcome.scope_string(), "flow.excavate flow.export");
    }

    // ── Failing closed ──────────────────────────────────────────────────────

    /// A stale or forged claim must reduce access, never expand it.
    #[test]
    fn an_unknown_claim_falls_back_to_the_floor() {
        let config = ladder();
        assert_eq!(config.resolve("12m").unwrap().id, "12m");
        assert_eq!(
            config.resolve("admin-please").unwrap().id,
            "basic",
            "an unrecognised tier id must not reach the top"
        );
    }

    // ── Measurement ─────────────────────────────────────────────────────────

    #[test]
    fn only_the_policies_the_ladder_names_are_measured() {
        let rows = vec![
            AssetRow {
                policy_id: ALIENS,
                asset_name: "6e53",
                quantity: 2_000_000,
                decimals: 0,
            },
            AssetRow {
                policy_id: "deadbeef",
                asset_name: "00",
                quantity: 99_999_999,
                decimals: 0,
            },
        ];
        let measured = measure_assets(&ladder(), &rows);
        assert_eq!(measured.get(&asset_source(ALIENS, None)), 2_000_000);
        assert_eq!(measured.get(&asset_source(PERP, None)), 0);
        assert_eq!(measured.0.len(), 2, "unnamed policies are not recorded");
    }

    /// Decimals are applied per row: two assets under one policy may declare
    /// different precision, so summing raw and dividing once would be wrong.
    #[test]
    fn decimals_are_applied_before_summing() {
        let config = TierSet {
            tiers: vec![
                tier("basic", 0, vec![]),
                tier(
                    "pro",
                    1,
                    vec![qual("mixed", vec![cond(&asset_source(ALIENS, None), 150)])],
                ),
            ],
        };
        let rows = vec![
            AssetRow {
                policy_id: ALIENS,
                asset_name: "aa",
                quantity: 100,
                decimals: 0,
            },
            // 100.000000 of an 6dp asset is 100 whole tokens, not 100 million.
            AssetRow {
                policy_id: ALIENS,
                asset_name: "bb",
                quantity: 100_000_000,
                decimals: 6,
            },
        ];
        let measured = measure_assets(&config, &rows);
        assert_eq!(measured.get(&asset_source(ALIENS, None)), 200);
        assert_eq!(assess(&config, &measured).unwrap().tier_id, "pro");
    }

    #[test]
    fn a_named_asset_source_ignores_its_policy_siblings() {
        let config = TierSet {
            tiers: vec![
                tier("basic", 0, vec![]),
                tier(
                    "pro",
                    1,
                    vec![qual(
                        "one asset",
                        vec![cond(&asset_source(ALIENS, Some("aa")), 10)],
                    )],
                ),
            ],
        };
        let rows = vec![AssetRow {
            policy_id: ALIENS,
            asset_name: "bb",
            quantity: 1_000,
            decimals: 0,
        }];
        let measured = measure_assets(&config, &rows);
        assert_eq!(measured.get(&asset_source(ALIENS, Some("aa"))), 0);
        assert_eq!(assess(&config, &measured).unwrap().tier_id, "basic");
    }

    #[test]
    fn asset_sources_round_trip() {
        assert_eq!(
            parse_asset_source(&asset_source(ALIENS, None)),
            Some((ALIENS, None))
        );
        assert_eq!(
            parse_asset_source(&asset_source(ALIENS, Some("6e53"))),
            Some((ALIENS, Some("6e53")))
        );
        assert_eq!(parse_asset_source("crowdlock.days"), None);
    }

    // ── Validation ──────────────────────────────────────────────────────────

    #[test]
    fn a_valid_ladder_passes() {
        assert_eq!(ladder().validate(), Ok(()));
    }

    #[test]
    fn a_ladder_with_no_unconditional_tier_is_rejected() {
        let config = TierSet {
            tiers: vec![tier(
                "6m",
                1,
                vec![qual("$Aliens", vec![cond(&asset_source(ALIENS, None), 1)])],
            )],
        };
        assert_eq!(config.validate(), Err(ConfigError::NoFloor));
    }

    #[test]
    fn duplicate_ids_and_ranks_are_rejected() {
        let mut config = ladder();
        config.tiers[1].id = "basic".into();
        assert_eq!(
            config.validate(),
            Err(ConfigError::DuplicateId("basic".into()))
        );

        let mut config = ladder();
        config.tiers[1].rank = 0;
        assert_eq!(config.validate(), Err(ConfigError::DuplicateRank(0)));
    }

    /// The silent one: a truncated policy id matches no asset any wallet can
    /// hold, so the tier becomes unreachable and the gate looks like it is
    /// simply strict.
    #[test]
    fn a_truncated_policy_id_is_rejected() {
        let config = TierSet {
            tiers: vec![
                tier("basic", 0, vec![]),
                tier(
                    "6m",
                    1,
                    vec![qual("$Aliens", vec![cond("asset:16657df32ad8", 1_000)])],
                ),
            ],
        };
        assert!(matches!(
            config.validate(),
            Err(ConfigError::MalformedPolicyId { .. })
        ));
    }

    /// The opposite silent one: `min: 0` is satisfied by every wallet, so the
    /// tier is granted to everyone.
    #[test]
    fn a_zero_threshold_is_rejected() {
        let config = TierSet {
            tiers: vec![
                tier("basic", 0, vec![]),
                tier(
                    "6m",
                    1,
                    vec![qual("$Aliens", vec![cond(&asset_source(ALIENS, None), 0)])],
                ),
            ],
        };
        assert!(matches!(
            config.validate(),
            Err(ConfigError::ZeroThreshold { .. })
        ));
    }

    // ── Config format ───────────────────────────────────────────────────────

    /// The wire/config format, pinned. This is what goes in wrangler.toml, so
    /// a field rename here is a deployment break.
    #[test]
    fn the_json_config_format_is_stable() {
        let json = r#"{
          "tiers": [
            { "id": "basic", "label": "90 days", "rank": 0,
              "limits": { "history_days": 90 } },
            { "id": "12m", "label": "12 months", "rank": 2,
              "any_of": [
                { "label": "$Aliens",   "all_of": [ { "source": "asset:16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb7", "min": 2000000 } ] },
                { "label": "$PERP", "all_of": [ { "source": "asset:a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c", "min": 500 } ] }
              ],
              "grants": ["flow.excavate"],
              "limits": { "history_days": 365 } }
          ]
        }"#;

        let config: TierSet = serde_json::from_str(json).unwrap();
        config.validate().unwrap();

        // The floor needs no `any_of`, and omitting it is the declaration.
        assert!(config.tiers[0].any_of.is_empty());
        assert_eq!(config.tiers[0].limit("history_days"), Some(90));

        let twelve = &config.tiers[1];
        assert_eq!(twelve.any_of.len(), 2, "two independent routes to 12m");
        assert_eq!(twelve.grants, vec!["flow.excavate"]);

        let by_perp = held(&[(&asset_source(PERP, None), 500)]);
        assert_eq!(assess(&config, &by_perp).unwrap().tier_id, "12m");
    }
}
