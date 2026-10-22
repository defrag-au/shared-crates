//! A player's holdings of one collection, and the paging that fetches them.

use cardano_assets::PolicyId;
use ownership_client::BundleResponse;

use crate::asset::OwnedAsset;
use crate::source::{Get, Source};

/// The most pages [`HoldingsLoad`] follows before calling the result done.
///
/// With a stake filter the service sends every row in one response, so a
/// second page is already unusual. The cap exists so a server that kept
/// returning a cursor could not keep a game loading forever.
pub const MAX_PAGES: usize = 20;

/// The collectibles a player holds of one policy, rarest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holdings {
    policy_id: PolicyId,
    assets: Vec<OwnedAsset>,
}

impl Holdings {
    pub fn new(policy_id: PolicyId) -> Self {
        Self {
            policy_id,
            assets: Vec::new(),
        }
    }

    pub fn policy_id(&self) -> &PolicyId {
        &self.policy_id
    }

    /// Fold one page in, returning its cursor for the next page.
    ///
    /// Non-collectibles (reference, royalty and fungible tokens) are dropped.
    /// An asset already held is replaced rather than duplicated, so a stake
    /// listed twice or overlapping pages cannot show one NFT twice.
    pub fn absorb(&mut self, page: BundleResponse) -> Option<String> {
        for entry in &page.entries {
            let Some(asset) = OwnedAsset::from_entry(&self.policy_id, entry) else {
                continue;
            };
            match self
                .assets
                .iter_mut()
                .find(|a| a.asset_name_hex == asset.asset_name_hex)
            {
                Some(existing) => *existing = asset,
                None => self.assets.push(asset),
            }
        }
        self.sort();
        page.next_cursor
    }

    /// Rarest first; assets without a rank after every ranked one. Ties
    /// break on the name, then on the hex, so the order is the same on every
    /// load. The service's own order is not stable.
    fn sort(&mut self) {
        self.assets.sort_by(|a, b| {
            let rank = |x: &OwnedAsset| x.rarity_rank.unwrap_or(u32::MAX);
            rank(a)
                .cmp(&rank(b))
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.asset_name_hex.cmp(&b.asset_name_hex))
        });
    }

    /// Every collectible held, rarest first.
    pub fn assets(&self) -> &[OwnedAsset] {
        &self.assets
    }

    /// The rarest asset held: the natural default champion.
    pub fn rarest(&self) -> Option<&OwnedAsset> {
        self.assets.first()
    }

    /// The held asset with this **owned** asset name.
    pub fn find(&self, asset_name_hex: &str) -> Option<&OwnedAsset> {
        self.assets
            .iter()
            .find(|a| a.asset_name_hex.eq_ignore_ascii_case(asset_name_hex))
    }

    pub fn len(&self) -> usize {
        self.assets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }
}

/// Where a [`HoldingsLoad`] has got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadState {
    /// Pages still to fetch, or one in flight.
    Loading,
    /// Every page is in. Possibly empty: holding nothing is an answer.
    Ready,
    /// A page failed. What arrived before it is kept but incomplete.
    Failed(String),
}

/// Fetching a player's holdings of one policy, page by page, with no I/O.
///
/// The edge drives it: take [`Self::next_request`], perform it, hand the
/// result to [`Self::on_response`], repeat while [`LoadState::Loading`]. One
/// request is outstanding at a time.
#[derive(Debug, Clone)]
pub struct HoldingsLoad {
    source: Source,
    holdings: Holdings,
    /// The page to ask for next; `None` with `pages == 0` is the first page.
    cursor: Option<String>,
    pages: usize,
    in_flight: bool,
    state: LoadState,
}

impl HoldingsLoad {
    pub fn new(source: Source, policy_id: PolicyId) -> Self {
        Self {
            source,
            holdings: Holdings::new(policy_id),
            cursor: None,
            pages: 0,
            in_flight: false,
            state: LoadState::Loading,
        }
    }

    /// The next request to perform, or `None` while one is in flight or the
    /// load is over. Each page is handed out exactly once.
    pub fn next_request(&mut self) -> Option<Get> {
        if self.in_flight || self.state != LoadState::Loading {
            return None;
        }
        self.in_flight = true;
        Some(
            self.source
                .page(self.holdings.policy_id(), self.cursor.as_deref()),
        )
    }

    /// Report the outcome of the request [`Self::next_request`] handed out:
    /// the response body on success, a message on failure.
    ///
    /// A response with nothing outstanding is ignored rather than trusted.
    pub fn on_response(&mut self, result: Result<&str, String>) {
        if !self.in_flight {
            return;
        }
        self.in_flight = false;

        let body = match result {
            Ok(body) => body,
            Err(message) => {
                self.state = LoadState::Failed(message);
                return;
            }
        };
        let page: BundleResponse = match serde_json::from_str(body) {
            Ok(page) => page,
            Err(e) => {
                self.state = LoadState::Failed(format!("unreadable holdings page: {e}"));
                return;
            }
        };

        self.pages += 1;
        let next = self.holdings.absorb(page);
        // A cursor equal to the one just used would ask for the same page
        // again, forever. Treat it as the end.
        let repeated = next.is_some() && next == self.cursor;
        match next {
            Some(cursor) if !repeated && self.pages < MAX_PAGES => self.cursor = Some(cursor),
            _ => self.state = LoadState::Ready,
        }
    }

    pub fn state(&self) -> &LoadState {
        &self.state
    }

    /// Everything that has arrived so far.
    pub fn holdings(&self) -> &Holdings {
        &self.holdings
    }

    /// How many pages have been folded in.
    pub fn pages(&self) -> usize {
        self.pages
    }
}
