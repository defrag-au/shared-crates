//! `TechPanel` — a workbench's panel for a tech tree, a tier at a time: tiles for the tier's nodes (built, ready, short or locked, and a server goal's fill), arrows between tiers, and a line about the picked one.
//!
//! The tree itself is shared-crates `arcade-tech`; this is only how a game
//! shows it. The game says what each tile is ([`TechTile`]) and how to write
//! text (its own font), and the panel lays out, draws, and answers what a
//! point is over ([`TechPanel::hit`]), so touch, a pad and keys all work it
//! the same way.
//!
//! A tier a page is what fits a phone: a tree of any depth shows one row of
//! choices at a time, three tiles across, at a size a thumb can tap.
//!
//! ```ignore
//! let layout = TechPanel::layout(canvas_w, canvas_h, tiles.len());
//! panel.draw(&layout, "WORKBENCH", "TIER 2 OF 4", &tiles, note, &TechStyle::default(), &label);
//! if let Some(TechHit::Tile(i)) = TechPanel::hit(&layout, touch) { /* build page tile i */ }
//! ```

use macroquad::prelude::*;

/// What a node is to the player, as a tile shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileState {
    /// Its needs are unmet.
    Locked,
    /// Reachable, but they carry too little.
    Short,
    /// They can build it, or give to it, now.
    Ready,
    /// Research done, or a goal filled.
    Done,
}

/// One tile: a node of the tier on show.
#[derive(Clone, Copy, Debug)]
pub struct TechTile<'a> {
    pub name: &'a str,
    pub state: TileState,
    /// The cost as the game writes it (`B3 G1`), or a goal's progress.
    pub cost: &'a str,
    /// A server goal's fill, 0 to 1; `None` for a player's own node.
    pub progress: Option<f32>,
}

/// The panel's colours.
#[derive(Clone, Copy, Debug)]
pub struct TechStyle {
    pub backdrop: Color,
    pub border: Color,
    pub title: Color,
    pub ready: Color,
    pub short: Color,
    pub locked: Color,
    pub done: Color,
    pub goal: Color,
    pub selected: Color,
    pub note: Color,
}

impl Default for TechStyle {
    /// A wooden bench by lamplight.
    fn default() -> Self {
        let hex = Color::from_hex;
        Self {
            backdrop: Color::new(0.10, 0.07, 0.04, 0.92),
            border: hex(0xb07a45),
            title: hex(0xffd23f),
            ready: hex(0xd8ffd0),
            short: hex(0xd8d8ff),
            locked: hex(0x6a6a8a),
            done: hex(0x9ad0ff),
            goal: hex(0xffb347),
            selected: hex(0xffd23f),
            note: hex(0xd8d8ff),
        }
    }
}

/// Where everything is, on one canvas, for one tier's tiles.
#[derive(Clone, Debug, PartialEq)]
pub struct TechLayout {
    pub panel: Rect,
    pub tiles: Vec<Rect>,
    /// The arrows to the tier before and after.
    pub prev: Rect,
    pub next: Rect,
    /// The centre of the line about the picked tile.
    pub note: Vec2,
}

/// What a point is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TechHit {
    /// A tile, by its place on the page.
    Tile(usize),
    Prev,
    Next,
}

/// The panel as the controls have it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TechPanel {
    /// The tier on show, as an index into the game's tiers.
    pub tier: usize,
    /// The tile a pad or keys have picked, by its place on the page.
    pub selected: Option<usize>,
    /// A tile refusing, and how far through its shake, 0 to 1.
    pub shake: Option<(usize, f32)>,
}

/// A tile's size, in canvas pixels: room for a name, a cost and a goal's
/// bar, and a thumb.
pub const TILE: (f32, f32) = (66.0, 36.0);
const COLS: usize = 3;
const GAP: f32 = 4.0;
const ARROW: (f32, f32) = (18.0, 12.0);

impl TechPanel {
    /// The panel on a `w` x `h` canvas for `tiles` tiles: three to a row, low
    /// on the screen but clear of its bottom edge.
    pub fn layout(w: f32, h: f32, tiles: usize) -> TechLayout {
        let rows = tiles.max(1).div_ceil(COLS);
        let (tw, th) = TILE;
        let pw = COLS as f32 * tw + (COLS - 1) as f32 * GAP + 12.0;
        let ph = rows as f32 * th + (rows - 1) as f32 * GAP + 32.0;
        let panel = Rect::new(((w - pw) / 2.0).floor(), (h - ph - 30.0).floor(), pw, ph);
        let tiles = (0..tiles)
            .map(|i| {
                let (col, row) = ((i % COLS) as f32, (i / COLS) as f32);
                Rect::new(
                    panel.x + 6.0 + col * (tw + GAP),
                    panel.y + 16.0 + row * (th + GAP),
                    tw,
                    th,
                )
            })
            .collect();
        let (aw, ah) = ARROW;
        TechLayout {
            prev: Rect::new(panel.x + 4.0, panel.y + 2.0, aw, ah),
            next: Rect::new(panel.x + panel.w - aw - 4.0, panel.y + 2.0, aw, ah),
            note: vec2(panel.x + panel.w / 2.0, panel.y + panel.h - 8.0),
            panel,
            tiles,
        }
    }

