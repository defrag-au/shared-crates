//! Paging a result set across message edits.
//!
//! A plugin renders one page and reports its shape ([`augie_plugin::PageInfo`]).
//! The **host** stores what a page turn needs, draws the controls, and calls
//! the plugin back for the page the user asked for. This crate is the middle
//! of that: the snapshot, and the arithmetic of which buttons to show.
//!
//! ## Why the host owns it
//!
//! A plugin cannot see the host's storage, and a page turn needs storage —
//! so a plugin that drew its own buttons would have to grow a store, a key
//! scheme and an expiry policy, and then every other plugin would grow its own
//! slightly different one. Paging is a property of *showing a list*, not of any
//! particular list, so it belongs where lists are shown.
//!
//! ## Store only what a re-run would destroy
//!
//! Most queries reproduce themselves. "The 5 rarest, skipping 10" is the same
//! five however often you ask, so its snapshot holds the *query* and each page
//! re-runs — which costs nothing to store and returns live data as a bonus.
//!
//! A random sample is the exception, and the reason [`Snapshot::order`] exists:
//! it is redrawn on every call, so page 2 of it would be an unrelated handful
//! rather than a later part of the same list. For those, and only those, the
//! drawn identifiers are stored and pages are slices of them.
//!
//! That asymmetry is also what makes a redraw meaningful to offer: a stored
//! deck can be thrown away and drawn again ([`PageAction::Redraw`]), where a
//! deterministic query has nothing to redraw.
//!
//! ## What never happens here
//!
//! **None of this reaches a model.** A snapshot is addressed by id, not
//! injected into a prompt: the model's view of a result stays the short summary
//! its tool returned. Paging is a UI affordance over data the host already
//! fetched, and spending prompt tokens on it would be paying twice for
//! something the user can already see.

use augie_plugin::ServiceAddress;
use serde::{Deserialize, Serialize};
use worker_stack::worker::kv::KvStore;

/// How long a posted list stays pageable.
///
/// Matched to the agent's answer context deliberately: both answer "can the
/// user still interact with this message", and two different lifetimes would
/// mean a reply that still works above buttons that no longer do.
pub const SNAPSHOT_TTL_SECONDS: u64 = 60 * 60 * 24;

fn snapshot_key(id: &str) -> String {
    format!("agent_snapshot:{id}")
}

/// Everything needed to render another page of a result already shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Service id, for logging and to find the guild's opt-in block.
    pub service: String,

    /// How to reach the plugin that renders a page.
    pub address: ServiceAddress,

    /// The tool to call — its plugin-local name, not the namespaced one the
    /// model sees.
    pub tool: String,

    /// The arguments that produced page 0, verbatim.
    ///
    /// Re-sent with a page selector added. Stored rather than rebuilt because
    /// they carry resolutions the host cannot redo — which collection a bare
    /// "the ghouls" meant, which of two ambiguous traits the user picked.
    pub arguments: serde_json::Value,

    pub page_size: usize,

    /// Total matching items, when the plugin could say. `None` shows a page
    /// number with no denominator rather than one derived from a guess.
    pub total: Option<u32>,

    /// The drawn order, for a set a re-run would not reproduce. Empty is the
    /// normal case — see the module docs.
    #[serde(default)]
    pub order: Vec<String>,

    /// What this set is, for the pager line.
    #[serde(default)]
    pub label: String,
}

impl Snapshot {
    /// Is this a stored deck rather than a re-runnable query?
    pub fn is_fixed_order(&self) -> bool {
        !self.order.is_empty()
    }

    /// The identifiers to render for `page`, when this is a stored deck.
    ///
    /// Empty past the end, which is what a Next click on a deck that shrank
    /// underneath it produces — the caller reports "past the last page" rather
    /// than rendering nothing and calling it a page.
    pub fn slice(&self, page: usize) -> &[String] {
        if self.page_size == 0 {
            return &[];
        }
        let start = page.saturating_mul(self.page_size).min(self.order.len());
        let end = start.saturating_add(self.page_size).min(self.order.len());
        &self.order[start..end]
    }

    /// Is there a page after `page`? Only knowable here for a stored deck;
    /// for a re-runnable query the plugin answers it on the next render.
    pub fn has_more_after(&self, page: usize) -> bool {
        self.is_fixed_order() && (page + 1) * self.page_size < self.order.len()
    }

    /// How many pages, when that is knowable without asking the plugin.
    pub fn page_count(&self) -> Option<usize> {
        if self.page_size == 0 {
            return None;
        }
        let items = if self.is_fixed_order() {
            self.order.len()
        } else {
            self.total? as usize
        };
        Some(items.div_ceil(self.page_size))
    }
}

