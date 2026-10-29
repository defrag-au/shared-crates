//! What a wall tile draws, and where it gets what it draws from.
//!
//! The wall owns *where* a tile is; a [`Tile`] owns *what* it shows. Each key
//! on a wall has its own tile, so one wall can mix a static image, a live
//! ticker, an animated sprite and a plain colour, tile by tile.
//!
//! Loaders are injected, not owned. A tile is generic over the host's loader
//! context `L` — whatever bundle of loaders the host keeps — and asks it for
//! what it needs while it is on screen. The kit names the one interface its
//! own tiles use, [`ImageSource`]; `macroquad-cnft`'s `ArtLoader` implements
//! it for NFT art, and a host can implement it over anything (a fake with
//! latency for a capture, a sprite atlas, a texture baked at start).

use std::collections::HashMap;
use std::hash::Hash;

use macroquad::prelude::*;

use crate::shape::{fill_rounded, fill_texture_rounded};

/// Where and how a tile is drawn this frame. Copy, so a layered tile hands
/// the same context to each layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileCtx {
    /// On the stage, already moved, grown and popped by the wall.
    pub rect: Rect,
    pub radius: f32,
    /// 0 to 1: arriving or leaving tiles fade.
    pub alpha: f32,
    /// The side in cells it is settling at.
    pub span: u8,
    /// 0 to 1 while the tile is growing into more room, and while a host
    /// pulse is fading: a cue for a glow.
    pub lift: f32,
    /// Seconds on the wall's clock, for a tile's own animation.
    pub time: f32,
}

/// One tile's content.
pub trait Tile<L: ?Sized> {
    /// Each frame, in the update phase. `visible` says whether the tile is on
    /// screen (or about to be): a tile asks its loaders for what it needs
    /// only then, so a long wall loads what is seen first.
    fn update(&mut self, _dt: f32, _loaders: &mut L, _visible: bool) {}

    /// Draw into `ctx.rect`. Called only for tiles on screen.
    fn draw(&mut self, ctx: TileCtx, loaders: &mut L);
}

// ── Images ──────────────────────────────────────────────────────────────────

/// What an image source has for a URL.
pub enum ImageView<'a> {
    /// Asked for, not here yet.
    Loading,
    /// Will not arrive: draw the fallback.
    Failed,
    /// Here. `age` is seconds since it arrived, for fading it in.
    Ready { texture: &'a Texture2D, age: f32 },
}

/// Anything that turns a URL into a texture, eventually.
pub trait ImageSource {
    /// Ask for `url`; a LOWER `priority` is fetched sooner, as in
    /// `image-core`'s queue. Asking every frame is cheap and fetches once.
    fn request(&mut self, url: &str, priority: i32);
    fn view(&mut self, url: &str) -> ImageView<'_>;
}

/// The priority an on-screen image asks with. Lower is sooner, so
/// [`ImageTile::priority`] takes a smaller number for a tile that matters
/// more (a spotlight, say).
pub const VISIBLE_PRIORITY: i32 = 10;

/// An image, cover-cropped into the tile's rounded square. A tinted ground
/// with a slow shimmer while it loads; the image fades in when it arrives.
pub struct ImageTile {
    pub url: String,
    /// The ground under the image, and all there is if it fails.
    pub ground: Color,
    pub priority: i32,
    /// Seconds the image takes to fade in.
    pub fade: f32,
}

impl ImageTile {
    pub fn new(url: impl Into<String>, ground: Color) -> Self {
        Self {
            url: url.into(),
            ground,
            priority: VISIBLE_PRIORITY,
            fade: 0.3,
        }
    }

    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

impl<L: ImageSource + ?Sized> Tile<L> for ImageTile {
    fn update(&mut self, _dt: f32, loaders: &mut L, visible: bool) {
        if visible {
            loaders.request(&self.url, self.priority);
        }
    }