    /// What `point` is over, if anything on the panel.
    pub fn hit(layout: &TechLayout, point: Vec2) -> Option<TechHit> {
        if layout.prev.contains(point) {
            return Some(TechHit::Prev);
        }
        if layout.next.contains(point) {
            return Some(TechHit::Next);
        }
        layout
            .tiles
            .iter()
            .position(|t| t.contains(point))
            .map(TechHit::Tile)
    }

    /// Every area a touch should take, with what it is: for a game whose
    /// touch controls reserve areas (`Thumbs`).
    pub fn areas(layout: &TechLayout) -> Vec<(Rect, TechHit)> {
        let mut out = vec![(layout.prev, TechHit::Prev), (layout.next, TechHit::Next)];
        out.extend(
            layout
                .tiles
                .iter()
                .enumerate()
                .map(|(i, r)| (*r, TechHit::Tile(i))),
        );
        out
    }

    /// The tier `step` away from this one, among `tiers`, wrapping.
    pub fn turn(&mut self, step: i32, tiers: usize) {
        if tiers == 0 {
            return;
        }
        let n = tiers as i32;
        self.tier = (self.tier as i32 + step).rem_euclid(n) as usize;
        self.selected = None;
        self.shake = None;
    }

    /// Draw the panel. `label(text, centre, colour)` writes text centred on
    /// a point, in the game's own font.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        layout: &TechLayout,
        title: &str,
        tier: &str,
        tiles: &[TechTile],
        note: &str,
        style: &TechStyle,
        label: &dyn Fn(&str, Vec2, Color),
    ) {
        let r = layout.panel;
        draw_rectangle(r.x, r.y, r.w, r.h, style.backdrop);
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, style.border);
        label(
            &format!("{title}  {tier}"),
            vec2(r.x + r.w / 2.0, r.y + 8.0),
            style.title,
        );
        for (arrow, glyph) in [(layout.prev, "<"), (layout.next, ">")] {
            draw_rectangle_lines(arrow.x, arrow.y, arrow.w, arrow.h, 1.0, style.border);
            label(glyph, arrow.center(), style.title);
        }
        for (i, (rect, tile)) in layout.tiles.iter().zip(tiles).enumerate() {
            let shake = match self.shake {
                Some((s, f)) if s == i => ((f * 40.0).sin() * 3.0 * (1.0 - f)).round(),
                _ => 0.0,
            };
            let t = Rect::new(rect.x + shake, rect.y, rect.w, rect.h);
            let colour = match (tile.state, tile.progress.is_some()) {
                (TileState::Done, _) => style.done,
                (TileState::Locked, _) => style.locked,
                (_, true) => style.goal,
                (TileState::Ready, false) => style.ready,
                (TileState::Short, false) => style.short,
            };
            let fill = if tile.state == TileState::Ready {
                0.14
            } else {
                0.04
            };
            draw_rectangle(t.x, t.y, t.w, t.h, Color::new(1.0, 1.0, 1.0, fill));
            let ring = if self.selected == Some(i) {
                style.selected
            } else {
                Color::new(colour.r, colour.g, colour.b, 0.6)
            };
            draw_rectangle_lines(t.x, t.y, t.w, t.h, 1.0, ring);
            label(tile.name, vec2(t.x + t.w / 2.0, t.y + 8.0), colour);
            label(tile.cost, vec2(t.x + t.w / 2.0, t.y + 19.0), style.note);
            if let Some(fill) = tile.progress {
                let bar = Rect::new(t.x + 4.0, t.y + t.h - 8.0, t.w - 8.0, 4.0);
                draw_rectangle(bar.x, bar.y, bar.w, bar.h, Color::new(0.0, 0.0, 0.0, 0.5));
                let done = (bar.w * fill.clamp(0.0, 1.0)).floor();
                draw_rectangle(bar.x, bar.y, done, bar.h, style.goal);
            }
        }
        if !note.is_empty() {
            label(note, layout.note, style.note);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tier_is_three_across_and_fits_a_phone() {
        // A 3x iPhone in portrait is a 240-wide canvas at the least.
        let l = TechPanel::layout(240.0, 520.0, 5);
        assert_eq!(l.tiles.len(), 5);
        assert!(l.panel.x >= 0.0 && l.panel.right() <= 240.0);
        assert_eq!(l.tiles[0].y, l.tiles[2].y, "three to a row");
        assert!(l.tiles[3].y > l.tiles[0].y);
        assert!(l.panel.bottom() <= 520.0 - 30.0, "clear of the bottom edge");
    }

    #[test]
    fn a_point_is_over_a_tile_an_arrow_or_nothing() {
        let l = TechPanel::layout(320.0, 224.0, 4);
        assert_eq!(
            TechPanel::hit(&l, l.tiles[3].center()),
            Some(TechHit::Tile(3))
        );
        assert_eq!(TechPanel::hit(&l, l.prev.center()), Some(TechHit::Prev));
        assert_eq!(TechPanel::hit(&l, l.next.center()), Some(TechHit::Next));
        assert_eq!(TechPanel::hit(&l, vec2(0.0, 0.0)), None);
        assert_eq!(TechPanel::areas(&l).len(), 6);
    }

    #[test]
    fn turning_wraps_and_forgets_the_pick() {
        let mut p = TechPanel {
            tier: 0,
            selected: Some(2),
            shake: None,
        };
        p.turn(-1, 4);
        assert_eq!((p.tier, p.selected), (3, None));
        p.turn(1, 4);
        assert_eq!(p.tier, 0);
        p.turn(1, 0);
        assert_eq!(p.tier, 0, "no tiers, no turning");
    }
}
