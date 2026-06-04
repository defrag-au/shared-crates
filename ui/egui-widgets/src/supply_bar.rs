//! Two-band mint supply bar — `minted` (on chain) + `ordered` (the backlog of
//! ordered-but-not-yet-minted units), over the unsold track. Built for the mint
//! dashboard: at a glance you see **fulfilled vs ordered vs available**, and
//! **oversubscription** (ordered demand exceeding the remaining supply) is shown
//! distinctly rather than silently clamped to "looks full".
//!
//! All three inputs are in the SAME unit — asset-units against `total` (the
//! collection's supply). The host computes them (minted = on-chain slots; ordered
//! = active orders' demand not yet on chain).

use egui::{Color32, CornerRadius, Response, Sense, Ui, Vec2, Widget};

const TRACK: Color32 = Color32::from_rgb(30, 30, 44);
/// Default minted fill (neutral blue). The host overrides per status (e.g. green
/// when the collection is live).
const MINTED: Color32 = Color32::from_rgb(120, 160, 200);
/// Ordered backlog — muted amber: "queued, in progress".
const ORDERED: Color32 = Color32::from_rgb(190, 150, 90);
/// Oversubscribed ordered band — a hotter amber/red: "more ordered than supply".
const OVERSUB: Color32 = Color32::from_rgb(210, 120, 90);

/// A two-band supply progress bar. `minted` and `ordered` are clamped so their
/// bands never exceed the track; when `ordered` demand would overflow the
/// remaining supply the ordered band fills the remainder and is tinted (see
/// [`SupplyBar::is_oversubscribed`]).
pub struct SupplyBar {
    minted: u64,
    ordered: u64,
    total: u64,
    height: f32,
    rounding: u8,
    minted_color: Color32,
    ordered_color: Color32,
    oversub_color: Color32,
    track_color: Color32,
}

impl SupplyBar {
    /// `minted` on chain, `ordered` awaiting, `total` supply — all asset-units.
    pub fn new(minted: u64, ordered: u64, total: u64) -> Self {
        Self {
            minted,
            ordered,
            total,
            height: 4.0,
            rounding: 2,
            minted_color: MINTED,
            ordered_color: ORDERED,
            oversub_color: OVERSUB,
            track_color: TRACK,
        }
    }

    /// Bar height in px (default 4).
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    /// Override the minted-band fill (e.g. green for a live collection).
    pub fn minted_color(mut self, c: Color32) -> Self {
        self.minted_color = c;
        self
    }

    /// Override the ordered-band fill.
    pub fn ordered_color(mut self, c: Color32) -> Self {
        self.ordered_color = c;
        self
    }

    /// More units ordered than there is supply left to fill — the host can badge
    /// this (e.g. an "oversubscribed" chip) alongside the tinted band.
    pub fn is_oversubscribed(&self) -> bool {
        self.total > 0 && self.minted.saturating_add(self.ordered) > self.total
    }

    /// Allocate + paint, returning the bar's [`Response`].
    pub fn show(self, ui: &mut Ui) -> Response {
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), self.height), Sense::hover());
        if !ui.is_rect_visible(rect) {
            return resp;
        }
        let painter = ui.painter();
        let corner = CornerRadius::same(self.rounding);
        painter.rect_filled(rect, corner, self.track_color);
        if self.total == 0 {
            return resp;
        }
        let total = self.total as f32;
        let minted_pct = (self.minted as f32 / total).clamp(0.0, 1.0);
        // The ordered band sits AFTER minted; clamp so the two together never
        // exceed the track (the overflow is the oversubscription, flagged below).
        let ordered_pct = (self.ordered as f32 / total).clamp(0.0, 1.0 - minted_pct);
        let oversub = self.is_oversubscribed();

        // Ordered band first (middle segment, square corners) ...
        if ordered_pct > 0.0 {
            let mut band = rect;
            band.min.x = rect.min.x + rect.width() * minted_pct;
            band.set_width(rect.width() * ordered_pct);
            let c = if oversub {
                self.oversub_color
            } else {
                self.ordered_color
            };
            painter.rect_filled(band, CornerRadius::ZERO, c);
        }
        // ... then the minted band on top-left (keeps the bar's left rounding).
        if minted_pct > 0.0 {
            let mut filled = rect;
            filled.set_width(rect.width() * minted_pct);
            painter.rect_filled(filled, corner, self.minted_color);
        }
        resp
    }
}

impl Widget for SupplyBar {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}