    fn draw(&mut self, ctx: TileCtx, loaders: &mut L) {
        let ground = Color {
            a: self.ground.a * ctx.alpha,
            ..self.ground
        };
        match loaders.view(&self.url) {
            ImageView::Ready { texture, age } => {
                let shown = if self.fade <= 0.0 {
                    1.0
                } else {
                    (age / self.fade).clamp(0.0, 1.0)
                };
                if shown < 1.0 {
                    fill_rounded(ctx.rect, ctx.radius, ground);
                }
                fill_texture_rounded(
                    texture,
                    ctx.rect,
                    ctx.radius,
                    Color::new(1.0, 1.0, 1.0, shown * ctx.alpha),
                );
            }
            ImageView::Loading => {
                fill_rounded(ctx.rect, ctx.radius, ground);
                shimmer(ctx);
            }
            ImageView::Failed => fill_rounded(ctx.rect, ctx.radius, ground),
        }
    }
}

/// A soft band of light crossing a tile that is waiting for its content.
pub fn shimmer(ctx: TileCtx) {
    let r = ctx.rect;
    let phase = (ctx.time * 0.7 + (r.x + r.y) * 0.002).rem_euclid(1.6);
    if phase >= 1.0 {
        return;
    }
    const SLICES: usize = 10;
    let band = r.w * 0.45;
    let x0 = r.x - band + phase * (r.w + band);
    let slice = band / SLICES as f32;
    let inset = ctx.radius.min(r.h * 0.5) * 0.3;
    for s in 0..SLICES {
        let left = (x0 + s as f32 * slice).max(r.x + inset);
        let right = (x0 + (s + 1) as f32 * slice).min(r.right() - inset);
        if right <= left {
            continue;
        }
        let mid = (s as f32 + 0.5) / SLICES as f32;
        let light = 1.0 - (mid - 0.5).abs() * 2.0;
        draw_rectangle(
            left,
            r.y + inset,
            right - left,
            r.h - 2.0 * inset,
            Color::new(1.0, 1.0, 1.0, 0.07 * light * light * ctx.alpha),
        );
    }
}

// ── Anything else ───────────────────────────────────────────────────────────

/// A tile drawn by a closure: the way to put anything live on a wall (a
/// ticker, a chart, a sprite on the host's clock) without a type for it.
pub struct FnTile<F>(pub F);

impl<L: ?Sized, F: FnMut(TileCtx, &mut L)> Tile<L> for FnTile<F> {
    fn draw(&mut self, ctx: TileCtx, loaders: &mut L) {
        (self.0)(ctx, loaders);
    }
}

/// Tiles drawn one over another into the same square: an image, then a
/// badge, then a ticker.
pub struct Layered<L: ?Sized>(pub Vec<Box<dyn Tile<L>>>);

impl<L: ?Sized> Tile<L> for Layered<L> {
    fn update(&mut self, dt: f32, loaders: &mut L, visible: bool) {
        for layer in &mut self.0 {
            layer.update(dt, loaders, visible);
        }
    }

    fn draw(&mut self, ctx: TileCtx, loaders: &mut L) {
        for layer in &mut self.0 {
            layer.draw(ctx, loaders);
        }
    }
}

/// A plain rounded colour: a placeholder, or a tile that is only a colour.
pub struct ColourTile(pub Color);

impl<L: ?Sized> Tile<L> for ColourTile {
    fn draw(&mut self, ctx: TileCtx, _loaders: &mut L) {
        let c = Color {
            a: self.0.a * ctx.alpha,
            ..self.0
        };
        fill_rounded(ctx.rect, ctx.radius, c);
    }
}

/// Each key's tile. The host fills it; [`super::Wall::update_tiles`] advances
/// them and [`super::Wall::draw_tiles`] draws the ones on screen.
pub struct Tiles<K, L: ?Sized> {
    tiles: HashMap<K, Box<dyn Tile<L>>>,
}

impl<K: Eq + Hash, L: ?Sized> Default for Tiles<K, L> {
    fn default() -> Self {
        Self {
            tiles: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash, L: ?Sized> Tiles<K, L> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: K, tile: impl Tile<L> + 'static) {
        self.tiles.insert(key, Box::new(tile));
    }

    pub fn insert_boxed(&mut self, key: K, tile: Box<dyn Tile<L>>) {
        self.tiles.insert(key, tile);
    }

    pub fn remove(&mut self, key: &K) {
        self.tiles.remove(key);
    }

    pub fn contains(&self, key: &K) -> bool {
        self.tiles.contains_key(key)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut Box<dyn Tile<L>>> {
        self.tiles.get_mut(key)
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
}
