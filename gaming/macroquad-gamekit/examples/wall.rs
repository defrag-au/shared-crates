//! A shuffling wall: square tiles packed by priority, easing into place when
//! the priorities change. The regression capture for [`wall`].
//!
//! ```sh
//! cargo run -p macroquad-gamekit --example wall
//! cargo run -p macroquad-gamekit --example wall -- --shot .tmp/wall.png --size 390x844 --at 4
//! cargo run -p macroquad-gamekit --example wall -- --shot .tmp/wall-wide.png --size 1350x844 --at 4
//! ```
//!
//! Every tile chooses its own rendering:
//!
//! - most are **images** from an injected loader. Here it is a fake that
//!   paints a little face after a random latency, and fails one in thirteen,
//!   so the loading shimmer, the fade-in and the fallback are all on screen;
//! - every ninth is **live**: a closure tile counting up on the wall's clock;
//! - a few are plain **colours**.
//!
//! A spotlight moves to the next tile every three seconds: its span grows and
//! the wall repacks around it. Every so often a tile pulses in place. Wheel or
//! drag to scroll; tap a tile to spotlight it.
//!
//! `--tiles 228` sizes it like a real collection, and `--profile` adds where
//! each frame's time went to a capture's report:
//!
//! ```sh
//! cargo run --release -p macroquad-gamekit --example wall -- --shot .tmp/wall.png --size 1350x844 --at 4 --tiles 228 --profile
//! ```

use std::collections::HashMap;

use macroquad::prelude::*;
use macroquad_gamekit::harness::Args;
use macroquad_gamekit::wall::{
    ColourTile, FnTile, ImageSource, ImageTile, ImageView, Layered, TileCtx, Tiles, Wall, WallStyle,
};
use macroquad_gamekit::{Backdrop, Frame, Game, app};

const DESIGN: Vec2 = vec2(390.0, 844.0);
const COUNT: u32 = 72;
const SPOTLIGHT_SECS: f32 = 3.0;

/// A deterministic scramble, so captures are the same every run.
fn hash(n: u32) -> u32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x
}

fn hue(n: u32) -> Color {
    let h = (hash(n) % 360) as f32 / 360.0;
    macroquad::color::hsl_to_rgb(h, 0.62, 0.58)
}

// ── The injected loaders ────────────────────────────────────────────────────

/// A fake image source: an asked-for URL "arrives" after a latency from its
/// hash, as a painted texture, or fails.
#[derive(Default)]
struct FakeImages {
    asked: HashMap<String, f32>,
    ready: HashMap<String, (Texture2D, f32)>,
    failed: Vec<String>,
    clock: f32,
}

impl FakeImages {
    fn update(&mut self, clock: f32) {
        self.clock = clock;
        let due: Vec<String> = self
            .asked
            .iter()
            .filter(|(url, at)| clock - **at >= latency(url))
            .map(|(url, _)| url.clone())
            .collect();
        for url in due {
            self.asked.remove(&url);
            let n: u32 = url.trim_start_matches("fake://").parse().unwrap_or(0);
            if hash(n).is_multiple_of(13) {
                self.failed.push(url);
            } else {
                self.ready.insert(url, (paint_face(n), clock));
            }
        }
    }
}

fn latency(url: &str) -> f32 {
    let n: u32 = url.trim_start_matches("fake://").parse().unwrap_or(0);
    0.3 + (hash(n ^ 0xABCD) % 1700) as f32 / 1000.0
}

impl ImageSource for FakeImages {
    fn request(&mut self, url: &str, _priority: i32) {
        if !self.ready.contains_key(url) && !self.failed.iter().any(|f| f == url) {
            self.asked.entry(url.to_string()).or_insert(self.clock);
        }
    }

    fn view(&mut self, url: &str) -> ImageView<'_> {
        if let Some((texture, at)) = self.ready.get(url) {
            return ImageView::Ready {
                texture,
                age: self.clock - at,
            };
        }
        if self.failed.iter().any(|f| f == url) {
            ImageView::Failed
        } else {
            ImageView::Loading
        }
    }
}

/// The host's bundle of loaders: everything a tile may draw from. Image tiles
/// reach it through `ImageSource`; live tiles read whatever else it carries.
struct Loaders {
    images: FakeImages,
    /// Text is safe to draw this frame (the kit holds it back while the
    /// window settles).
    text: bool,
}

impl ImageSource for Loaders {
    fn request(&mut self, url: &str, priority: i32) {
        self.images.request(url, priority);
    }
    fn view(&mut self, url: &str) -> ImageView<'_> {
        self.images.view(url)
    }
}

