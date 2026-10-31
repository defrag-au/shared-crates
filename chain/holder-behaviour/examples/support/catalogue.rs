//! The frozen achievement catalogue, as far as this build can read it.
//!
//! The `predicate` table is kept as a [`toml::Value`] rather than a [`Predicate`] on
//! purpose: the catalogue names 42 predicate kinds and this build implements five, so a
//! strict parse would refuse the whole file. An entry is **evaluable** when its table
//! deserialises. Everything else is a gap in this build, reported by kind so a missing
//! answer cannot be mistaken for a zero.

use std::collections::BTreeMap;

use holder_behaviour::{Definition, Predicate};

/// A catalogue, with the predicates this build can answer resolved once at load.
pub struct LoadedCatalogue {
    entries: Vec<CatalogueEntry>,
    /// One entry per catalogue achievement, in the same order: what this build can answer,
    /// or `None`. Computed once, so a definition and a rarity row cannot disagree.
    predicates: Vec<Option<Predicate>>,
}

#[derive(serde::Deserialize)]
struct CatalogueFile {
    #[serde(default, rename = "achievement")]
    achievements: Vec<CatalogueEntry>,
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
        Ok(Self {
            entries: file.achievements,
            predicates,
        })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The catalogue id at `index`, in catalogue order.
    pub fn id_at(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(|entry| entry.id.as_str())
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
