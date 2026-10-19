//! `HoldingsLoader` — a player's holdings of one collection, fetched across frames.

use std::task::Poll;

use cnft_holdings::{Holdings, HoldingsLoad, LoadState, PolicyId, Source};

use crate::fetch::{TextRequest, get_text};

/// Drives a [`HoldingsLoad`] with real requests. Call [`Self::update`] once
/// per frame.
///
/// The paging, the parsing and the rules about what counts as a holding are
/// all in `cnft-holdings`; this only performs the requests it asks for.
pub struct HoldingsLoader {
    load: HoldingsLoad,
    in_flight: Option<TextRequest>,
}

impl HoldingsLoader {
    /// Start loading. The first request goes out on the first
    /// [`Self::update`].
    pub fn new(source: Source, policy_id: PolicyId) -> Self {
        Self {
            load: HoldingsLoad::new(source, policy_id),
            in_flight: None,
        }
    }

    /// Advance: collect a finished page, send the next one. Returns the state
    /// after this frame's work.
    pub fn update(&mut self) -> &LoadState {
        if let Some(req) = &mut self.in_flight
            && let Poll::Ready(result) = req.poll()
        {
            self.in_flight = None;
            match result {
                Ok(body) => self.load.on_response(Ok(&body)),
                Err(e) => self.load.on_response(Err(e.to_string())),
            }
        }
        if self.in_flight.is_none()
            && let Some(get) = self.load.next_request()
        {
            self.in_flight = Some(get_text(&get.url, get.bearer.as_deref()));
        }
        self.load.state()
    }

    pub fn state(&self) -> &LoadState {
        self.load.state()
    }

    /// Everything that has arrived so far, rarest first.
    pub fn holdings(&self) -> &Holdings {
        self.load.holdings()
    }

    pub fn is_loading(&self) -> bool {
        *self.load.state() == LoadState::Loading
    }
}
