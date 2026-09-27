//! What to follow.
//!
//! A socket can be joined to many collections at once, and each join is narrowed to
//! the event types that collection's consumer acts on — so a watch set is a list of
//! *collections*, each with its own filter, not one filter for the socket. That is
//! what the [crate docs](crate) measure: a busy collection carries most of its
//! volume as listings, bids and metadata churn, and taking everything a collection
//! emits is a volume problem rather than a relevance one.
//!
//! The other half of each watch is its **chain**, and it is stated rather than
//! derived. A stream slug identifies a *collection* and nothing in it names the
//! chain, so there is no table that could derive one from the other — the chain
//! comes from the config, and [`chain_ref`](crate::chain_ref) maps an incoming
//! payload's chain slug back to identity so the two can be checked against each
//! other.

use serde::{Deserialize, Serialize};

use chains::ChainRef;

use crate::event_type::EventType;
use crate::filter::EventFilter;
use crate::topic::Topic;

/// The event types an ownership ledger and the trait and image caches beside it
/// act on — the default for a watch that does not name its own.
///
/// - `item_transferred` and `item_sold` are the movements.
/// - `item_metadata_updated` carries the complete trait set and the image URL, and
///   is the only way the stream reports either changing.
///
/// Measured on the whole feed, these are a small fraction of what a collection
/// emits: `item_metadata_updated` alone was 73% of Robinhood Chain's stream traffic
/// in one sample, and a single collection produced ~600 frames/s unfiltered. Naming
/// the types you act on is what makes a collection affordable.
pub fn ownership_events() -> Vec<EventType> {
    vec![
        EventType::ItemTransferred,
        EventType::ItemSold,
        EventType::ItemMetadataUpdated,
    ]
}

/// One collection to follow, and which of its events to act on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionWatch {
    /// The stream slug — the **suffixed** form.
    ///
    /// Not the slug `/collections/{slug}` returns: measured on one socket,
    /// `collection:stonkbrokers` delivered 0 frames and
    /// `collection:stonkbrokers-434284142` delivered 41, both joining `ok`. A wrong
    /// slug is silent, so the config is where a slug that has been confirmed to
    /// deliver belongs.
    pub slug: String,
    /// Which chain the collection is on, as CAIP-2 — `"eip155:4663"`.
    ///
    /// Stated rather than derived. A collection slug does not carry its chain, and
    /// OpenSea's `/chains` cannot supply an id, so the only place this fact can come
    /// from is the config that names the collection.
    pub chain: ChainRef,
    /// Which event types to receive. Empty means every type the collection emits.
    #[serde(default = "ownership_events")]
    pub event_types: Vec<EventType>,
}

impl CollectionWatch {
    /// A collection, following the ownership and metadata events by default.
    pub fn new(slug: impl Into<String>, chain: ChainRef) -> Self {
        Self {
            slug: slug.into(),
            chain,
            event_types: ownership_events(),
        }
    }

    /// Narrows (or widens) the event types. An empty list is every type.
    pub fn events(mut self, event_types: impl IntoIterator<Item = EventType>) -> Self {
        self.event_types = event_types.into_iter().collect();
        self
    }

    /// The topic to join.
    pub fn topic(&self) -> Topic {
        Topic::collection(self.slug.clone())
    }

    /// The join filter.
    pub fn filter(&self) -> EventFilter {
        if self.event_types.is_empty() {
            EventFilter::All
        } else {
            EventFilter::only(self.event_types.clone())
        }
    }
}

/// The collections a follower watches.
///
/// Serialises to a plain list, so a config file is a `collections` array of objects
/// and nothing else:
///
/// ```json
/// {
///   "collections": [
///     { "slug": "madjacket-rh", "chain": "eip155:4663" }
///   ]
/// }
/// ```
///
/// An absent `event_types` is [`ownership_events`]; an empty one is every type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchSet {
    /// The collections.
    pub collections: Vec<CollectionWatch>,
}

impl WatchSet {
    /// No watch set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a collection.
    pub fn watch(mut self, collection: CollectionWatch) -> Self {
        self.collections.push(collection);
        self
    }

    /// How many collections.
    pub fn len(&self) -> usize {
        self.collections.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.collections.is_empty()
    }

    /// The joins to make, in order.
    pub fn subscriptions(&self) -> Vec<(Topic, EventFilter)> {
        self.collections
            .iter()
            .map(|collection| (collection.topic(), collection.filter()))
            .collect()
    }