/// A small face on a flat colour: something that reads as "art" at any size.
fn paint_face(n: u32) -> Texture2D {
    const S: u16 = 64;
    let bg = hue(n);
    let skin = hue(n.wrapping_add(7));
    let mut px = Vec::with_capacity(usize::from(S) * usize::from(S) * 4);
    for y in 0..S {
        for x in 0..S {
            let (fx, fy) = (f32::from(x) - 32.0, f32::from(y) - 36.0);
            let head = fx * fx + fy * fy < 22.0 * 22.0;
            let eye = |ex: f32| {
                let (dx, dy) = (fx - ex, fy + 4.0);
                dx * dx + dy * dy < 16.0
            };
            let c = if head && (eye(-8.0) || eye(8.0)) {
                Color::new(0.08, 0.08, 0.12, 1.0)
            } else if head {
                skin
            } else {
                bg
            };
            px.extend([
                (c.r * 255.0) as u8,
                (c.g * 255.0) as u8,
                (c.b * 255.0) as u8,
                255,
            ]);
        }
    }
    let t = Texture2D::from_rgba8(S, S, &px);
    t.set_filter(FilterMode::Linear);
    t
}

// ── The demo ────────────────────────────────────────────────────────────────

struct Demo {
    wall: Wall<u32>,
    tiles: Tiles<u32, Loaders>,
    loaders: Loaders,
    spotlight: u32,
    since: f32,
    scroll: f32,
    drag_from: Option<f32>,
    next_pulse: f32,
    area: Rect,
    dt: f32,
    /// Seconds run, from the frames' `dt`. The loader and the pulses keep
    /// this clock, as the wall keeps its own: a capture's settling frames
    /// can restart `Frame::clock`, and a loader timing against it would see
    /// time go backwards and wait for ever.
    clock: f32,
    /// How many tiles: `--tiles N`, 72 by default.
    count: u32,
}

impl Demo {
    fn new() -> Self {
        let count = macroquad_gamekit::harness::flag_num::<u32>("--tiles").unwrap_or(COUNT);
        let mut tiles: Tiles<u32, Loaders> = Tiles::new();
        for k in 0..count {
            if k % 9 == 4 {
                // A live tile: its own animation on the wall's clock.
                let ground = hue(k);
                tiles.insert(
                    k,
                    FnTile(move |ctx: TileCtx, loaders: &mut Loaders| live(ctx, loaders, ground)),
                );
            } else if k % 17 == 11 {
                tiles.insert(k, ColourTile(hue(k)));
            } else {
                // An image, with a ring drawn over it while it is lifted.
                tiles.insert(
                    k,
                    Layered(vec![
                        Box::new(ImageTile::new(
                            format!("fake://{k}"),
                            hue(k).with_alpha(0.35),
                        )),
                        Box::new(FnTile(|ctx: TileCtx, _: &mut Loaders| ring(ctx))),
                    ]),
                );
            }
        }
        Self {
            wall: Wall::with_style(3, WallStyle::default()),
            tiles,
            loaders: Loaders {
                images: FakeImages::default(),
                text: false,
            },
            spotlight: 0,
            since: 0.0,
            // `--scroll N`: start scrolled, for a capture of the clipped edge.
            scroll: macroquad_gamekit::harness::flag_num::<f32>("--scroll").unwrap_or(0.0),
            drag_from: None,
            next_pulse: 1.5,
            area: Rect::new(0.0, 0.0, 1.0, 1.0),
            dt: 0.0,
            clock: 0.0,
            count,
        }
    }

    /// The ranking: the spotlight first and biggest, every seventh tile
    /// boosted, the rest in key order.
    fn arrangement(&self) -> Vec<(u32, u8)> {
        let big = if self.wall.cols() >= 6 { 3 } else { 2 };
        let mut out = vec![(self.spotlight, big)];
        out.extend((0..self.count).filter(|k| *k != self.spotlight).map(|k| {
            (
                k,
                if k % 7 == 3 && self.wall.cols() >= 5 {
                    2
                } else {
                    1
                },
            )
        }));
        out
    }
}

/// A live tile: a counter climbing and a sweep going round.
fn live(ctx: TileCtx, loaders: &mut Loaders, ground: Color) {
    let r = ctx.rect;
    macroquad_gamekit::fill_rounded(r, ctx.radius, ground.with_alpha(0.25 * ctx.alpha));
    let c = r.center();
    let radius = r.w.min(r.h) * 0.3;
    let sweep = (ctx.time * 1.4).rem_euclid(std::f32::consts::TAU);
    for i in 0..24 {
        let a = i as f32 / 24.0 * std::f32::consts::TAU;
        let behind = (sweep - a).rem_euclid(std::f32::consts::TAU);
        let k = (1.0 - behind / std::f32::consts::TAU).powi(3);
        draw_circle(
            c.x + a.cos() * radius,
            c.y + a.sin() * radius,
            radius * 0.09,
            ground.with_alpha((0.25 + 0.75 * k) * ctx.alpha),
        );
    }
    if loaders.text {
        let n = format!("{:.2}", ctx.time * 3.7);
        let size = (r.w * 0.16).max(10.0);
        let m = measure_text(&n, None, size as u16, 1.0);
        draw_text(
            &n,
            c.x - m.width * 0.5,
            c.y + m.height * 0.4,
            size,
            WHITE.with_alpha(ctx.alpha),
        );
    }
}

