//! Browsing a whole collection a page at a time, and the list of collections.

use std::collections::HashSet;

use cardano_assets::PolicyId;
use ownership_client::BundleResponse;
use serde::Deserialize;

use crate::asset::OwnedAsset;
use crate::source::{Get, encode_component};

/// The order a [`Browse`] walks a collection in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Order {
    /// Rarest first, unranked last. Paged by offset: the service cannot
    /// carry a cursor through a rarity sort.
    #[default]
    Rarity,
    /// By on-chain asset name. Paged by the service's keyset cursor.
    Name,
}

/// Where the next page starts.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Position {
    Offset(u32),
    Cursor(Option<String>),
}

/// A collection, every asset in it, fetched a page at a time on demand.
///
/// Unlike [`crate::HoldingsLoad`] it does not fetch everything: a 10,000
/// asset collection is 100 pages. The caller says when it wants another,
/// typically when the player scrolls near the end of what has arrived.
#[derive(Debug, Clone)]
pub struct Browse {
    base: String,
    policy_id: PolicyId,
    order: Order,
    page_size: u32,
    assets: Vec<OwnedAsset>,
    seen: HashSet<String>,
    position: Position,
    total: Option<u32>,
    exhausted: bool,
    wanted: bool,
    in_flight: bool,
    error: Option<String>,
}

impl Browse {
    /// `base` is the ownership service (normally [`crate::OWNERSHIP_BASE`]).
    /// The first page is wanted at once. A page size of 0 is read as 1.
    pub fn new(base: &str, policy_id: PolicyId, order: Order, page_size: u32) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            policy_id,
            order,
            page_size: page_size.max(1),
            assets: Vec::new(),
            seen: HashSet::new(),
            position: match order {
                Order::Rarity => Position::Offset(0),
                Order::Name => Position::Cursor(None),
            },
            total: None,
            exhausted: false,
            wanted: true,
            in_flight: false,
            error: None,
        }
    }

    pub fn policy_id(&self) -> &PolicyId {
        &self.policy_id
    }

    pub fn order(&self) -> Order {
        self.order
    }

    /// Ask for one more page. Idempotent until that page has been handed
    /// out by [`Self::next_request`].
    pub fn want_more(&mut self) {
        self.wanted = true;
    }

    /// After a failure: clear it, and want the page that failed again.
    pub fn retry(&mut self) {
        self.error = None;
        self.wanted = true;
    }

    /// The request for the next page, if one is wanted and possible.
    pub fn next_request(&mut self) -> Option<Get> {
        if !self.wanted || self.in_flight || self.exhausted || self.error.is_some() {
            return None;
        }
        self.wanted = false;
        self.in_flight = true;
        let mut url = format!(
            "{}/api/bundle/{}?limit={}&traits=decoded",
            self.base, self.policy_id, self.page_size
        );
        match &self.position {
            Position::Offset(0) | Position::Cursor(None) => {}
            Position::Offset(n) => url.push_str(&format!("&offset={n}")),
            Position::Cursor(Some(c)) => {
                url.push_str("&cursor=");
                url.push_str(&encode_component(c));
            }
        }
        if self.order == Order::Rarity {
            url.push_str("&sort=rarity");
        }
        Some(Get { url, bearer: None })
    }

    /// The outcome of the request [`Self::next_request`] handed out. Ignored
    /// when nothing is outstanding.
    pub fn on_response(&mut self, result: Result<&str, String>) {
        if !self.in_flight {
            return;
        }
        self.in_flight = false;
        let page: BundleResponse = match result.and_then(|body| {
            serde_json::from_str(body).map_err(|e| format!("unreadable page: {e}"))
        }) {
            Ok(page) => page,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };

        self.total = page.asset_count.or(self.total);
        let got = page.entries.len() as u32;
        for entry in &page.entries {
            if !self.seen.insert(entry.asset_name_hex.clone()) {
                continue;
            }
            if let Some(asset) = OwnedAsset::from_entry(&self.policy_id, entry) {
                self.assets.push(asset);
            }
        }

        match &mut self.position {
            Position::Offset(n) => {
                *n += got;
                // A short page is the last one; so is reaching the count.
                self.exhausted = got < self.page_size || self.total.is_some_and(|t| *n >= t);
            }
            Position::Cursor(c) => match page.next_cursor {
                // A cursor that does not move would fetch the same page
                // forever.
                Some(next) if c.as_deref() != Some(next.as_str()) => *c = Some(next),
                _ => self.exhausted = true,
            },
        }
    }

    /// Every asset so far, in the service's order.
    pub fn assets(&self) -> &[OwnedAsset] {
        &self.assets
    }

    /// How many assets the collection has, once a page has said.
    pub fn total(&self) -> Option<u32> {
        self.total
    }

    /// Every page is in.
    pub fn exhausted(&self) -> bool {
        self.exhausted
    }

    /// A page is in flight.
    pub fn loading(&self) -> bool {
        self.in_flight
    }

    /// Why the last page failed, until [`Self::retry`].
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// One collection the ownership service tracks.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CollectionSummary {
    pub policy_id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub asset_count: Option<u32>,
    #[serde(default)]
    pub holder_count: Option<u32>,
}

impl CollectionSummary {
    /// The label, or the policy id when there is none.
    pub fn name(&self) -> &str {
        self.label
            .as_deref()
            .filter(|l| !l.trim().is_empty())
            .unwrap_or(&self.policy_id)
    }
}

/// `GET /api/collections` on the ownership service.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CollectionList {
    pub policies: Vec<CollectionSummary>,
}

impl CollectionList {
    /// The request for the list.
    pub fn request(base: &str) -> Get {
        Get {
            url: format!("{}/api/collections", base.trim_end_matches('/')),
            bearer: None,
        }
    }

    /// Parse the list, sorted by name without regard to case.
    pub fn parse(body: &str) -> Result<Self, String> {
        let mut list: Self =
            serde_json::from_str(body).map_err(|e| format!("unreadable collection list: {e}"))?;
        list.policies.sort_by_key(|c| c.name().to_lowercase());
        Ok(list)
    }

    /// The collections whose name contains `filter`, or whose policy id
    /// starts with it, ignoring case. An empty filter matches everything.
    ///
    /// A prefix for the policy id, not a substring: two letters of a name
    /// would otherwise match somewhere inside most 56-character hex ids.
    pub fn matching<'a>(&'a self, filter: &str) -> impl Iterator<Item = &'a CollectionSummary> {
        let f = filter.trim().to_lowercase();
        self.policies.iter().filter(move |c| {
            f.is_empty() || c.name().to_lowercase().contains(&f) || c.policy_id.starts_with(&f)
        })
    }
}
