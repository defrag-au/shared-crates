//! Paging a rendered result across message edits.
//!
//! A tool answers once, rendering every page it can afford to. The host
//! remembers those pages against a request id and swaps between them when the
//! reader clicks. This crate is the memory and the arithmetic; the host wires
//! it to its own storage and component routing.
//!
//! ## A page turn is not a request
//!
//! The obvious design re-invokes the tool with a page number, and it is wrong.
//! A page turn would then have to reproduce the original request exactly —
//! re-resolve the collection, re-resolve the trait, re-read config the model
//! never saw, re-authorise the caller — and every one of those is a chance to
//! land somewhere slightly different. The failures all look the same from
//! outside: a card that rendered perfectly, above a Next button that errors.
//!
//! Paging is a **view over a result already computed**. Storing the rendered
//! pages makes that structural rather than aspirational: there is nothing left
//! to resolve, so nothing left to resolve differently, and the list cannot
//! shift under the reader between page one and page five.
//!
//! What that buys, concretely:
//!
//! - a page turn costs no query, no round trip, and no permission re-check —
//!   the data was fetched and authorised once, when it was asked for
//! - the plugin holds no state and is never called again
//! - a snapshot has no credentials, no address and no arguments in it, so an
//!   expired or leaked one is a stale picture rather than a way to run
//!   anything
//!
//! ## None of it reaches a model
//!
//! A snapshot is addressed by id, never injected into a prompt. Turning a page
//! runs no model at all: the reader is looking at the list, and paying a model
//! to re-describe what is on screen would be paying twice.

use augie_plugin::PluginBlock;
use serde::{Deserialize, Serialize};
use worker_stack::worker::kv::KvStore;

/// How long a posted result stays pageable.
///
/// Matched to the agent's answer context deliberately: both answer "can the
/// reader still interact with this message", and two different lifetimes would
/// mean a reply that still works above buttons that no longer do.
pub const SNAPSHOT_TTL_SECONDS: u64 = 60 * 60 * 24;

fn snapshot_key(id: &str) -> String {
    format!("agent_snapshot:{id}")
}

/// The rendered pages of one answer, and what they are.
///
/// Note what is **not** here: no service, no address, no tool name, no
/// arguments. Nothing in a snapshot can be used to run anything — it is a
/// picture, and paging it is a presentation change.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Every page, in order. Page 0 is the one first posted.
    pub pages: Vec<Vec<PluginBlock>>,

    /// Total matching items, when the tool could say. `None` shows a page
    /// number with no denominator rather than one derived from a guess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,

    /// How many items were rendered, across every page.
    #[serde(default)]
    pub shown: u32,

    /// What this set is, for the pager line.
    #[serde(default)]
    pub label: String,
}

impl Snapshot {
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Did more match than was rendered? Derived from the two counts, so the
    /// card cannot claim one thing while its own numbers say another.
    pub fn is_truncated(&self) -> bool {
        self.total.is_some_and(|total| total > self.shown)
    }

    /// The blocks for `page`, or `None` past the end.
    pub fn page(&self, page: usize) -> Option<&Vec<PluginBlock>> {
        self.pages.get(page)
    }

    pub fn has_more_after(&self, page: usize) -> bool {
        page + 1 < self.pages.len()
    }
}

/// What a pager button does when clicked.
///
/// One variant, and deliberately so: everything a click can do is show a page
/// that already exists. There is no redraw, because a redraw would mean
/// re-running a query — which is the thing this design removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageAction {
    Show(usize),
}

/// A control the host should render and route.
///
/// Deliberately not a [`augie_plugin::PluginComponent`]: the host mints the
/// `custom_id`, because that id is how a click finds its way back to the
/// host's own storage. This crate decides *which* buttons and *what state*;
/// routing is not its business.
#[derive(Debug, Clone, PartialEq)]
pub struct PagerButton {
    pub action: PageAction,
    pub label: String,
    pub disabled: bool,
}

/// The controls for a page, or empty when there is nothing to page.
pub fn pager_buttons(snapshot: &Snapshot, page: usize) -> Vec<PagerButton> {
    // One page: a Previous and a Next that are both dead is worse than no
    // controls, because it implies there is somewhere to go.
    if snapshot.page_count() <= 1 {
        return Vec::new();
    }

    vec![
        PagerButton {
            // Saturating rather than checked: this button is disabled on page
            // 0 anyway, and an underflow would take out the whole reply.
            action: PageAction::Show(page.saturating_sub(1)),
            label: "Previous".to_string(),
            disabled: page == 0,
        },
        PagerButton {
            action: PageAction::Show(page + 1),
            label: "Next".to_string(),
            disabled: !snapshot.has_more_after(page),
        },
    ]
}