/// A glow ring while a tile is lifted (growing, or pulsed).
fn ring(ctx: TileCtx) {
    if ctx.lift <= 0.01 {
        return;
    }
    let r = ctx.rect;
    let a = ctx.lift * ctx.alpha;
    macroquad_gamekit::stroke_rounded(r, ctx.radius, 3.0, Color::new(1.0, 0.85, 0.35, a));
    macroquad_gamekit::stroke_rounded(
        Rect::new(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0),
        ctx.radius + 3.0,
        2.0,
        Color::new(1.0, 0.85, 0.35, a * 0.35),
    );
}

impl Game for Demo {
    fn update(&mut self, frame: &Frame) {
        self.dt = frame.dt;
        self.clock += frame.dt;
        let b = frame.bounds;
        self.area = Rect::new(b.x + 12.0, b.y + 52.0, b.w - 24.0, b.h - 64.0);
        let style = *self.wall.style();
        self.wall
            .set_cols(Wall::<u32>::cols_for(self.area.w, 104.0, style.gap));

        // Scroll: wheel and drag.
        if self.area.contains(frame.pointer()) {
            self.scroll -= mouse_wheel().1 * 0.5;
        }
        match frame.input.gesture.drag {
            Some(at) => {
                if let Some(from) = self.drag_from {
                    self.scroll += from - at.y;
                }
                self.drag_from = Some(at.y);
            }
            None => self.drag_from = None,
        }
        // Not before the wall has rows: a `--scroll` asked for at the start
        // would be clamped to nothing.
        if self.wall.rows() > 0 {
            self.scroll = self.scroll.clamp(0.0, self.wall.max_scroll(self.area));
        }

        // A tap spotlights; otherwise the spotlight moves on by itself.
        self.since += frame.dt;
        if let Some(tap) = frame.input.gesture.tap
            && let Some(key) = self.wall.hit(self.area, self.scroll, tap)
        {
            self.spotlight = key;
            self.since = 0.0;
        } else if self.since >= SPOTLIGHT_SECS {
            self.spotlight = (self.spotlight + 1) % 12;
            self.since = 0.0;
        }
        self.wall.arrange(self.arrangement());

        // Now and then a tile pulses: "this one just did something".
        if self.clock >= self.next_pulse {
            let k = hash(frame.frames) % 18;
            self.wall.pulse(&k);
            self.next_pulse = self.clock + 0.7;
        }

        self.wall.update(frame.dt);
        // Tiles ask for what they need in the update phase, so loading
        // proceeds even through a capture's fast-forward, which never draws.
        self.wall.update_tiles(
            self.area,
            self.scroll,
            &mut self.tiles,
            &mut self.loaders,
            frame.dt,
        );
        self.loaders.images.update(self.clock);
        self.loaders.text = frame.text;
    }

    fn draw(&mut self, frame: &Frame) {
        clear_background(Color::from_hex(0x0b0c12));
        self.wall
            .draw_tiles(self.area, self.scroll, &mut self.tiles, &mut self.loaders);
        // No cover over the title: the wall clips its scrolled rows itself.
        let b = frame.bounds;
        if frame.text {
            draw_text(
                "gamekit wall",
                b.x + 14.0,
                b.y + 32.0,
                24.0,
                Color::from_hex(0xe3e8ff),
            );
        }
    }

    fn backdrop(&self) -> Backdrop {
        Backdrop {
            top: Color::from_hex(0x0b0c12),
            bottom: Color::from_hex(0x0b0c12),
            accent: Color::from_hex(0x9ece6a),
        }
    }

    fn capture_note(&self, frame: &Frame) -> String {
        format!(
            "wall · {} cols · {} rows · spotlight {} · settled {} · images {} ready, {} asked, {} failed · t={:.2}s",
            self.wall.cols(),
            self.wall.rows(),
            self.spotlight,
            self.wall.settled(),
            self.loaders.images.ready.len(),
            self.loaders.images.asked.len(),
            self.loaders.images.failed.len(),
            frame.clock
        )
    }
}

fn window_conf() -> Conf {
    let args = Args::read();
    let (w, h) = args.window_size((DESIGN.x as i32, DESIGN.y as i32));
    Conf {
        window_title: args.window_title("gamekit wall"),
        window_width: w,
        window_height: h,
        high_dpi: true,
        window_resizable: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    app::run(DESIGN, Demo::new()).await;
}
