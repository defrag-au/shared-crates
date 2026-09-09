//! `CornerAction` — a small icon button pinned to one corner of a thumbnail or
//! card, for a one-tap action on the thing it sits on (refresh, remove, pin).
//!
//! Painted over a rect the caller has already drawn — a `card_browser`
//! thumbnail, an image, a tile — so it takes no layout space. It is a real
//! widget, registered after the host, so egui's hit test gives *it* the
//! click and the host card underneath does not also toggle. At rest it is a
//! muted dark chip with an accent glyph, present but not competing with the
//! image; on hover the chip fills with the accent so the target is obvious.
//!
//! Corner badges that only *display* (an owned dot, a quantity) are not this
//! widget — see `offer_tile`. This one is for something the operator does.
//!
//! ## Hover-revealed? Gate on `contains_pointer()`, never `hovered()`
//!
//! It is natural to show this only while the host card is under the pointer.
//! Doing that with the host's `Response::hovered()` produces a control that
//! **cannot be clicked**, and the failure looks like a dead button rather than
//! a layout mistake:
//!
//! Because this is registered *after* the host, egui's hit test gives it the
//! pointer — which is what makes the click work, but also means the HOST's
//! `hovered()` goes false the instant the pointer reaches this chip.
//! `hovered()` respects occlusion. The gate then fails, the chip is not drawn
//! or interacted that frame, the host is hovered again the next frame, and the
//! chip flickers in and out under the cursor, never living long enough to
//! complete a press→release.
//!
//! `Response::contains_pointer()` is geometric and cannot be stolen by a child,
//! so it is the correct gate. This bit `listing_grid`'s add-to-cart.
//!
//! ## Example
//!
//! ```ignore
//! let resp = CornerAction::new(PhosphorIcon::ArrowsClockwise)
//!     .tooltip("Refresh image from on-chain metadata")
//!     .show(ui, ctx.thumb_rect, ("refresh-image", &asset_hex));
//! if resp.clicked() { /* dispatch */ }
//!
//! // Sit beside an existing badge in the same corner:
//! CornerAction::new(PhosphorIcon::X).shift(14.0).show(ui, rect, "remove");
//! ```

use std::hash::Hash;

use egui::{Align2, Color32, Pos2, Rect, Response, Sense, Ui, Vec2};

use crate::icons::PhosphorIcon;
use crate::theme;

/// Which corner of the host rect the button is pinned to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    /// Every corner, for stories and tests.
    pub const ALL: [Corner; 4] = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomLeft,
        Corner::BottomRight,
    ];
}

/// Builder for one corner button. See the module docs.
#[derive(Clone, Debug)]
pub struct CornerAction {
    icon: PhosphorIcon,
    corner: Corner,
    size: f32,
    inset: f32,
    shift: f32,
    accent: Color32,
    tooltip: Option<String>,
}

impl CornerAction {
    /// A 16pt button in the top-right corner, 4pt in from the edges,
    /// cyan-accented.
    pub fn new(icon: PhosphorIcon) -> Self {
        Self {
            icon,
            corner: Corner::TopRight,
            size: 16.0,
            inset: 4.0,
            shift: 0.0,
            accent: theme::ACCENT_CYAN,
            tooltip: None,
        }
    }

    pub fn corner(mut self, corner: Corner) -> Self {
        self.corner = corner;
        self
    }

    /// Side length of the square chip.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Gap between the chip and the host rect's edges.
    pub fn inset(mut self, inset: f32) -> Self {
        self.inset = inset;
        self
    }

    /// Move the chip *inward* along the horizontal edge, to clear a badge
    /// that already lives in this corner (an owned dot, a count pill).
    pub fn shift(mut self, shift: f32) -> Self {
        self.shift = shift;
        self
    }

    /// Glyph colour at rest, chip fill on hover.
    pub fn accent(mut self, accent: Color32) -> Self {
        self.accent = accent;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Where the chip lands on `host`. Pure, so placement is testable
    /// without a `Ui`.
    pub fn rect(&self, host: Rect) -> Rect {
        let size = Vec2::splat(self.size);
        let inset = self.inset;
        let min = match self.corner {
            Corner::TopLeft => Pos2::new(host.min.x + inset + self.shift, host.min.y + inset),
            Corner::TopRight => Pos2::new(
                host.max.x - inset - self.size - self.shift,
                host.min.y + inset,
            ),
            Corner::BottomLeft => Pos2::new(
                host.min.x + inset + self.shift,
                host.max.y - inset - self.size,
            ),
            Corner::BottomRight => Pos2::new(
                host.max.x - inset - self.size - self.shift,
                host.max.y - inset - self.size,
            ),
        };
        Rect::from_min_size(min, size)
    }

    /// Paint the chip over `host` and return its response. `id_salt` must be
    /// unique per host (an asset id), or every card shares one button.
    pub fn show(self, ui: &mut Ui, host: Rect, id_salt: impl Hash) -> Response {
        let rect = self.rect(host);
        let id = ui.id().with(("corner-action", id_salt));
        let response = ui.interact(rect, id, Sense::click());

        let (bg, fg) = if response.hovered() {
            (self.accent, theme::BG_PRIMARY)
        } else {
            (
                Color32::from_rgba_premultiplied(20, 21, 30, 200),
                self.accent,
            )
        };
        let painter = ui.painter();
        painter.rect_filled(rect, 3.0, bg);
        self.icon.paint(
            painter,
            rect.center(),
            Align2::CENTER_CENTER,
            (self.size * 0.7).round(),
            fg,
        );

        match self.tooltip {
            Some(tip) => response.on_hover_text(tip),
            None => response,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> Rect {
        Rect::from_min_size(Pos2::new(100.0, 200.0), Vec2::new(120.0, 120.0))
    }

    #[test]
    fn default_sits_inside_the_top_right() {
        let rect = CornerAction::new(PhosphorIcon::ArrowsClockwise).rect(host());
        assert_eq!(rect.max.x, 216.0);
        assert_eq!(rect.min.y, 204.0);
        assert_eq!(rect.size(), Vec2::splat(16.0));
        assert!(host().contains_rect(rect));
    }

    #[test]
    fn every_corner_stays_inside_the_host() {
        for corner in Corner::ALL {
            let rect = CornerAction::new(PhosphorIcon::X)
                .corner(corner)
                .rect(host());
            assert!(host().contains_rect(rect), "{corner:?} escaped the host");
        }
    }

    #[test]
    fn shift_moves_inward_on_both_sides() {
        let right = CornerAction::new(PhosphorIcon::X).shift(14.0).rect(host());
        assert_eq!(right.max.x, 202.0);
        let left = CornerAction::new(PhosphorIcon::X)
            .corner(Corner::TopLeft)
            .shift(14.0)
            .rect(host());
        assert_eq!(left.min.x, 118.0);
    }
}