/// What a pager button does when clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageAction {
    /// Show this page of the existing set.
    Show(usize),
    /// Throw the stored deck away and draw a new one.
    ///
    /// Only offered for a fixed order: re-running a deterministic query would
    /// produce the same list, so a button promising a fresh one would lie.
    Redraw,
}

/// A control the host should render and route.
///
/// Deliberately not a [`augie_plugin::PluginComponent`]: the host has to mint
/// the `custom_id`, because the id is how the click finds its way back to the
/// host's own storage. This crate decides *which* buttons and *what state*;
/// routing is not its business.
#[derive(Debug, Clone, PartialEq)]
pub struct PagerButton {
    pub action: PageAction,
    pub label: String,
    pub emoji: Option<String>,
    pub disabled: bool,
}

/// The controls for a page of a result, or empty when there is nothing to page.
///
/// `more` comes from the plugin's latest render rather than the snapshot,
/// because for a re-runnable query it is the only thing that knows.
pub fn pager_buttons(page: usize, more: bool, fixed_order: bool) -> Vec<PagerButton> {
    // One page and nothing after it: a Previous and a Next that are both dead
    // is worse than no controls, because it implies there is somewhere to go.
    if page == 0 && !more && !fixed_order {
        return Vec::new();
    }

    let mut buttons = vec![
        PagerButton {
            // Saturating rather than checked: this button is disabled on page
            // 0 anyway, and an underflow would take out the whole reply.
            action: PageAction::Show(page.saturating_sub(1)),
            label: "Previous".to_string(),
            emoji: None,
            disabled: page == 0,
        },
        PagerButton {
            action: PageAction::Show(page + 1),
            label: "Next".to_string(),
            emoji: None,
            disabled: !more,
        },
    ];

    if fixed_order {
        buttons.push(PagerButton {
            action: PageAction::Redraw,
            label: "Shuffle".to_string(),
            emoji: Some("🎲".to_string()),
            disabled: false,
        });
    }

    buttons
}

/// The line under a paged card: where you are, and in what.
///
/// Every number here comes from data. A total the plugin could not supply is
/// simply absent — "page 3" says less than "page 3 of 12" and is the only
/// honest thing to say when the size of the set is genuinely unknown.
pub fn pager_line(snapshot: &Snapshot, page: usize) -> String {
    let mut line = if snapshot.label.is_empty() {
        format!("page {}", page + 1)
    } else {
        format!("{} · page {}", snapshot.label, page + 1)
    };

    if let Some(pages) = snapshot.page_count() {
        line.push_str(&format!(" of {pages}"));
    }

    // A deck drawn from a larger pool has to say so, or "page 2 of 10" reads
    // as though ten pages is the whole collection.
    if snapshot.is_fixed_order() {
        let drawn = snapshot.order.len();
        match snapshot.total {
            Some(total) if total as usize > drawn => {
                line.push_str(&format!(" — {drawn} shuffled from {total}"));
            }
            _ => line.push_str(" — shuffled"),
        }
    }

    line
}

/// Record a pageable result. Best-effort: a failure costs the list its
/// controls, which is better than failing an answer that already rendered.
pub async fn store(kv: &KvStore, id: &str, snapshot: &Snapshot) -> bool {
    let put = match kv.put(&snapshot_key(id), snapshot) {
        Ok(put) => put,
        Err(e) => {
            tracing::warn!("snapshot serialize failed for {id}: {e}");
            return false;
        }
    };

    match put.expiration_ttl(SNAPSHOT_TTL_SECONDS).execute().await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("snapshot write failed for {id}: {e}");
            false
        }
    }
}