    /// Which chain a topic's collection is on — how a delivered frame is routed.
    ///
    /// A frame names its topic, so this is the lookup that turns an incoming event
    /// into the chain its token reference needs.
    pub fn chain_of(&self, topic: &Topic) -> Option<ChainRef> {
        let Topic::Collection(slug) = topic else {
            return None;
        };
        self.collections
            .iter()
            .find(|collection| collection.slug == *slug)
            .map(|collection| collection.chain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chains::{CardanoNetwork, EvmChain};

    fn robinhood() -> ChainRef {
        ChainRef::Evm(EvmChain::Robinhood)
    }

    #[test]
    fn a_join_names_the_suffixed_slug_and_the_ownership_filter() {
        let watch = CollectionWatch::new("madjacket-rh", robinhood());
        assert_eq!(watch.topic().as_wire(), "collection:madjacket-rh");
        assert_eq!(
            watch.filter(),
            EventFilter::only([
                EventType::ItemTransferred,
                EventType::ItemSold,
                EventType::ItemMetadataUpdated,
            ])
        );
    }

    #[test]
    fn an_empty_event_list_is_every_type_rather_than_a_filter_that_matches_nothing() {
        // An empty `event_types` collapses to `{}` on the wire, which is "no
        // filter". Getting this backwards would be silent — the join answers `ok`
        // either way — so it is pinned here.
        let watch = CollectionWatch::new("x", robinhood()).events([]);
        assert_eq!(watch.filter(), EventFilter::All);
    }

    #[test]
    fn a_watch_set_writes_and_reads_as_a_plain_collection_list() {
        let set = WatchSet::new()
            .watch(CollectionWatch::new("madjacket-rh", robinhood()))
            .watch(
                CollectionWatch::new("stonkbrokers-434284142", robinhood())
                    .events([EventType::ItemTransferred]),
            );

        let json = serde_json::to_string(&set).unwrap();
        let read_back: WatchSet = serde_json::from_str(&json).unwrap();
        assert_eq!(read_back, set);
        assert_eq!(read_back.subscriptions().len(), 2);
    }

    #[test]
    fn a_config_that_omits_event_types_gets_the_ownership_three() {
        let set: WatchSet = serde_json::from_str(
            r#"{"collections":[{"slug":"madjacket-rh","chain":"eip155:4663"}]}"#,
        )
        .unwrap();

        assert_eq!(set.collections[0].event_types, ownership_events());
    }

    #[test]
    fn a_config_naming_an_unknown_chain_fails_rather_than_defaulting_to_cardano() {
        // `chains` refuses a chain it cannot name, and the refusal has to survive
        // the trip through this type — a default here would put a Robinhood
        // collection on mainnet Cardano.
        let error = serde_json::from_str::<WatchSet>(
            r#"{"collections":[{"slug":"x","chain":"solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp"}]}"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("chain"), "got {error}");
    }

    #[test]
    fn a_captured_event_type_name_reads_back_as_itself() {
        // The whole vocabulary round-trips, and a name this crate does not know is
        // kept rather than refused, so a config written before an OpenSea addition
        // still loads.
        let set: WatchSet = serde_json::from_str(
            r#"{"collections":[{"slug":"x","chain":"eip155:4663",
                "event_types":["item_sold","item_something_new"]}]}"#,
        )
        .unwrap();

        assert_eq!(
            set.collections[0].event_types,
            vec![
                EventType::ItemSold,
                EventType::Unknown("item_something_new".to_owned())
            ]
        );
    }

    #[test]
    fn a_topic_routes_back_to_its_chain() {
        let set = WatchSet::new().watch(CollectionWatch::new("madjacket-rh", robinhood()));
        assert_eq!(
            set.chain_of(&Topic::collection("madjacket-rh")),
            Some(robinhood())
        );
        assert_eq!(set.chain_of(&Topic::collection("elsewhere")), None);
        assert_eq!(set.chain_of(&Topic::AllCollections), None);
    }

    #[test]
    fn a_watch_may_name_a_cardano_collection_too() {
        // The stated purpose is Cardano collections with a Robinhood counterpart, so
        // a watch set holding both chains has to be representable — the chain is
        // what tells them apart.
        let set = WatchSet::new()
            .watch(CollectionWatch::new("madjacket-rh", robinhood()))
            .watch(CollectionWatch::new(
                "madjacket",
                ChainRef::Cardano(CardanoNetwork::Mainnet),
            ));
        assert_eq!(set.len(), 2);
        assert_eq!(
            set.chain_of(&Topic::collection("madjacket")),
            Some(ChainRef::Cardano(CardanoNetwork::Mainnet))
        );
    }
}
