//! The frozen achievement catalogue, as far as this build can read it.
//!
//! The `predicate` table is kept as a [`toml::Value`] rather than a [`Predicate`] on
//! purpose: the catalogue names 42 predicate kinds and this build implements five, so a
//! strict parse would refuse the whole file. An entry is **evaluable** when its table
//! deserialises. Everything else is a gap in this build, reported by kind so a missing
//! answer cannot be mistaken for a zero.

use std::collections::BTreeMap;

use holder_behaviour::{Definition, Predicate, StatusPredicate, TokenRef};

/// A catalogue, with the predicates this build can answer resolved once at load.
pub struct LoadedCatalogue {
    entries: Vec<CatalogueEntry>,
    /// One entry per catalogue achievement, in the same order: what this build can answer,
    /// or `None`. Computed once, so a definition and a rarity row cannot disagree.
    predicates: Vec<Option<Predicate>>,
    /// The policy the catalogue proposes, when it carries an `[allocation]` block.
    plan: Option<Plan>,
    /// Every declared status, by id.
    statuses: Vec<Status>,
}

/// A gated allocation policy, as the catalogue writes it.
#[derive(Debug, Clone, Copy)]
pub struct Allocation {
    /// Core achievements for the guaranteed phase.
    pub gtd_core: u32,
    /// Core achievements for the first-come phase.
    pub fcfs_core: u32,
    /// Flavour achievements that also reach the first-come phase.
    pub fcfs_flavour: u32,
    /// Mints per identity, at most.
    pub per_identity: u32,
    /// A flavour achievement held by more than this share does not count.
    pub rarity_floor: f64,
    /// How many of a holder's rarest flavour achievements count.
    pub count_rarest: u32,
}

/// A condition that is true at a read, declared once and referenced by id.
///
/// Declared in the catalogue rather than in the workspace's code so a campaign can add
/// "holds a leader role" or "holds a partner collection" without a deploy — the same reason the
/// achievement definitions live there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// Stable id, which is what `requires` and `promotes` name.
    pub id: String,
    /// What it asks.
    pub predicate: StatusPredicate,
}

/// The policy a campaign runs: the thresholds, and the statuses that decide who is in.
///
/// Kept apart from [`Allocation`] because that type is `Copy` and swept over every field, while
/// this names statuses by id and is read once.
#[derive(Debug, Clone)]
pub struct Plan {
    /// The thresholds and the cap.
    pub allocation: Allocation,
    /// Statuses that must **all** hold for any achievement to count. The gate.
    pub requires: Vec<String>,
    /// Statuses that put an identity in the guaranteed phase regardless of its score. Any one
    /// suffices.
    pub promotes: Vec<String>,
    /// Extra mints a promoted identity may take, on top of `allocation.per_identity`.
    pub promote_bonus: u32,
}

#[derive(serde::Deserialize)]
struct CatalogueFile {
    #[serde(default, rename = "achievement")]
    achievements: Vec<CatalogueEntry>,
    #[serde(default)]
    status: Vec<StatusFile>,
    #[serde(default)]
    allocation: Option<AllocationFile>,
}

#[derive(serde::Deserialize)]
struct AllocationFile {
    gtd: PhaseFile,
    fcfs: PhaseFile,
    #[serde(default = "default_per_identity")]
    per_identity: u32,
    #[serde(default = "default_floor")]
    rarity_floor: f64,
    #[serde(default = "default_rarest")]
    count_rarest: u32,
    /// Status ids that must all hold before any achievement counts. Empty means no gate.
    #[serde(default)]
    requires: Vec<String>,
    /// Status ids that put an identity straight into the guaranteed phase.
    #[serde(default)]
    promotes: Vec<String>,
    /// Mints a promoted identity may take beyond `per_identity`.
    #[serde(default)]
    promote_bonus: u32,
}

