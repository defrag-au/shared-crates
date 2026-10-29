//! `IconCache` — each listed wallet's icon, on its way from a bridge to a
//! texture, kept once per wallet key.
//!
//! Shared by every chain's modal host: a bridge's icon request is asked once
//! per wallet, polled until it answers, and its RGBA becomes a texture the
//! first time it is drawn — which is the only step needing a graphics
//! context. The cache holds no bridge: the host asks and polls with its own,
//! and hands the cache the outcome, so one cache serves CIP-30 and EIP-6963
//! alike.

use macroquad::prelude::{FilterMode, Texture2D};
use wallet_miniquad::{PollResultDetailed, ReqId};

/// The square an icon is rasterised to. Twice the largest size the modal draws
/// one (56 points), so it stays sharp on a 2× display.
pub const ICON_PX: u16 = 128;

/// One wallet's icon.
enum Icon {
    Asked(ReqId),
    Pixels(Vec<u8>),
    Ready(Texture2D),
    /// None announced, or it would not load: the monogram stays.
    Missing,
}

#[derive(Default)]
pub struct IconCache {
    entries: Vec<(String, Icon)>,
}

impl IconCache {
    /// Ask for `key`'s icon unless it has been asked already. `ask` is the
    /// host's bridge call; `None` (no icons here) leaves the monogram.
    pub fn ask(&mut self, key: &str, ask: impl FnOnce() -> Option<ReqId>) {
        if self.entries.iter().any(|(k, _)| k == key) {
            return;
        }
        let icon = ask().map_or(Icon::Missing, Icon::Asked);
        self.entries.push((key.to_string(), icon));
    }

    /// The requests still waiting, for the host to poll.
    pub fn waiting(&self) -> Vec<ReqId> {
        self.entries
            .iter()
            .filter_map(|(_, icon)| match icon {
                Icon::Asked(req) => Some(*req),
                _ => None,
            })
            .collect()
    }

    /// What polling `req` produced: its pixels when it answered, `None` when
    /// it failed. `result` is the poll, `take` fetches the pixels of an `Ok`.
    pub fn settle(
        &mut self,
        req: ReqId,
        result: PollResultDetailed,
        take: impl FnOnce() -> Option<Vec<u8>>,
    ) {
        let next = match result {
            PollResultDetailed::Pending => return,
            PollResultDetailed::Ok { .. } => take().map_or(Icon::Missing, Icon::Pixels),
            PollResultDetailed::Err { .. } => Icon::Missing,
        };
        if let Some((_, icon)) = self
            .entries
            .iter_mut()
            .find(|(_, icon)| matches!(icon, Icon::Asked(r) if r.0 == req.0))
        {
            *icon = next;
        }
    }

    /// `key`'s texture, once its pixels have arrived. Makes the texture the
    /// first time, so this must run where a graphics context exists.
    pub fn texture(&mut self, key: &str) -> Option<Texture2D> {
        let (_, icon) = self.entries.iter_mut().find(|(k, _)| k == key)?;
        if let Icon::Pixels(rgba) = icon {
            let texture = Texture2D::from_rgba8(ICON_PX, ICON_PX, rgba);
            texture.set_filter(FilterMode::Linear);
            *icon = Icon::Ready(texture);
        }
        match icon {
            Icon::Ready(texture) => Some(texture.clone()),
            _ => None,
        }
    }

    /// Every key asked about, in order — for a test to see what was asked.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }
}
