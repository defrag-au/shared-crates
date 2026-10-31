//! `ArtGrid` — a scrolling grid of any number of cards, of which only the rows
//! in view are ever visited: a collection of ten thousand costs a frame what a
//! dozen does.
//!
//! The grid is geometry and a [`Scroll`]; the host draws each card. Columns
//! come from the width (as many as fit at [`GridSpec::min_cell`]), each cell is
//! square art over a caption band, and the content can open with a header the
//! host draws, which scrolls away with the grid (a summary above the cards).
//!
//! ```ignore
//! let geo = self.grid.update(area, &input, items.len(), spec, frame.dt);
//! for i in self.grid.visible(area, &geo, items.len(), 2) {
//!     art.request(&items[i].url, priority_for(i));   // load what is near
//! }
//! self.grid.draw(area, &geo, items.len(), |i, rect| card(&p, &items[i], rect));
//! if let Some(i) = self.grid.hit(area, &geo, items.len(), tap) { open(i) }
//! ```
//!
//! While the grid moves, a thin thumb on its right edge says where in the
//! whole the reader is, and fades once it settles.

use std::ops::Range;

use macroquad::prelude::*;
use macroquad_gamekit::clip::with_clip;
use macroquad_widgets::{Scroll, ScrollInput, fill_rounded, theme::with_alpha};

/// The grid's shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridSpec {
    /// The narrowest a card may be, in points: the columns are as many as fit.
    pub min_cell: f32,
    /// Between cards, and around the grid's edge.
    pub gap: f32,
    /// The caption band under each card's square art.
    pub caption: f32,
    /// A header above the first row, drawn by the host, that scrolls away.
    pub header: f32,
    /// The most columns, however wide the area.
    pub max_cols: usize,
}

impl Default for GridSpec {
    fn default() -> Self {
        Self {
            min_cell: 150.0,
            gap: 10.0,
            caption: 44.0,
            header: 0.0,
            max_cols: 8,
        }
    }
}

/// The grid laid out for a width and a count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridGeometry {
    pub cols: usize,
    /// One card: its width, and its art plus caption.
    pub cell: Vec2,
    /// The whole content's height, header included.
    pub content_h: f32,
    gap: f32,
    header: f32,
}

impl GridGeometry {
    /// Lay out `count` cards across `width`.
    pub fn new(width: f32, count: usize, spec: GridSpec) -> Self {
        let gap = spec.gap;
        let fit = ((width - gap) / (spec.min_cell + gap)).floor() as usize;
        let cols = fit.clamp(1, spec.max_cols.max(1));
        let w = ((width - gap * (cols as f32 + 1.0)) / cols as f32).max(1.0);
        let cell = vec2(w, w + spec.caption);
        let rows = count.div_ceil(cols);
        let content_h = spec.header + gap + rows as f32 * (cell.y + gap);
        Self {
            cols,
            cell,
            content_h,
            gap,
            header: spec.header,
        }
    }

    /// Where card `i` is, in content coordinates (0 at the header's top).
    pub fn cell_in_content(&self, i: usize) -> Rect {
        let (row, col) = (i / self.cols, i % self.cols);
        Rect::new(
            self.gap + col as f32 * (self.cell.x + self.gap),
            self.header + self.gap + row as f32 * (self.cell.y + self.gap),
            self.cell.x,
            self.cell.y,
        )
    }

    /// The rows overlapping content from `top` to `top + h`.
    fn rows_in(&self, top: f32, h: f32) -> Range<usize> {
        let step = self.cell.y + self.gap;
        let first = ((top - self.header - self.gap) / step).floor().max(0.0) as usize;
        let last = ((top + h - self.header) / step).ceil().max(0.0) as usize;
        first..last.max(first)
    }
}

/// See the [module docs](self).
#[derive(Clone, Debug, Default)]
pub struct ArtGrid {
    scroll: Scroll,
    /// Seconds since the grid last moved: how faded its thumb is.
    still: f32,
}