/// The result a pager click refers to, if it hasn't expired.
pub async fn load(kv: &KvStore, id: &str) -> Option<Snapshot> {
    match kv.get(&snapshot_key(id)).json().await {
        Ok(snapshot) => snapshot,
        Err(e) => {
            tracing::warn!("snapshot read failed for {id}: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(order: Vec<String>, total: Option<u32>) -> Snapshot {
        Snapshot {
            service: "collection-ownership".to_string(),
            address: ServiceAddress::Binding("COLLECTION_OWNERSHIP".to_string()),
            tool: "assets".to_string(),
            arguments: serde_json::Value::Null,
            page_size: 5,
            total,
            order,
            label: "Perps · Post It".to_string(),
        }
    }

    fn deck(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("asset{i}")).collect()
    }

    /// The case this whole crate exists for: a random sample is redrawn per
    /// call, so its pages have to be slices of one stored draw.
    #[test]
    fn a_stored_deck_pages_by_slicing_the_draw() {
        let snap = snapshot(deck(12), Some(420));

        assert_eq!(snap.slice(0), &deck(12)[0..5]);
        assert_eq!(snap.slice(1), &deck(12)[5..10]);
        // A short final page is a page, not an error.
        assert_eq!(snap.slice(2).len(), 2);

        assert!(snap.has_more_after(0));
        assert!(snap.has_more_after(1));
        assert!(!snap.has_more_after(2));
    }

    /// Clicking Next on a deck that shrank underneath you must not render an
    /// empty page as though it were a real one.
    #[test]
    fn paging_past_a_deck_yields_nothing_rather_than_wrapping() {
        let snap = snapshot(deck(7), None);
        assert!(snap.slice(2).is_empty());
        assert!(snap.slice(99).is_empty());
    }

    /// A re-runnable query stores no items, so the host must re-invoke rather
    /// than slice — `slice` returning empty is what signals that.
    #[test]
    fn a_rerunnable_query_stores_no_order() {
        let snap = snapshot(Vec::new(), Some(420));
        assert!(!snap.is_fixed_order());
        assert!(snap.slice(0).is_empty());
        // And `more` cannot be answered from the snapshot alone.
        assert!(!snap.has_more_after(0));
    }

    /// Page counts come from the deck when there is one, and from the total
    /// otherwise — never from the page currently on screen.
    #[test]
    fn page_count_prefers_the_deck_over_the_pool() {
        // 50 drawn from 420: ten pages of the deck, not 84 of the collection.
        assert_eq!(snapshot(deck(50), Some(420)).page_count(), Some(10));
        // No deck: the pool is what is being paged.
        assert_eq!(snapshot(Vec::new(), Some(420)).page_count(), Some(84));
        // Unknown pool and no deck: no denominator to offer.
        assert_eq!(snapshot(Vec::new(), None).page_count(), None);
        // Partial last page counts as a page.
        assert_eq!(snapshot(Vec::new(), Some(11)).page_count(), Some(3));
    }

    /// The display has to distinguish "ten pages of everything" from "ten
    /// pages of a sample", or it overstates what the user is looking at.
    #[test]
    fn a_shuffled_deck_says_what_it_was_drawn_from() {
        let line = pager_line(&snapshot(deck(50), Some(420)), 1);
        assert!(line.contains("page 2 of 10"), "{line}");
        assert!(line.contains("50 shuffled from 420"), "{line}");
    }

    #[test]
    fn an_unknown_total_shows_a_page_number_and_no_denominator() {
        let line = pager_line(&snapshot(Vec::new(), None), 2);
        assert_eq!(line, "Perps · Post It · page 3");
        assert!(!line.contains(" of "), "no invented total: {line}");
    }

    /// A whole result gets no controls — two dead buttons imply somewhere to go.
    #[test]
    fn a_single_page_gets_no_controls() {
        assert!(pager_buttons(0, false, false).is_empty());
    }

    #[test]
    fn the_ends_of_a_list_disable_the_direction_that_leads_nowhere() {
        let first = pager_buttons(0, true, false);
        assert_eq!(first.len(), 2);
        assert!(first[0].disabled, "no previous page from the first");
        assert!(!first[1].disabled);
        assert_eq!(first[0].action, PageAction::Show(0), "saturates, not wraps");

        let last = pager_buttons(3, false, false);
        assert!(!last[0].disabled);
        assert!(last[1].disabled, "no next page from the last");
        assert_eq!(last[0].action, PageAction::Show(2));
    }

    /// Redraw is offered only where it means something. A deterministic query
    /// re-run gives the same list back, so the button would be a lie.
    #[test]
    fn only_a_stored_deck_can_be_redrawn() {
        let shuffled = pager_buttons(0, true, true);
        assert!(shuffled.iter().any(|b| b.action == PageAction::Redraw));

        let deterministic = pager_buttons(0, true, false);
        assert!(!deterministic.iter().any(|b| b.action == PageAction::Redraw));
    }

    /// A one-page deck still offers a redraw: there is nowhere to page, but
    /// drawing a different sample is exactly what the user would want.
    #[test]
    fn a_single_page_deck_can_still_be_reshuffled() {
        let buttons = pager_buttons(0, false, true);
        assert!(!buttons.is_empty(), "a deck always has a redraw");
        assert!(buttons.iter().any(|b| b.action == PageAction::Redraw));
        assert!(buttons[1].disabled, "but Next still leads nowhere");
    }

    #[test]
    fn a_snapshot_round_trips_as_json() {
        let snap = snapshot(deck(3), Some(9));
        let back: Snapshot = serde_json::from_str(&serde_json::to_string(&snap).unwrap()).unwrap();
        assert_eq!(back, snap);
    }

    /// A degenerate page size must not divide by zero or spin.
    #[test]
    fn a_zero_page_size_is_survivable() {
        let mut snap = snapshot(deck(3), Some(9));
        snap.page_size = 0;
        assert!(snap.slice(0).is_empty());
        assert_eq!(snap.page_count(), None);
    }
}