#[derive(serde::Deserialize)]
struct StatusFile {
    id: String,
    /// A CAIP-19 asset id — the collection the read must cover.
    collection: TokenRef,
    /// Holds at least this many units. Mutually exclusive with `trait`.
    #[serde(default)]
    at_least: Option<u64>,
    /// A trait category; any value satisfies it. Mutually exclusive with `at_least`.
    #[serde(default, rename = "trait")]
    trait_category: Option<String>,
}

#[derive(serde::Deserialize, Default)]
struct PhaseFile {
    #[serde(default)]
    core_at_least: u32,
    #[serde(default)]
    or_flavour_at_least: u32,
}

fn default_per_identity() -> u32 {
    3
}

fn default_floor() -> f64 {
    0.02
}

fn default_rarest() -> u32 {
    3
}

impl From<&AllocationFile> for Allocation {
    fn from(file: &AllocationFile) -> Self {
        Self {
            gtd_core: file.gtd.core_at_least,
            fcfs_core: file.fcfs.core_at_least,
            fcfs_flavour: file.fcfs.or_flavour_at_least,
            per_identity: file.per_identity,
            rarity_floor: file.rarity_floor,
            count_rarest: file.count_rarest,
        }
    }
}

impl StatusFile {
    /// A status asks for a holding **or** a trait, never both and never neither.
    fn resolve(&self) -> Result<Status, String> {
        let predicate = match (&self.at_least, &self.trait_category) {
            (Some(at_least), None) => StatusPredicate::HoldsOf {
                collection: self.collection.clone(),
                at_least: *at_least,
            },
            (None, Some(category)) => StatusPredicate::HoldsTrait {
                collection: self.collection.clone(),
                category: category.clone(),
                value: None,
            },
            (Some(_), Some(_)) => {
                return Err(format!(
                    "status {}: `at_least` and `trait` are alternatives, not both",
                    self.id
                ));
            }
            (None, None) => {
                return Err(format!("status {}: needs `at_least` or `trait`", self.id));
            }
        };
        Ok(Status {
            id: self.id.clone(),
            predicate,
        })
    }
}

#[derive(serde::Deserialize)]
struct CatalogueEntry {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    statement: String,
    #[serde(default)]
    axis: String,
    #[serde(default)]
    role: String,
    /// The source cannot answer; the catalogue says why. Never scores.
    #[serde(default)]
    unavailable: Option<String>,
    #[serde(default)]
    predicate: Option<toml::Value>,
}