/// How long the thumb stays after the grid stops.
const THUMB_HOLD: f32 = 0.9;
const THUMB_FADE: f32 = 0.3;

impl ArtGrid {
    pub fn new() -> Self {
        Self {
            scroll: Scroll::new(),
            still: THUMB_HOLD + THUMB_FADE,
        }
    }

    /// The grid's scroll: its offset is what an app bar's `raised` reads.
    pub fn scroll(&self) -> &Scroll {
        &self.scroll
    }

    /// Back to the top, as a new filter or a tab tapped twice does.
    pub fn to_top(&mut self) {
        self.scroll.set(0.0);
    }

    /// Advance a frame: lay the grid out across `area` and move it with
    /// `input`. Behind a sheet, pass `ScrollInput::none()`.
    pub fn update(
        &mut self,
        area: Rect,
        input: &ScrollInput,
        count: usize,
        spec: GridSpec,
        dt: f32,
    ) -> GridGeometry {
        let geo = GridGeometry::new(area.w, count, spec);
        let moved = self
            .scroll
            .update(area, input, Some((geo.content_h - area.h).max(0.0)), dt);
        self.still = if moved || self.scroll.held() {
            0.0
        } else {
            self.still + dt
        };
        geo
    }

    /// The tap, unless it only caught the grid coasting. Hit-test with this.
    pub fn filter_tap(&self, tap: Option<Vec2>) -> Option<Vec2> {
        self.scroll.filter_tap(tap)
    }

    /// The cards in view, plus `overscan` rows either side: the ones worth
    /// loading art for.
    pub fn visible(
        &self,
        area: Rect,
        geo: &GridGeometry,
        count: usize,
        overscan: usize,
    ) -> Range<usize> {
        let rows = geo.rows_in(self.scroll.offset(), area.h);
        let first = rows.start.saturating_sub(overscan) * geo.cols;
        let last = ((rows.end + overscan) * geo.cols).min(count);
        first.min(last)..last
    }

    /// Where card `i` is on the stage this frame.
    pub fn cell(&self, area: Rect, geo: &GridGeometry, i: usize) -> Rect {
        let r = geo.cell_in_content(i);
        Rect::new(
            area.x + r.x,
            area.y + r.y - self.scroll.offset(),
            r.w,
            r.h,
        )
    }

    /// Where the header is on the stage this frame.
    pub fn header(&self, area: Rect, spec: GridSpec) -> Rect {
        Rect::new(area.x, area.y - self.scroll.offset(), area.w, spec.header)
    }

    /// Draw the cards in view, each by `card`, clipped to `area`, then the
    /// thumb while the grid moves. The header is the host's to draw, at
    /// [`ArtGrid::header`], inside the same clip if it wants one.
    pub fn draw(
        &self,
        area: Rect,
        geo: &GridGeometry,
        count: usize,
        thumb: Color,
        mut card: impl FnMut(usize, Rect),
    ) {
        with_clip(area, || {
            for i in self.visible(area, geo, count, 0) {
                card(i, self.cell(area, geo, i));
            }
        });
        let fade = 1.0 - ((self.still - THUMB_HOLD) / THUMB_FADE).clamp(0.0, 1.0);
        if fade > 0.0 && geo.content_h > area.h {
            let h = (area.h * area.h / geo.content_h).max(36.0);
            let max = geo.content_h - area.h;
            let y = area.y + (area.h - h) * (self.scroll.offset() / max).clamp(0.0, 1.0);
            let bar = Rect::new(area.right() - 5.0, y, 3.0, h);
            fill_rounded(bar, 1.5, with_alpha(thumb, 0.55 * fade));
        }
    }

    /// The card under `tap` (already through [`ArtGrid::filter_tap`]).
    pub fn hit(
        &self,
        area: Rect,
        geo: &GridGeometry,
        count: usize,
        tap: Option<Vec2>,
    ) -> Option<usize> {
        let tap = tap.filter(|t| area.contains(*t))?;
        self.visible(area, geo, count, 0)
            .find(|&i| self.cell(area, geo, i).contains(tap))
    }