/// The line under a paged card: where you are, and in what.
///
/// Every number comes from data. A total the tool could not supply is simply
/// absent — "page 3 of 8" says more than "page 3", but only "page 3" is
/// honest when the size of the set is genuinely unknown.
pub fn pager_line(snapshot: &Snapshot, page: usize) -> String {
    let mut line = if snapshot.label.is_empty() {
        format!("page {} of {}", page + 1, snapshot.page_count())
    } else {
        format!(
            "{} · page {} of {}",
            snapshot.label,
            page + 1,
            snapshot.page_count()
        )
    };

    // Both counts, said plainly. Without this the last page reads as the end
    // of the list when it is only the end of what was fetched.
    match snapshot.total {
        Some(total) if snapshot.is_truncated() => {
            line.push_str(&format!(" · {} of {total} shown", snapshot.shown));
        }
        Some(total) => line.push_str(&format!(" · {total} in total")),
        None => {}
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

    fn page(n: usize) -> Vec<PluginBlock> {
        vec![PluginBlock::Text {
            content: format!("page {n}"),
        }]
    }

    /// Five items per page, all of them rendered — the complete case.
    fn snapshot(pages: usize, total: Option<u32>) -> Snapshot {
        Snapshot {
            pages: (0..pages).map(page).collect(),
            total,
            shown: total.unwrap_or((pages * 5) as u32),
            label: "Black Flag · Skin: Ghoul".to_string(),
        }
    }

    /// A page turn is a lookup, not a query. This is the whole design in one
    /// assertion: the pages are already here.
    #[test]
    fn a_page_is_read_not_fetched() {
        let snap = snapshot(3, Some(15));
        assert_eq!(snap.page(0), Some(&page(0)));
        assert_eq!(snap.page(2), Some(&page(2)));
        assert_eq!(
            snap.page(3),
            None,
            "past the end is None, not an empty page"
        );
    }

    #[test]
    fn the_ends_of_a_list_disable_the_direction_that_leads_nowhere() {
        let snap = snapshot(3, None);

        let first = pager_buttons(&snap, 0);
        assert!(first[0].disabled, "no previous page from the first");
        assert!(!first[1].disabled);
        assert_eq!(first[0].action, PageAction::Show(0), "saturates, not wraps");

        let last = pager_buttons(&snap, 2);
        assert!(!last[0].disabled);
        assert!(last[1].disabled, "no next page from the last");
    }

    /// A whole result gets no controls — two dead buttons imply somewhere to go.
    #[test]
    fn a_single_page_gets_no_controls() {
        assert!(pager_buttons(&snapshot(1, Some(3)), 0).is_empty());
        assert!(pager_buttons(&Snapshot::default(), 0).is_empty());
    }

    /// The denominator is the number of pages that EXIST, so it can never
    /// promise a page the snapshot cannot serve.
    #[test]
    fn the_page_total_counts_stored_pages() {
        let line = pager_line(&snapshot(16, Some(80)), 1);
        assert!(line.contains("page 2 of 16"), "{line}");
        assert!(line.contains("80 in total"), "{line}");
    }

    /// Truncation has to be visible, or the last page reads as the end of the
    /// list rather than the end of what was fetched. Both counts appear, so
    /// "30 of 80" cannot be misread as "80 shown".
    #[test]
    fn a_truncated_result_shows_both_counts() {
        let mut snap = snapshot(6, Some(80));
        snap.shown = 30;

        let line = pager_line(&snap, 5);
        assert!(line.contains("page 6 of 6"), "{line}");
        assert!(line.contains("30 of 80 shown"), "{line}");
        assert!(
            !line.contains("in total"),
            "that would claim completeness: {line}"
        );
    }

    #[test]
    fn an_unknown_total_shows_a_page_number_and_no_denominator() {
        let line = pager_line(&snapshot(4, None), 2);
        assert_eq!(line, "Black Flag · Skin: Ghoul · page 3 of 4");
    }

    /// The rendered pages ARE the stored result, so a lossy round trip would
    /// page to blank cards.
    #[test]
    fn a_snapshot_round_trips_as_json() {
        let snap = snapshot(3, Some(15));
        let back: Snapshot = serde_json::from_str(&serde_json::to_string(&snap).unwrap()).unwrap();
        assert_eq!(back, snap);
    }

    /// Nothing in a snapshot can run anything — no address, no tool, no
    /// arguments. An expired or leaked one is a stale picture, not a lever.
    #[test]
    fn a_snapshot_carries_no_way_to_invoke_anything() {
        let json = serde_json::to_string(&snapshot(2, Some(9))).unwrap();
        for lever in ["address", "binding", "tool", "arguments", "policy"] {
            assert!(!json.contains(lever), "snapshot leaks `{lever}`: {json}");
        }
    }
}
