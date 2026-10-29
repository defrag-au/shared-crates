//! `Wall` — square tiles packed densely by priority, that shuffle with easing
//! when the priorities change.
//!
//! The host ranks its keys and gives each a side in cells: 1 for an ordinary
//! tile, 2 or 3 for one that should draw the eye. [`Wall::arrange`] packs them
//! ([`pack`]) and every tile eases from where it was to where it now belongs.
//! Raising one key's span is how a host highlights it: the wall makes room,
//! and the rest repack around it.
//!
//! - **Motion is in cells**, not pixels, so resizing the window re-lays the
//!   wall instantly while a shuffle still eases.
//! - **A shuffle ripples** outward from the first tile that moved, so the eye
//!   can follow it; a tile growing pops with a little overshoot and draws on
//!   top of the tiles passing under it.
//! - **Re-packs are rate-limited** ([`WallStyle::min_interval`]): a host can
//!   call `arrange` every frame and the wall still settles between shuffles.
//! - **[`Wall::pulse`]** pops one tile in place, with no re-pack: "this one
//!   just did something".
//!
//! What a tile shows is its own business ([`tile`]): each key has a [`Tile`],
//! and loaders are injected by the host. Without tiles, [`Wall::place`] hands
//! back where everything is, for a host that draws itself.
//!
//! Draw the wall before whatever frames it (a header, a panel's title): a
//! scrolled wall's top row slides under them rather than being clipped.

pub mod pack;
pub mod tile;

use std::collections::HashMap;
use std::f32::consts::PI;
use std::hash::Hash;

use macroquad::prelude::*;

use crate::ease::Ease;
use crate::profile;
use crate::tween::Animated;

pub use pack::{Cell, MAX_SPAN, clamp_span, pack};
pub use tile::{
    ColourTile, FnTile, ImageSource, ImageTile, ImageView, Layered, Tile, TileCtx, Tiles,
    VISIBLE_PRIORITY, shimmer,
};

/// The most a pulse widens a tile by, in stage units (half on each side).
const SWELL_UNITS: f32 = 5.0;

/// How a wall looks and moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallStyle {
    /// Between tiles, in stage units.
    pub gap: f32,
    /// Corner radius as a fraction of one cell's side.
    pub round: f32,
    /// Seconds a tile takes to slide to a new place.
    pub move_secs: f32,
    /// Seconds a tile takes to grow into more room.
    pub grow_secs: f32,
    /// Seconds an arriving tile takes to appear, and a leaving one to go.
    pub appear_secs: f32,
    pub leave_secs: f32,
    /// Seconds of delay per cell of distance from where a shuffle starts.
    pub ripple: f32,
    /// The longest any tile waits for the ripple to reach it.
    pub max_ripple: f32,
    /// The least time between two re-packs. An `arrange` sooner waits.
    pub min_interval: f32,
    /// Seconds a [`Wall::pulse`] lasts.
    pub pulse_secs: f32,
}