    /// Scroll so card `i` is in view, as a restored selection is.
    pub fn reveal(&mut self, area: Rect, geo: &GridGeometry, i: usize) {
        let r = geo.cell_in_content(i);
        let top = self.scroll.offset();
        if r.y < top {
            self.scroll.set(r.y - geo.gap);
        } else if r.bottom() > top + area.h {
            self.scroll.set(r.bottom() + geo.gap - area.h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHONE: Rect = Rect {
        x: 0.0,
        y: 103.0,
        w: 390.0,
        h: 600.0,
    };

    #[test]
    fn a_phone_fits_two_columns_and_a_desk_more() {
        let spec = GridSpec::default();
        assert_eq!(GridGeometry::new(390.0, 100, spec).cols, 2);
        assert_eq!(GridGeometry::new(1200.0, 100, spec).cols, 7);
        assert_eq!(GridGeometry::new(100.0, 100, spec).cols, 1, "never none");
        let g = GridGeometry::new(390.0, 100, spec);
        assert!((g.cell.x * 2.0 + spec.gap * 3.0 - 390.0).abs() < 0.01);
    }

    #[test]
    fn only_the_rows_in_view_are_visited() {
        let spec = GridSpec::default();
        let grid = ArtGrid::new();
        let geo = GridGeometry::new(PHONE.w, 10_000, spec);
        let seen = grid.visible(PHONE, &geo, 10_000, 0);
        // 600 points of rows about 230 tall: three rows, two cards each.
        assert_eq!(seen, 0..6);
        let near = grid.visible(PHONE, &geo, 10_000, 2);
        assert_eq!(near, 0..10, "and two more rows below to load ahead");
    }

    #[test]
    fn the_last_row_is_cut_to_the_count() {
        let spec = GridSpec::default();
        let grid = ArtGrid::new();
        let geo = GridGeometry::new(PHONE.w, 3, spec);
        assert_eq!(grid.visible(PHONE, &geo, 3, 4), 0..3);
        assert_eq!(grid.visible(PHONE, &geo, 0, 4), 0..0);
    }

    #[test]
    fn a_header_pushes_the_cards_down_and_scrolls_away() {
        let spec = GridSpec {
            header: 120.0,
            ..GridSpec::default()
        };
        let mut grid = ArtGrid::new();
        let geo = GridGeometry::new(PHONE.w, 50, spec);
        assert_eq!(grid.cell(PHONE, &geo, 0).y, PHONE.y + 120.0 + spec.gap);
        grid.scroll.set(400.0);
        grid.update(PHONE, &ScrollInput::none(), 50, spec, 0.0);
        assert_eq!(grid.header(PHONE, spec).y, PHONE.y - 400.0);
    }

    #[test]
    fn a_tap_finds_its_card() {
        let spec = GridSpec::default();
        let grid = ArtGrid::new();
        let geo = GridGeometry::new(PHONE.w, 20, spec);
        let second = grid.cell(PHONE, &geo, 1);
        assert_eq!(grid.hit(PHONE, &geo, 20, Some(second.center())), Some(1));
        let gap = vec2(second.x - spec.gap * 0.5, second.center().y);
        assert_eq!(grid.hit(PHONE, &geo, 20, Some(gap)), None, "between cards");
    }

    #[test]
    fn reveal_scrolls_a_card_into_view() {
        let spec = GridSpec::default();
        let mut grid = ArtGrid::new();
        let geo = GridGeometry::new(PHONE.w, 200, spec);
        grid.reveal(PHONE, &geo, 99);
        grid.update(PHONE, &ScrollInput::none(), 200, spec, 0.0);
        let r = grid.cell(PHONE, &geo, 99);
        assert!(r.y >= PHONE.y && r.bottom() <= PHONE.bottom(), "{r:?}");
    }
}
