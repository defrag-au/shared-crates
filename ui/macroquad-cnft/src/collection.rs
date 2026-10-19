//! `BrowseLoader` and `CollectionsLoader` — a whole collection, and the list of collections, fetched across frames.

use std::task::Poll;

use cnft_holdings::{Browse, CollectionList, Order, PolicyId};

use crate::fetch::{TextRequest, get_text};

/// Drives a [`Browse`] with real requests. Call [`Self::update`] once per
/// frame; call [`Browse::want_more`] through [`Self::browse_mut`] when the
/// player nears the end of what has arrived.
pub struct BrowseLoader {
    browse: Browse,
    in_flight: Option<TextRequest>,
}

impl BrowseLoader {
    pub fn new(base: &str, policy_id: PolicyId, order: Order, page_size: u32) -> Self {
        Self {
            browse: Browse::new(base, policy_id, order, page_size),
            in_flight: None,
        }
    }

    /// Collect a finished page, send a wanted one.
    pub fn update(&mut self) {
        if let Some(req) = &mut self.in_flight
            && let Poll::Ready(result) = req.poll()
        {
            self.in_flight = None;
            match result {
                Ok(body) => self.browse.on_response(Ok(&body)),
                Err(e) => self.browse.on_response(Err(e.to_string())),
            }
        }
        if self.in_flight.is_none()
            && let Some(get) = self.browse.next_request()
        {
            self.in_flight = Some(get_text(&get.url, get.bearer.as_deref()));
        }
    }

    pub fn browse(&self) -> &Browse {
        &self.browse
    }

    pub fn browse_mut(&mut self) -> &mut Browse {
        &mut self.browse
    }
}

/// The ownership service's collection list, fetched once.
pub struct CollectionsLoader {
    in_flight: Option<TextRequest>,
    result: Option<Result<CollectionList, String>>,
}

impl CollectionsLoader {
    pub fn new(base: &str) -> Self {
        let get = CollectionList::request(base);
        Self {
            in_flight: Some(get_text(&get.url, None)),
            result: None,
        }
    }

    pub fn update(&mut self) {
        if let Some(req) = &mut self.in_flight
            && let Poll::Ready(result) = req.poll()
        {
            self.in_flight = None;
            self.result = Some(match result {
                Ok(body) => CollectionList::parse(&body),
                Err(e) => Err(e.to_string()),
            });
        }
    }

    /// `None` while loading.
    pub fn result(&self) -> Option<&Result<CollectionList, String>> {
        self.result.as_ref()
    }
}