impl Default for WallStyle {
    fn default() -> Self {
        Self {
            gap: 6.0,
            round: 0.12,
            move_secs: 0.55,
            grow_secs: 0.6,
            appear_secs: 0.45,
            leave_secs: 0.25,
            ripple: 0.035,
            max_ripple: 0.35,
            min_interval: 0.9,
            pulse_secs: 0.45,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Motion {
    /// In cells: x, y, side, side.
    rect: Animated<Rect>,
    alpha: Animated<f32>,
    scale: Animated<f32>,
    lift: Animated<f32>,
    span: u8,
    leaving: bool,
    /// When it last grew, on the wall's clock: the most recent grower draws
    /// on top.
    grew_at: f32,
    /// When it was last pulsed.
    pulsed_at: f32,
}

/// A tile placed on the stage this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed<K> {
    pub key: K,
    pub ctx: TileCtx,
    /// Inside the wall's area: drawn.
    pub on_screen: bool,
    /// Within a row of it: worth loading.
    pub near: bool,
    pub leaving: bool,
}

pub struct Wall<K> {
    cols: u16,
    style: WallStyle,
    /// The asks last packed, in rank order.
    order: Vec<(K, u8)>,
    pending: Option<Vec<(K, u8)>>,
    motion: HashMap<K, Motion>,
    rows: u16,
    clock: f32,
    last_pack: Option<f32>,
}

impl<K: Clone + Eq + Hash> Wall<K> {
    pub fn new(cols: u16) -> Self {
        Self::with_style(cols, WallStyle::default())
    }

    pub fn with_style(cols: u16, style: WallStyle) -> Self {
        Self {
            cols: cols.max(1),
            style,
            order: Vec::new(),
            pending: None,
            motion: HashMap::new(),
            rows: 0,
            clock: 0.0,
            last_pack: None,
        }
    }

    pub fn style(&self) -> &WallStyle {
        &self.style
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// A new width in columns. Re-packs at once, eased, whatever the rate
    /// limit: a resize should not wait.
    pub fn set_cols(&mut self, cols: u16) {
        let cols = cols.max(1);
        if cols == self.cols {
            return;
        }
        self.cols = cols;
        let order = self.pending.take().unwrap_or_else(|| self.order.clone());
        self.apply(order);
    }

    /// The columns that fit `width` with cells at least `min_cell` wide.
    pub fn cols_for(width: f32, min_cell: f32, gap: f32) -> u16 {
        (((width + gap) / (min_cell + gap)).floor() as u16).max(1)
    }

    /// Rank the keys and give each a side in cells. The first call places
    /// them with a cascade; later calls shuffle, at most once per
    /// [`WallStyle::min_interval`] (a sooner call waits, and the latest wins).
    pub fn arrange(&mut self, items: impl IntoIterator<Item = (K, u8)>) {
        let items: Vec<(K, u8)> = items.into_iter().collect();
        if self.pending.is_none() && items == self.order {
            return;
        }
        if self.pending.as_ref() == Some(&items) {
            return;
        }
        let due = self
            .last_pack
            .is_none_or(|at| self.clock - at >= self.style.min_interval);
        if due {
            self.pending = None;
            self.apply(items);
        } else {
            self.pending = Some(items);
        }
    }

    /// Re-pack now, whatever the rate limit.
    pub fn arrange_now(&mut self, items: impl IntoIterator<Item = (K, u8)>) {
        self.pending = None;
        self.apply(items.into_iter().collect());
    }

    /// Pop one tile in place: a brief swell and glow, no re-pack.
    pub fn pulse(&mut self, key: &K) {
        if let Some(m) = self.motion.get_mut(key) {
            m.pulsed_at = self.clock;
        }
    }

    /// Advance the motion by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        let _s = profile::scope("wall.update");
        self.clock += dt.max(0.0);
        if self.pending.is_some()
            && self
                .last_pack
                .is_none_or(|at| self.clock - at >= self.style.min_interval)
            && let Some(items) = self.pending.take()
        {
            self.apply(items);
        }
        for m in self.motion.values_mut() {
            m.rect.update(dt);
            m.alpha.update(dt);
            m.scale.update(dt);
            m.lift.update(dt);
        }
        self.motion
            .retain(|_, m| !(m.leaving && m.alpha.settled() && m.alpha.get() <= 0.0));
    }

    /// Nothing is moving and no shuffle is waiting.
    pub fn settled(&self) -> bool {
        self.pending.is_none()
            && self.motion.values().all(|m| {
                m.rect.settled() && m.alpha.settled() && m.scale.settled() && m.lift.settled()
            })
            && self
                .motion
                .values()
                .all(|m| self.clock - m.pulsed_at >= self.style.pulse_secs)
    }

    /// Rows the packed wall uses.
    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// One cell's side on the stage, filling `width`.
    pub fn cell_size(&self, width: f32) -> f32 {
        let gaps = self.style.gap * f32::from(self.cols - 1);
        ((width - gaps) / f32::from(self.cols)).max(1.0)
    }

    /// The packed wall's height on the stage at `width`: what a scroll ranges
    /// over.
    pub fn content_height(&self, width: f32) -> f32 {
        let c = self.cell_size(width);
        (f32::from(self.rows) * (c + self.style.gap) - self.style.gap).max(0.0)
    }

    /// The scroll that puts the bottom row at the bottom of `area`.
    pub fn max_scroll(&self, area: Rect) -> f32 {
        (self.content_height(area.w) - area.h).max(0.0)
    }

    /// Where every tile is this frame, in draw order (leaving tiles first, the
    /// latest grower last), scrolled up by `scroll`.
    pub fn place(&self, area: Rect, scroll: f32) -> Vec<Placed<K>> {
        let _s = profile::scope("wall.place");
        let c = self.cell_size(area.w);
        let step = c + self.style.gap;
        let radius = c * self.style.round;
        let near_area = Rect::new(area.x, area.y - step, area.w, area.h + 2.0 * step);
        let mut out: Vec<(Placed<K>, (u8, f32))> = self
            .motion
            .iter()
            .map(|(key, m)| {
                let r = m.rect.get();
                let base = Rect::new(
                    area.x + r.x * step,
                    area.y - scroll + r.y * step,
                    r.w * step - self.style.gap,
                    r.h * step - self.style.gap,
                );
                let pulse = self.pulse_at(m);
                // A swell of 6%, but never more than a few units: a big tile
                // swelling by its share would spill past the wall's edge.
                let swell = 0.06f32.min(SWELL_UNITS / base.w.max(1.0));
                let scale = m.scale.get() * (1.0 + swell * pulse);
                let rect = scaled(base, scale);
                let ctx = TileCtx {
                    rect,
                    radius,
                    alpha: m.alpha.get().clamp(0.0, 1.0),
                    span: m.span,
                    lift: m.lift.get().max(pulse).clamp(0.0, 1.0),
                    time: self.clock,
                };
                let layer = if m.leaving {
                    0
                } else if ctx.lift > 0.0 {
                    2
                } else {
                    1
                };
                (
                    Placed {
                        key: key.clone(),
                        ctx,
                        on_screen: rect.overlaps(&area),
                        near: rect.overlaps(&near_area),
                        leaving: m.leaving,
                    },
                    (layer, m.grew_at.max(m.pulsed_at)),
                )
            })
            .collect();
        out.sort_by(|a, b| a.1.0.cmp(&b.1.0).then(a.1.1.total_cmp(&b.1.1)));
        out.into_iter().map(|(p, _)| p).collect()
    }

    /// The tile under `point`, topmost first. Leaving tiles take no taps.
    pub fn hit(&self, area: Rect, scroll: f32, point: Vec2) -> Option<K> {
        if !area.contains(point) {
            return None;
        }
        self.place(area, scroll)
            .into_iter()
            .rev()
            .find(|p| !p.leaving && p.ctx.rect.contains(point))
            .map(|p| p.key)
    }

    /// Advance every tile and let it ask its loaders for what it needs, the
    /// ones near the view first. Call from `Game::update`, after
    /// [`Wall::update`]: loading is update work, and a capture's fast-forward
    /// runs updates without draws.
    pub fn update_tiles<L: ?Sized>(
        &self,
        area: Rect,
        scroll: f32,
        tiles: &mut Tiles<K, L>,
        loaders: &mut L,
        dt: f32,
    ) {
        let _s = profile::scope("wall.update_tiles");
        for p in self.place(area, scroll) {
            if let Some(tile) = tiles.get_mut(&p.key) {
                profile::time("tile.update", || tile.update(dt, loaders, p.near));
            }
        }
    }

    /// Draw the tiles on screen, in order. A key with no tile is skipped.
    pub fn draw_tiles<L: ?Sized>(
        &self,
        area: Rect,
        scroll: f32,
        tiles: &mut Tiles<K, L>,
        loaders: &mut L,
    ) {
        let _s = profile::scope("wall.draw_tiles");
        for p in self.place(area, scroll).into_iter().filter(|p| p.on_screen) {
            if let Some(tile) = tiles.get_mut(&p.key) {
                profile::time("tile.draw", || tile.draw(p.ctx, loaders));
            }
        }
    }

    /// 0 to 1 to 0 over a pulse.
    fn pulse_at(&self, m: &Motion) -> f32 {
        let t = (self.clock - m.pulsed_at) / self.style.pulse_secs.max(1e-3);
        if (0.0..1.0).contains(&t) {
            (t * PI).sin()
        } else {
            0.0
        }
    }

    fn apply(&mut self, items: Vec<(K, u8)>) {
        let _s = profile::scope("wall.repack");
        let s = self.style;
        let spans: Vec<u8> = items.iter().map(|(_, span)| *span).collect();
        let cells = profile::time("wall.pack", || pack::pack(self.cols, &spans));
        self.rows = pack::rows(&cells);
        let first = self.last_pack.is_none();
        self.last_pack = Some(self.clock);

        // Where the shuffle starts: the first tile whose place changed.
        let origin = items
            .iter()
            .zip(&cells)
            .find(|((key, _), cell)| {
                self.motion
                    .get(key)
                    .is_none_or(|m| m.leaving || m.rect.target() != cell_rect(cell))
            })
            .map(|(_, cell)| *cell);

        let mut present: Vec<&K> = Vec::with_capacity(items.len());
        for (index, ((key, _), cell)) in items.iter().zip(&cells).enumerate() {
            present.push(key);
            let target = cell_rect(cell);
            let delay = if first {
                // The first arrangement cascades in, in rank order.
                (index as f32 * s.ripple * 0.5).min(s.max_ripple * 2.0)
            } else {
                origin.map_or(0.0, |o| {
                    let d = (i32::from(cell.col) - i32::from(o.col)).abs()
                        + (i32::from(cell.row) - i32::from(o.row)).abs();
                    (d as f32 * s.ripple).min(s.max_ripple)
                })
            };
            match self.motion.get_mut(key) {
                Some(m) => {
                    if m.leaving {
                        m.leaving = false;
                        m.alpha.to(1.0, s.appear_secs, Ease::CubicOut);
                        m.scale.to(1.0, s.appear_secs, Ease::BackOut);
                    }
                    if m.rect.target() != target {
                        if cell.span > m.span {
                            // Growing: into the room on the settle curve (an
                            // overshoot would carry it past the wall's edge),
                            // on top, glowing, and swelling as it lands.
                            let lands = delay * 0.5 + s.grow_secs * 0.55;
                            m.rect
                                .to_after(target, delay * 0.5, s.grow_secs, Ease::ExpoOut);
                            m.lift.set(1.0);
                            m.lift.to_after(0.0, lands, s.grow_secs, Ease::QuadOut);
                            m.grew_at = self.clock;
                            m.pulsed_at = self.clock + lands - s.pulse_secs * 0.5;
                        } else {
                            m.rect.to_after(target, delay, s.move_secs, Ease::ExpoOut);
                        }
                    }
                    m.span = cell.span;
                }
                None => {
                    let mut alpha = Animated::new(0.0);
                    alpha.to_after(1.0, delay, s.appear_secs, Ease::CubicOut);
                    let mut scale = Animated::new(0.7);
                    scale.to_after(1.0, delay, s.appear_secs, Ease::BackOut);
                    self.motion.insert(
                        key.clone(),
                        Motion {
                            rect: Animated::new(target),
                            alpha,
                            scale,
                            lift: Animated::new(0.0),
                            span: cell.span,
                            leaving: false,
                            grew_at: f32::NEG_INFINITY,
                            pulsed_at: f32::NEG_INFINITY,
                        },
                    );
                }
            }
        }

        // Keys no longer ranked leave.
        for (key, m) in self.motion.iter_mut() {
            if !m.leaving && !present.contains(&key) {
                m.leaving = true;
                m.alpha.to(0.0, s.leave_secs, Ease::QuadIn);
                m.scale.to(0.85, s.leave_secs, Ease::QuadIn);
            }
        }
        self.order = items;
    }
}

/// A cell as a rectangle in cell units.
fn cell_rect(cell: &Cell) -> Rect {
    let side = f32::from(cell.span);
    Rect::new(f32::from(cell.col), f32::from(cell.row), side, side)
}

/// `r` scaled about its centre.
fn scaled(r: Rect, k: f32) -> Rect {
    if (k - 1.0).abs() < 1e-4 {
        return r;
    }
    let c = r.center();
    let (w, h) = (r.w * k, r.h * k);
    Rect::new(c.x - w * 0.5, c.y - h * 0.5, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 300.0,
        h: 600.0,
    };

    fn run(wall: &mut Wall<u32>, secs: f32) {
        let steps = (secs / 0.016).ceil() as usize;
        for _ in 0..steps {
            wall.update(0.016);
        }
    }

    fn rect_of(wall: &Wall<u32>, key: u32) -> Rect {
        wall.place(AREA, 0.0)
            .into_iter()
            .find(|p| p.key == key)
            .expect("placed")
            .ctx
            .rect
    }

    #[test]
    fn tiles_cascade_in_and_settle_where_they_were_packed() {
        let mut wall = Wall::new(3);
        wall.arrange((0..6).map(|k| (k, 1)));
        assert!(!wall.settled(), "the first arrangement cascades in");
        run(&mut wall, 2.0);
        assert!(wall.settled());
        // 3 columns across 300 with 6-unit gaps: cells of 96.
        assert_eq!(wall.cell_size(300.0), 96.0);
        assert_eq!(rect_of(&wall, 4), Rect::new(102.0, 102.0, 96.0, 96.0));
        assert_eq!(wall.rows(), 2);
    }

    #[test]
    fn growing_a_tile_makes_room_and_the_others_move_with_easing() {
        let mut wall = Wall::new(3);
        wall.arrange((0..6).map(|k| (k, 1)));
        run(&mut wall, 2.0);
        let before = rect_of(&wall, 1);

        // Highlight tile 0: a 2x2.
        wall.arrange([(0, 2), (1, 1), (2, 1), (3, 1), (4, 1), (5, 1)]);
        // Two cells from where the shuffle starts, tile 1 waits for the
        // ripple, then slides.
        run(&mut wall, 0.05);
        assert_eq!(rect_of(&wall, 1), before, "the ripple has not reached it");
        run(&mut wall, 0.1);
        let moving = rect_of(&wall, 1);
        assert_ne!(moving, before, "it has started to move");
        run(&mut wall, 2.0);
        assert!(wall.settled());
        // Tile 1 now sits in the column beside the 2x2.
        assert_eq!(rect_of(&wall, 1).x, 204.0);
        assert_eq!(rect_of(&wall, 0).w, 96.0 * 2.0 + 6.0);
        assert!(moving.x > before.x && moving.x < 204.0, "eased, not jumped");
    }

    #[test]
    fn a_growing_tile_glows_and_draws_on_top() {
        let mut wall = Wall::new(3);
        wall.arrange((0..6).map(|k| (k, 1)));
        run(&mut wall, 2.0);
        wall.arrange([(0, 1), (1, 1), (2, 1), (3, 2), (4, 1), (5, 1)]);
        run(&mut wall, 0.1);
        let placed = wall.place(AREA, 0.0);
        let last = placed.last().expect("tiles");
        assert_eq!(last.key, 3, "the grower is drawn last");
        assert!(last.ctx.lift > 0.0);
    }

    #[test]
    fn shuffles_wait_for_the_last_to_settle_and_the_latest_ask_wins() {
        let mut wall = Wall::new(3);
        wall.arrange((0..6).map(|k| (k, 1)));
        run(&mut wall, 0.2);
        wall.arrange([(0, 2), (1, 1), (2, 1), (3, 1), (4, 1), (5, 1)]);
        wall.arrange([(5, 2), (1, 1), (2, 1), (3, 1), (4, 1), (0, 1)]);
        run(&mut wall, 0.3);
        assert!(!wall.settled(), "a shuffle is waiting its turn");
        run(&mut wall, 2.5);
        assert!(wall.settled());
        assert_eq!(rect_of(&wall, 5).w, 198.0, "the latest ask was packed");
    }

    #[test]
    fn a_key_left_out_fades_away_and_takes_no_taps() {
        let mut wall = Wall::new(3);
        wall.arrange((0..4).map(|k| (k, 1)));
        run(&mut wall, 2.0);
        wall.arrange((0..3).map(|k| (k, 1)));
        run(&mut wall, 0.05);
        let leaving = wall
            .place(AREA, 0.0)
            .into_iter()
            .find(|p| p.key == 3)
            .expect("still drawn");
        assert!(leaving.leaving && leaving.ctx.alpha < 1.0);
        assert_eq!(wall.hit(AREA, 0.0, leaving.ctx.rect.center()), None);
        run(&mut wall, 1.0);
        assert!(
            wall.place(AREA, 0.0).iter().all(|p| p.key != 3),
            "gone once faded"
        );
    }

    #[test]
    fn a_pulse_swells_a_tile_in_place_and_passes() {
        let mut wall = Wall::new(3);
        wall.arrange((0..3).map(|k| (k, 1)));
        run(&mut wall, 2.0);
        let rest = rect_of(&wall, 1);
        wall.pulse(&1);
        run(&mut wall, 0.2);
        let swollen = rect_of(&wall, 1);
        assert!(swollen.w > rest.w && (swollen.center() - rest.center()).length() < 0.01);
        run(&mut wall, 0.5);
        assert_eq!(rect_of(&wall, 1), rest);
    }

    #[test]
    fn taps_find_the_tile_under_them_through_a_scroll() {
        let mut wall = Wall::new(3);
        wall.arrange((0..30).map(|k| (k, 1)));
        run(&mut wall, 3.0);
        // Row 3 starts at 306; scrolled by 300 it is at the top of the area.
        assert_eq!(wall.hit(AREA, 300.0, vec2(10.0, 10.0)), Some(9));
        assert_eq!(wall.hit(AREA, 0.0, vec2(-5.0, 10.0)), None);
        assert_eq!(wall.max_scroll(AREA), wall.content_height(300.0) - 600.0);
    }

    #[test]
    fn only_tiles_near_the_view_are_loaded_and_only_those_in_it_drawn() {
        let mut wall = Wall::new(3);
        wall.arrange((0..60).map(|k| (k, 1)));
        run(&mut wall, 4.0);
        let placed = wall.place(AREA, 0.0);
        let on = placed.iter().filter(|p| p.on_screen).count();
        let near = placed.iter().filter(|p| p.near).count();
        // Rows are 102 apart, so 600 tall shows rows 0 to 5; row 6, just
        // below, is loaded but not drawn.
        assert_eq!(on, 18);
        assert_eq!(near, 21);
    }

    #[test]
    fn a_resize_repacks_at_once() {
        let mut wall = Wall::new(3);
        wall.arrange((0..6).map(|k| (k, 1)));
        run(&mut wall, 2.0);
        wall.set_cols(6);
        assert_eq!(wall.rows(), 1);
        assert_eq!(Wall::<u32>::cols_for(1000.0, 120.0, 6.0), 7);
    }
}