impl LoadedCatalogue {
    pub fn load(path: &str) -> Result<Self, String> {
        let text =
            std::fs::read_to_string(path).map_err(|error| format!("read {path}: {error}"))?;
        let file: CatalogueFile =
            toml::from_str(&text).map_err(|error| format!("parse {path}: {error}"))?;
        if file.achievements.is_empty() {
            return Err(format!("{path} carries no achievements"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for entry in &file.achievements {
            if !seen.insert(entry.id.as_str()) {
                return Err(format!("{path}: duplicate achievement id {}", entry.id));
            }
        }
        let predicates = file
            .achievements
            .iter()
            .map(CatalogueEntry::predicate)
            .collect();

        let mut status_ids = std::collections::BTreeSet::new();
        let mut statuses = Vec::with_capacity(file.status.len());
        for status in &file.status {
            if !status_ids.insert(status.id.as_str()) {
                return Err(format!("{path}: duplicate status id {}", status.id));
            }
            statuses.push(status.resolve()?);
        }

        let plan = match file.allocation {
            Some(allocation) => {
                let plan = Plan {
                    allocation: Allocation::from(&allocation),
                    requires: allocation.requires.clone(),
                    promotes: allocation.promotes.clone(),
                    promote_bonus: allocation.promote_bonus,
                };
                for id in plan.requires.iter().chain(&plan.promotes) {
                    if !status_ids.contains(id.as_str()) {
                        return Err(format!(
                            "{path}: `{id}` is named by [allocation] but no [[status]] declares it"
                        ));
                    }
                }
                Some(plan)
            }
            None => None,
        };

        Ok(Self {
            entries: file.achievements,
            predicates,
            plan,
            statuses,
        })
    }

    /// The policy the catalogue proposes, if it carries an `[allocation]` block.
    pub fn plan(&self) -> Option<&Plan> {
        self.plan.as_ref()
    }

    /// Every declared status, in catalogue order.
    pub fn statuses(&self) -> &[Status] {
        &self.statuses
    }

    /// One status by id.
    pub fn status(&self, id: &str) -> Option<&Status> {
        self.statuses.iter().find(|status| status.id == id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The catalogue id at `index`, in catalogue order.
    pub fn id_at(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(|entry| entry.id.as_str())
    }

    /// Every id, in catalogue order — the input to the bundle's digest.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|entry| entry.id.as_str())
    }

    /// The definitions this build can answer, in catalogue order.
    pub fn definitions(&self) -> Vec<Definition<'_>> {
        self.entries
            .iter()
            .zip(&self.predicates)
            .filter_map(|(entry, predicate)| {
                Some(Definition {
                    id: entry.id.as_str(),
                    predicate: predicate.clone()?,
                })
            })
            .collect()
    }

    /// The catalogue index of each entry [`definitions`](Self::definitions) returned.
    pub fn positions(&self) -> Vec<usize> {
        self.predicates
            .iter()
            .enumerate()
            .filter(|(_, predicate)| predicate.is_some())
            .map(|(index, _)| index)
            .collect()
    }

    /// Whether this build evaluated the achievement at `index`.
    pub fn evaluated(&self, index: usize) -> bool {
        self.predicates.get(index).is_some_and(Option::is_some)
    }

    /// The catalogue fields a surface shows, at a catalogue index.
    pub fn title_at(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(|entry| entry.title.as_str())
    }

    /// The minimum continuous holding a span predicate requires, in days.
    ///
    /// `any_span_at_least` is the one predicate whose true earn slot can be recomputed from
    /// the lots, because its threshold is a duration. Other as-at predicates — a drawdown, a
    /// round trip, a holding count — have no such clock and report the reading slot instead.
    pub fn predicate_days(&self, index: usize) -> Option<u32> {
        let predicate = self.entries.get(index)?.predicate.as_ref()?;
        if predicate.get("kind")?.as_str()? != "any_span_at_least" {
            return None;
        }
        predicate
            .get("days")?
            .as_integer()
            .and_then(|days| u32::try_from(days).ok())
    }

    pub fn statement_at(&self, index: usize) -> Option<&str> {
        self.entries
            .get(index)
            .map(|entry| entry.statement.as_str())
    }

    pub fn axis_at(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(|entry| entry.axis.as_str())
    }

    pub fn role_at(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(|entry| entry.role.as_str())
    }

    /// How many catalogue entries this build did not evaluate, grouped by predicate kind.
    pub fn gaps(&self) -> BTreeMap<String, u32> {
        let mut gaps = BTreeMap::new();
        for (entry, predicate) in self.entries.iter().zip(&self.predicates) {
            if predicate.is_some() {
                continue;
            }
            let kind = entry
                .kind()
                .map(str::to_string)
                .unwrap_or_else(|| format!("({})", entry.says_why()));
            *gaps.entry(kind).or_default() += 1;
        }
        gaps
    }
}

impl CatalogueEntry {
    /// The predicate, when this build can both read it and answer it.
    fn predicate(&self) -> Option<Predicate> {
        if self.unavailable.is_some() {
            return None;
        }
        self.predicate.clone()?.try_into().ok()
    }

    fn kind(&self) -> Option<&str> {
        self.predicate.as_ref()?.get("kind")?.as_str()
    }

    fn says_why(&self) -> &'static str {
        match &self.unavailable {
            Some(_) => "unavailable",
            None => "no predicate",
        }
    }
}
