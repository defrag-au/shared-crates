//! `Page` — a screen that scrolls as a whole and is laid out top to bottom:
//! an asset's detail, a settings list, a market summary.
//!
//! Such a page only learns how tall it is by drawing, so its scroll's end is
//! the height it came to last frame. [`Page::begin`] moves it and says where
//! to start laying out; [`Page::end`] records where the layout finished.
//!
//! ```ignore
//! let (mut y, tap) = page.begin(body, &input, p.tap, dt);
//! let top = y;
//! with_clip(body, || { /* lay out from y, hit-testing with `tap` */ });
//! page.end(top, y);
//! ```
//!
//! A grid of many cards uses [`crate::ArtGrid`] instead, which knows its
//! height before it draws.

use macroquad::prelude::{Rect, Vec2};
use macroquad_widgets::{Scroll, ScrollInput};

/// Room left under the last thing on a page, so it does not sit on the edge.
const FOOT: f32 = 24.0;

/// See the [module docs](self).
#[derive(Clone, Debug, Default)]
pub struct Page {
    scroll: Scroll,
    content: f32,
}

impl Page {
    pub fn new() -> Self {
        Self::default()
    }

    /// Move with this frame's `input`. Returns the top to lay out from, and
    /// `tap` if it is the page's: inside `area`, and not one that only caught
    /// the page coasting.
    pub fn begin(
        &mut self,
        area: Rect,
        input: &ScrollInput,
        tap: Option<Vec2>,
        dt: f32,
    ) -> (f32, Option<Vec2>) {
        let max = (self.content - area.h).max(0.0);
        self.scroll.update(area, input, Some(max), dt);
        let tap = self.scroll.filter_tap(tap).filter(|t| area.contains(*t));
        (area.y - self.scroll.offset(), tap)
    }

    /// The layout that began at `top` finished at `bottom`.
    pub fn end(&mut self, top: f32, bottom: f32) {
        self.content = bottom - top + FOOT;
    }

    /// How far the page has scrolled: what an app bar's lift reads.
    pub fn offset(&self) -> f32 {
        self.scroll.offset()
    }

    /// Back to the top, as a tab tapped while on it does.
    pub fn scroll_home(&mut self) {
        self.scroll.set(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 0.0,
        y: 100.0,
        w: 390.0,
        h: 600.0,
    };

    fn drag(y: f32) -> ScrollInput {
        ScrollInput {
            drag: Some(Vec2::new(200.0, y)),
            pointer: Vec2::new(200.0, y),
            wheel: Vec2::ZERO,
        }
    }

    #[test]
    fn a_page_scrolls_as_far_as_it_was_tall_last_frame() {
        let mut page = Page::new();
        let (top, _) = page.begin(AREA, &ScrollInput::none(), None, 0.0);
        page.end(top, top + 1000.0);
        // Drag a long way up: it stops at the content's end.
        page.begin(AREA, &drag(650.0), None, 1.0 / 60.0);
        page.begin(AREA, &drag(-900.0), None, 1.0 / 60.0);
        assert_eq!(page.offset(), 1000.0 + FOOT - AREA.h);
        let (top, _) = page.begin(AREA, &drag(-900.0), None, 1.0 / 60.0);
        assert_eq!(top, AREA.y - page.offset());
    }

    #[test]
    fn a_short_page_does_not_scroll_and_takes_taps_inside_only() {
        let mut page = Page::new();
        let (top, _) = page.begin(AREA, &ScrollInput::none(), None, 0.0);
        page.end(top, top + 200.0);
        let (_, inside) = page.begin(
            AREA,
            &ScrollInput::none(),
            Some(Vec2::new(10.0, 300.0)),
            0.0,
        );
        assert!(inside.is_some());
        let (_, outside) = page.begin(AREA, &ScrollInput::none(), Some(Vec2::new(10.0, 50.0)), 0.0);
        assert!(
            outside.is_none(),
            "a tap on the bar above is not the page's"
        );
        assert_eq!(page.offset(), 0.0);
    }
}
