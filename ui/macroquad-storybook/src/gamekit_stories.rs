//! Stories for `macroquad-gamekit`: easing curves, a fanned card hand you can
//! hover, drag and long-press, rich text with inline icons, hit effects,
//! rounded art, the preloader's cover lifting off a screen, and a shuffling
//! wall of tiles to drive by hand.
//!
//! Every story is driven by the storybook's clock (`dt` and `t`), so P pauses,
//! `.` steps a frame and S slows time: the way to judge a 150 ms settle. With
//! `--shot` the clock runs at a fixed 1/60 s, and `--at <seconds>` captures at
//! an exact moment, so a capture is repeatable.
//!
//! Card names and text here are sample strings for judging layout, not game
//! content.

use macroquad::prelude::*;
use macroquad_gamekit::wall::{
    ColourTile, FnTile, ImageSource, ImageTile, ImageView, Layered, TileCtx, Tiles,
};
use macroquad_gamekit::{
    Align, Animated, Baked, Ease, HoverGrace, Icon, MacroquadMeasure, Particles, Pointer,
    PointerConfig, PointerEvent, Popups, Pose, Preloader, RichText, RoundedArt, Sequencer, Shake,
    Sprite, TextStyle, draw_icon, draw_order, emblem, fill_rounded, gem, line, plate,
    shrink_to_fit, stroke_rounded, topmost,
};
use macroquad_widgets::{Gesture, Painter};
use ui_theme::TextSize;

/// Which kit story this is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KitView {
    Easing,
    Hand,
    Text,
    Fx,
    Art,
    Preload,
    Face,
    Wall,
}

/// Card size in points. // theme-exempt: geometry, not text
const CARD: Vec2 = Vec2::new(120.0, 168.0);
/// The drag threshold is this fraction of the card's height.
const DRAG_FRACTION: f32 = 0.05;
const LONG_PRESS: f32 = 0.45;
const SETTLE: f32 = 0.15;
const HOVER_GRACE: f32 = 0.14;
const ICON_ENERGY: u32 = 1;
const ICON_GUARD: u32 = 2;

/// Sample cards: a name, a cost, and body text as (text, emphasised) runs with
/// an optional icon after the run.
struct Sample {
    name: &'static str,
    cost: u32,
    body: &'static [(&'static str, bool, Option<u32>)],
}

const SAMPLES: [Sample; 6] = [
    Sample {
        name: "Strike",
        cost: 1,
        body: &[
            ("Deal ", false, None),
            ("6", true, None),
            (" damage.", false, None),
        ],
    },
    Sample {
        name: "Brace",
        cost: 1,
        body: &[
            ("Gain ", false, None),
            ("5", true, Some(ICON_GUARD)),
            (" until your next turn.", false, None),
        ],
    },
    Sample {
        name: "Overclock",
        cost: 0,
        body: &[
            ("Gain ", false, None),
            ("2", true, Some(ICON_ENERGY)),
            (". Draw 1 card.", false, None),
        ],
    },
    Sample {
        name: "Heavy Swing With A Long Name",
        cost: 2,
        body: &[
            ("Deal ", false, None),
            ("12", true, None),
            (
                " damage. If the enemy is guarding, deal it twice.",
                false,
                None,
            ),
        ],
    },
    Sample {
        name: "Patch Up",
        cost: 1,
        body: &[
            ("Heal ", false, None),
            ("4", true, None),
            (".", false, None),
        ],
    },
    Sample {
        name: "Salvage",
        cost: 1,
        body: &[
            ("Draw ", false, None),
            ("2", true, None),
            (". Strip 1 from your hand.", false, None),
        ],
    },
];

struct Card {
    sample: usize,
    sprite: Sprite,
}

struct Drag {
    card: usize,
    grab: Vec2,
}

enum Cue {
    Deal,
}

struct Hand {
    faces: Vec<Baked>,
    big_faces: Vec<Baked>,
    cards: Vec<Card>,
    flying: Vec<Card>,
    pointer: Pointer,
    hover: HoverGrace<usize>,
    drag: Option<Drag>,
    detail: Option<usize>,
    seq: Sequencer<Cue>,
    next_sample: usize,
}

struct FxStage {
    shake: Shake,
    popups: Popups,
    particles: Particles,
    pulse: Animated<f32>,
    last_fire: f64,
}

/// A preloader over a stand-in screen, restarted every few seconds.
struct PreloadStage {
    preloader: Preloader,
    started: f64,
}

/// Seconds the stand-in screen takes to be ready, and between restarts.
const PRELOAD_READY: f64 = 1.6;
const PRELOAD_CYCLE: f64 = 4.0;

pub struct KitStory {
    pub view: KitView,
    art: RoundedArt,
    textures: Vec<Texture2D>,
    hand: Option<Hand>,
    fx: FxStage,
    preload: PreloadStage,
    /// Made on first draw: only the wall story pays for its tiles.
    wall: Option<WallStage>,
}

impl KitStory {
    pub fn new(view: KitView, textures: Vec<Texture2D>) -> Self {
        Self {
            view,
            art: RoundedArt::new(),
            textures,
            hand: None,
            fx: FxStage {
                shake: Shake::new(18.0, 1.6),
                popups: Popups::new(0.9, 46.0),
                particles: Particles::new(11, 520.0),
                pulse: Animated::new(1.0),
                last_fire: -10.0,
            },
            preload: PreloadStage {
                preloader: Preloader::new(),
                started: 0.0,
            },
            wall: None,
        }
    }

    /// Draw the story in `area`. `dt` and `t` come from the storybook clock.
    pub fn draw(&mut self, p: &Painter, area: Rect, g: &Gesture, dt: f32, t: f64) {
        match self.view {
            KitView::Easing => easing(p, area, t),
            KitView::Hand => self.hand(p, area, g, dt),
            KitView::Text => text_story(p, area),
            KitView::Fx => self.fx(p, area, g, dt, t),
            KitView::Art => self.art_story(p, area),
            KitView::Preload => self.preload_story(p, area, g, dt, t),
            KitView::Face => face_story(p, area),
            KitView::Wall => self.wall_story(p, area, g, dt, t),
        }
    }

    // ── preload ─────────────────────────────────────────────────────────

    /// The cover and spinner while a stand-in screen "loads", then the lift.
    fn preload_story(&mut self, p: &Painter, area: Rect, g: &Gesture, dt: f32, t: f64) {
        let stage = &mut self.preload;
        if g.tap.is_some_and(|at| area.contains(at)) || t - stage.started >= PRELOAD_CYCLE {
            stage.preloader = Preloader::new();
            stage.started = t;
        }
        let ready = t - stage.started >= PRELOAD_READY;
        stage.preloader.update(dt, ready);

        // The screen underneath, drawn whole.
        fill_rounded(area, 12.0, p.theme.color.bg_secondary);
        p.text(
            "the screen, drawn whole (tap to load again)",
            area.x + 16.0,
            area.y + 24.0,
            p.size(TextSize::Sm),
            p.theme.color.text_muted,
        );
        for (i, width) in [0.72, 0.55, 0.64].into_iter().enumerate() {
            let bar = Rect::new(
                area.x + 16.0,
                area.y + 52.0 + i as f32 * 34.0,
                (area.w - 32.0) * width,
                20.0,
            );
            fill_rounded(bar, 6.0, p.theme.color.bg_highlight);
        }
        stage.preloader.draw(
            area,
            p.theme.color.bg_secondary,
            p.theme.color.bg_primary,
            p.theme.color.accent,
        );
    }

    // ── hand ────────────────────────────────────────────────────────────

    fn hand(&mut self, p: &Painter, area: Rect, g: &Gesture, dt: f32) {
        if self.hand.is_none() {
            let dpi = screen_dpi_scale();
            let faces = (0..SAMPLES.len())
                .map(|i| bake_card(p, &self.art, &self.textures, i, dpi * 1.25))
                .collect();
            let big_faces = (0..SAMPLES.len())
                .map(|i| bake_card(p, &self.art, &self.textures, i, dpi * 2.0))
                .collect();
            let mut hand = Hand {
                faces,
                big_faces,
                cards: Vec::new(),
                flying: Vec::new(),
                pointer: Pointer::new(PointerConfig {
                    drag_threshold: CARD.y * DRAG_FRACTION,
                    long_press: LONG_PRESS,
                }),
                hover: HoverGrace::new(HOVER_GRACE),
                drag: None,
                detail: None,
                seq: Sequencer::new(),
                next_sample: 0,
            };
            for _ in 0..5 {
                hand.seq.push(Cue::Deal);
            }
            self.hand = Some(hand);
        }
        let Some(h) = &mut self.hand else { return };

        let deck_at = vec2(area.x + 50.0, area.y + area.h - 70.0);
        let play_line = area.y + area.h - CARD.y * 1.35;
        let mouse = Vec2::from(mouse_position());

        // Input.
        for ev in h.pointer.update(g, dt) {
            match ev {
                PointerEvent::DragStarted { from, .. } if h.detail.is_none() => {
                    if let Some(i) = topmost(h.cards.iter().map(|c| &c.sprite), from) {
                        let pos = h.cards[i].sprite.pose.get().pos;
                        h.drag = Some(Drag {
                            card: i,
                            grab: from - pos,
                        });
                        h.hover.clear();
                    }
                }
                PointerEvent::Dragged { at } => {
                    if let Some(d) = &h.drag
                        && let Some(c) = h.cards.get_mut(d.card)
                    {
                        c.sprite.pose.set(Pose::at(at - d.grab).scaled(1.1));
                        c.sprite.z = 1000;
                    }
                }
                PointerEvent::Released { at, dragged: true } => {
                    if let Some(d) = h.drag.take()
                        && d.card < h.cards.len()
                    {
                        if at.y < play_line {
                            let mut c = h.cards.remove(d.card);
                            let target = vec2(area.x + area.w / 2.0, area.y + 60.0);
                            c.sprite.go(
                                Pose::at(target).scaled(0.6).faded(0.0),
                                0.24,
                                Ease::CubicIn,
                            );
                            h.flying.push(c);
                            h.seq.push(Cue::Deal);
                        } else {
                            h.cards[d.card].sprite.z = 0;
                        }
                    }
                }
                PointerEvent::LongPress { at } if h.detail.is_none() => {
                    h.detail =
                        topmost(h.cards.iter().map(|c| &c.sprite), at).map(|i| h.cards[i].sample);
                }
                PointerEvent::Tap { .. } if h.detail.is_some() => h.detail = None,
                _ => {}
            }
        }
        let hit = if h.drag.is_none() && h.detail.is_none() {
            topmost(h.cards.iter().map(|c| &c.sprite), mouse)
        } else {
            None
        };
        let hovered = h.hover.update(hit, dt);

        // Cues: deal from the deck, one at a time. Dealing does not wait for
        // the hand to stop moving: the next card's arrival is the movement.
        if let Some(Cue::Deal) = h.seq.poll(dt, false) {
            let sample = h.next_sample % SAMPLES.len();
            h.next_sample += 1;
            h.cards.push(Card {
                sample,
                sprite: Sprite::new(CARD, Pose::at(deck_at).scaled(0.4).rotated(-0.6).faded(0.0)),
            });
            h.seq.hold(0.09);
        }

        // Layout: every card not in hand-drag glides to its slot.
        let slots = fan(h.cards.len(), hovered, area);
        for (i, c) in h.cards.iter_mut().enumerate() {
            if h.drag.as_ref().is_some_and(|d| d.card == i) {
                continue;
            }
            let (pose, z) = slots[i];
            c.sprite.z = z;
            if c.sprite.pose.target() != pose {
                c.sprite.go(pose, SETTLE, Ease::ExpoOut);
            }
        }
        for c in h.cards.iter_mut().chain(h.flying.iter_mut()) {
            c.sprite.update(dt);
        }
        h.flying.retain(|c| !c.sprite.settled());

        // Draw.
        fill_rounded(area, 12.0, p.theme.color.bg_secondary);
        draw_line(
            area.x + 12.0,
            play_line,
            area.x + area.w - 12.0,
            play_line,
            1.0,
            Color {
                a: 0.35,
                ..p.theme.color.accent
            },
        );
        let hint = p.size(TextSize::Sm);
        p.text(
            "release above this line to play",
            area.x + 16.0,
            play_line - 8.0,
            hint,
            p.theme.color.text_muted,
        );
        p.text(
            "hover · drag up to play · hold to inspect",
            area.x + 16.0,
            area.y + 24.0,
            hint,
            p.theme.color.text_muted,
        );
        let deck = Rect::new(deck_at.x - 26.0, deck_at.y - 36.0, 52.0, 72.0);
        fill_rounded(deck, 6.0, p.theme.color.bg_highlight);
        stroke_rounded(deck, 6.0, 2.0, p.theme.color.border);

        let sprites: Vec<Sprite> = h.cards.iter().map(|c| c.sprite).collect();
        for i in draw_order(&sprites) {
            let c = &h.cards[i];
            h.faces[c.sample].draw(c.sprite.pose.get(), WHITE);
        }
        for c in &h.flying {
            h.faces[c.sample].draw(c.sprite.pose.get(), WHITE);
        }

        if let Some(s) = h.detail {
            draw_rectangle(
                area.x,
                area.y,
                area.w,
                area.h,
                Color::new(0.0, 0.0, 0.0, 0.6),
            );
            let centre = vec2(area.x + area.w / 2.0, area.y + area.h / 2.0);
            h.big_faces[s].draw(Pose::at(centre).scaled(1.8), WHITE);
            p.text(
                "tap to close",
                centre.x - 40.0,
                area.y + area.h - 16.0,
                hint,
                p.theme.color.text_muted,
            );
        }
    }

    // ── fx ──────────────────────────────────────────────────────────────

    fn fx(&mut self, p: &Painter, area: Rect, g: &Gesture, dt: f32, t: f64) {
        let f = &mut self.fx;
        let centre = vec2(area.x + area.w / 2.0, area.y + area.h / 2.0);
        // Fire on a tap, and on its own every 1.6 s so a capture shows it.
        let auto = t - f.last_fire >= 1.6;
        if g.tap.is_some_and(|at| area.contains(at)) || auto {
            f.last_fire = t;
            f.shake.add(0.55);
            let style = TextStyle::new(p.font.cloned(), p.size(TextSize::Xl3), p.theme.color.error)
                .outline(BLACK, 2.0);
            f.popups.spawn("-7", style, centre + vec2(30.0, -60.0));
            f.particles
                .burst(centre, 24, 260.0, 6.0, 0.7, p.theme.color.accent_orange);
            f.pulse.set(1.18);
            f.pulse.to(1.0, 0.25, Ease::BackOut);
        }
        f.shake.update(dt);
        f.popups.update(dt);
        f.particles.update(dt);
        f.pulse.update(dt);

        fill_rounded(area, 12.0, p.theme.color.bg_secondary);
        p.text(
            "tap to hit (also fires every 1.6 s)",
            area.x + 16.0,
            area.y + 24.0,
            p.size(TextSize::Sm),
            p.theme.color.text_muted,
        );
        let off = f.shake.offset();
        let s = 110.0 * f.pulse.get();
        let target = Rect::new(centre.x - s / 2.0 + off.x, centre.y - s / 2.0 + off.y, s, s);
        fill_rounded(target, 14.0, p.theme.color.bg_highlight);
        stroke_rounded(target, 14.0, 3.0, p.theme.color.accent_red);
        f.particles.draw();
        f.popups.draw(screen_dpi_scale());
    }

    // ── art ─────────────────────────────────────────────────────────────

    fn art_story(&mut self, p: &Painter, area: Rect) {
        fill_rounded(area, 12.0, p.theme.color.bg_secondary);
        let note = if self.art.rounds() {
            "cover-cropped into each frame, corners clipped by shader"
        } else {
            "shader unavailable on this backend: corners are square"
        };
        p.text(
            note,
            area.x + 16.0,
            area.y + 24.0,
            p.size(TextSize::Sm),
            p.theme.color.text_muted,
        );
        let frames = [
            Rect::new(area.x + 24.0, area.y + 48.0, 180.0, 110.0),
            Rect::new(area.x + 224.0, area.y + 48.0, 100.0, 160.0),
            Rect::new(area.x + 344.0, area.y + 48.0, 130.0, 130.0),
        ];
        for (i, r) in frames.iter().enumerate() {
            if let Some(tex) = self.textures.get(i) {
                self.art.draw(tex, *r, 16.0, WHITE);
            }
            stroke_rounded(*r, 16.0, 2.0, p.theme.color.border);
        }
    }
}

/// Slots for a fanned hand of `n`, with `hovered` lifted and its neighbours
/// pushed aside. Returns each card's pose and draw order.
fn fan(n: usize, hovered: Option<usize>, area: Rect) -> Vec<(Pose, i32)> {
    if n == 0 {
        return Vec::new();
    }
    let cx = area.x + area.w / 2.0;
    let base_y = area.y + area.h - CARD.y * 0.62;
    let spacing = (CARD.x * 0.7).min((area.w - CARD.x - 40.0) / (n.max(2) - 1) as f32);
    let mid = (n as f32 - 1.0) / 2.0;
    (0..n)
        .map(|i| {
            let k = i as f32 - mid;
            let mut pos = vec2(cx + k * spacing, base_y + k * k * 4.0);
            let mut rotation = k * 0.07;
            let mut scale = 1.0;
            let mut z = i as i32;
            if let Some(h) = hovered {
                match i.cmp(&h) {
                    std::cmp::Ordering::Equal => {
                        pos.y = base_y - CARD.y * 0.32;
                        rotation = 0.0;
                        scale = 1.22;
                        z = 100;
                    }
                    std::cmp::Ordering::Less => pos.x -= CARD.x * 0.22,
                    std::cmp::Ordering::Greater => pos.x += CARD.x * 0.22,
                }
            }
            (Pose::at(pos).rotated(rotation).scaled(scale), z)
        })
        .collect()
}

/// Bake one sample card face at `scale` pixels per point.
fn bake_card(p: &Painter, art: &RoundedArt, textures: &[Texture2D], i: usize, scale: f32) -> Baked {
    let s = &SAMPLES[i];
    let theme = &p.theme;
    Baked::render(CARD, scale, |k| {
        let r = Rect::new(0.0, 0.0, CARD.x, CARD.y);
        fill_rounded(r, 10.0, theme.color.bg_primary);
        stroke_rounded(
            Rect::new(1.5, 1.5, CARD.x - 3.0, CARD.y - 3.0),
            9.0,
            3.0,
            theme.color.border,
        );
        if let Some(tex) = textures.get(i % textures.len().max(1)) {
            art.draw(tex, Rect::new(8.0, 30.0, CARD.x - 16.0, 56.0), 6.0, WHITE);
        }

        // Name: wraps right of the cost, stepping down the ramp until it fits
        // the 26-point band above the art.
        let band = 26.0;
        let (name, nl) = [TextSize::Base, TextSize::Sm, TextSize::Xs]
            .into_iter()
            .map(|size| {
                let mut name = RichText::new();
                let st = name.style(TextStyle::new(
                    p.font.cloned(),
                    p.size(size),
                    theme.color.text_primary,
                ));
                name.text(s.name, st);
                let l = name.layout(&MacroquadMeasure, CARD.x - 36.0, Align::Start, -1.0);
                (name, l)
            })
            .find_map(|(n, l)| (l.size.y <= band).then_some((n, l)))
            .unwrap_or_else(|| {
                let mut name = RichText::new();
                let st = name.style(TextStyle::new(
                    p.font.cloned(),
                    p.size(TextSize::Xs),
                    theme.color.text_primary,
                ));
                name.text(s.name, st);
                let l = name.layout(&MacroquadMeasure, CARD.x - 36.0, Align::Start, -1.0);
                (name, l)
            });
        nl.draw(
            &name,
            vec2(30.0, 2.0 + (band - nl.size.y).max(0.0) / 2.0),
            k,
            1.0,
            |_, _| {},
        );

        // Cost.
        draw_circle(16.0, 15.0, 10.0, theme.color.accent_blue);
        let cost = s.cost.to_string();
        let cs = p.size(TextSize::Md);
        let cd = p.measure(&cost, cs);
        draw_text_ex(
            &cost,
            16.0 - cd.width / 2.0,
            15.0 + cd.offset_y / 2.0 - 1.0,
            TextParams {
                font: p.font,
                font_size: (cs * k).round() as u16,
                font_scale: 1.0 / k,
                color: theme.color.bg_primary,
                ..Default::default()
            },
        );

        // Body, centred, with emphasis and icons.
        let mut body = RichText::new();
        let plain = body.style(TextStyle::new(
            p.font.cloned(),
            p.size(TextSize::Sm),
            theme.color.text_secondary,
        ));
        let strong = body.style(TextStyle::new(
            p.font.cloned(),
            p.size(TextSize::Md),
            theme.color.accent,
        ));
        for (text, emphasised, icon) in s.body {
            body.text(text, if *emphasised { strong } else { plain });
            if let Some(id) = icon {
                body.icon(*id, vec2(11.0, 11.0), plain);
            }
        }
        let bl = body.layout(&MacroquadMeasure, CARD.x - 16.0, Align::Center, 2.0);
        bl.draw(&body, vec2(8.0, 94.0), k, 1.0, |id, rect| {
            let c = rect.center();
            match id {
                ICON_ENERGY => draw_circle(c.x, c.y, rect.w / 2.0, theme.color.accent_yellow),
                ICON_GUARD => {
                    draw_rectangle(
                        rect.x + 1.0,
                        rect.y,
                        rect.w - 2.0,
                        rect.h * 0.7,
                        theme.color.accent_blue,
                    );
                    draw_triangle(
                        vec2(rect.x + 1.0, rect.y + rect.h * 0.7),
                        vec2(rect.x + rect.w - 1.0, rect.y + rect.h * 0.7),
                        vec2(c.x, rect.y + rect.h),
                        theme.color.accent_blue,
                    );
                }
                _ => {}
            }
        });
    })
}

/// Each curve: a plot, and a dot riding it on a loop.
fn easing(p: &Painter, area: Rect, t: f64) {
    const RUN: f64 = 1.2;
    const REST: f64 = 0.5;
    let phase = (t % (RUN + REST)).min(RUN) / RUN;
    let label = p.size(TextSize::Sm);
    let (plot_w, plot_h, row_h) = (70.0, 34.0, 46.0);
    for (i, e) in Ease::ALL.iter().enumerate() {
        let y = area.y + 8.0 + i as f32 * row_h;
        p.text(
            e.name(),
            area.x,
            y + plot_h / 2.0 + 4.0,
            label,
            p.theme.color.text_secondary,
        );
        let px = area.x + 110.0;
        draw_rectangle_lines(px, y, plot_w, plot_h, 1.0, p.theme.color.bg_highlight);
        let mut prev = vec2(px, y + plot_h);
        for s in 1..=40 {
            let u = s as f32 / 40.0;
            let pt = vec2(px + u * plot_w, y + plot_h - e.apply(u) * plot_h);
            draw_line(prev.x, prev.y, pt.x, pt.y, 1.5, p.theme.color.accent);
            prev = pt;
        }
        let track_x = px + plot_w + 24.0;
        let track_w = (area.w - (track_x - area.x) - 24.0).max(60.0);
        draw_line(
            track_x,
            y + plot_h / 2.0,
            track_x + track_w,
            y + plot_h / 2.0,
            1.0,
            p.theme.color.bg_highlight,
        );
        let v = e.apply(phase as f32);
        draw_circle(
            track_x + v * track_w,
            y + plot_h / 2.0,
            7.0,
            p.theme.color.accent_cyan,
        );
    }
}

// ── card face ───────────────────────────────────────────────────────────

/// The `icon` set and the `face` parts: every icon, gems in each tier hue,
/// stat emblems with one to three digits, and title plates whose names shrink
/// and then cut.
fn face_story(p: &Painter, area: Rect) {
    let c = p.theme.color;
    let px = screen_dpi_scale();
    let label = |s: &str, x: f32, y: f32| p.text(s, x, y, p.size(TextSize::Xs), c.text_muted);
    let mut y = area.y + 16.0;

    label("icon", area.x + 16.0, y);
    y += 12.0;
    for (i, icon) in Icon::ALL.iter().enumerate() {
        let r = Rect::new(area.x + 16.0 + i as f32 * 40.0, y, 26.0, 26.0);
        draw_icon(*icon, r, c.text_primary);
    }
    y += 52.0;

    label("gem · the tier hues", area.x + 16.0, y);
    y += 30.0;
    for (i, hue) in [
        c.text_muted,
        c.accent_green,
        c.accent_blue,
        c.accent_magenta,
        c.accent_orange,
    ]
    .into_iter()
    .enumerate()
    {
        let body = (
            Color::new(
                hue.r + (1.0 - hue.r) * 0.55,
                hue.g + (1.0 - hue.g) * 0.55,
                hue.b + (1.0 - hue.b) * 0.55,
                1.0,
            ),
            Color::new(hue.r * 0.7, hue.g * 0.7, hue.b * 0.7, 1.0),
        );
        let n = (i * 17 + 3).to_string();
        let style = TextStyle::new(p.font.cloned(), 14.0, c.bg_primary);
        gem(
            vec2(area.x + 36.0 + i as f32 * 52.0, y),
            15.0,
            body,
            Some((n.as_str(), style)),
            px,
        );
    }
    y += 40.0;

    label("emblem", area.x + 16.0, y);
    y += 12.0;
    for (i, (icon, hue, n)) in [
        (Icon::Sword, c.accent_red, "7"),
        (Icon::Shield, c.accent_blue, "12"),
        (Icon::Bolt, c.accent_yellow, "100"),
        (Icon::Heart, c.accent_green, "26"),
    ]
    .into_iter()
    .enumerate()
    {
        emblem(
            vec2(area.x + 16.0 + i as f32 * 92.0, y),
            22.0,
            icon,
            hue,
            (n, p.font),
            true,
            px,
        );
    }
    y += 44.0;

    label("plate · a name that shrinks, then cuts", area.x + 16.0, y);
    y += 12.0;
    for name in [
        "Short",
        "A Somewhat Longer Name",
        "A Name Far Too Long For Any Plate At All",
    ] {
        let r = Rect::new(area.x + 16.0, y, 200.0, 26.0);
        plate(r, 6.0, (c.bg_highlight, c.bg_primary));
        let (s, size) = shrink_to_fit(p.font, name, (16.0, 11.0), r.w - 20.0, px);
        line(
            &s,
            TextStyle::new(p.font.cloned(), size, c.text_primary),
            vec2(r.x + 10.0, r.center().y),
            vec2(0.0, 0.5),
            px,
        );
        y += 34.0;
    }
}

/// One paragraph at three widths and three alignments, boxes drawn to show
/// the wrap width, plus a width that follows the mouse.
fn text_story(p: &Painter, area: Rect) {
    let mut t = RichText::new();
    let plain = t.style(TextStyle::new(
        p.font.cloned(),
        p.size(TextSize::Md),
        p.theme.color.text_primary,
    ));
    let strong = t.style(
        TextStyle::new(p.font.cloned(), p.size(TextSize::Lg), p.theme.color.accent)
            .shadow(BLACK, vec2(1.0, 1.0)),
    );
    let muted = t.style(TextStyle::new(
        p.font.cloned(),
        p.size(TextSize::Sm),
        p.theme.color.text_muted,
    ));
    t.text("Deal ", plain)
        .text("8", strong)
        .text(" damage. Gain ", plain)
        .icon(ICON_ENERGY, vec2(14.0, 14.0), plain)
        .text(" if the enemy is guarding.\n", plain)
        .text("A run that continues mid-word: un", plain)
        .text("breakable", strong)
        .text(" stays together.", plain)
        .text(" (small print)", muted);
    let icon = |id: u32, r: Rect| {
        if id == ICON_ENERGY {
            let c = r.center();
            draw_circle(c.x, c.y, r.w / 2.0, p.theme.color.accent_yellow);
        }
    };
    let dpi = screen_dpi_scale();
    let mouse = Vec2::from(mouse_position());
    let live = if area.contains(mouse) {
        (mouse.x - area.x - 16.0).max(60.0)
    } else {
        300.0
    };
    let mut y = area.y + 8.0;
    for (w, align, name) in [
        (180.0, Align::Start, "180 · start"),
        (240.0, Align::Center, "240 · centre"),
        (300.0, Align::End, "300 · end"),
        (live, Align::Start, "follows the mouse"),
    ] {
        p.text(
            name,
            area.x + 16.0,
            y + 12.0,
            p.size(TextSize::Xs),
            p.theme.color.text_muted,
        );
        let l = t.layout(&MacroquadMeasure, w, align, 3.0);
        let origin = vec2(area.x + 16.0, y + 20.0);
        draw_rectangle_lines(
            origin.x,
            origin.y,
            w,
            l.size.y,
            1.0,
            p.theme.color.bg_highlight,
        );
        l.draw(&t, origin, dpi, 1.0, icon);
        y += l.size.y + 40.0;
    }
}

// ── wall ────────────────────────────────────────────────────────────────────

/// A wall to drive by hand: tiles that load, tick and shuffle.
///
/// - tap a tile to spotlight it (it grows and the wall repacks around it);
///   otherwise the spotlight moves on every few seconds;
/// - drag or wheel to scroll;
/// - `[` and `]` shrink or grow the spotlight (1 to 3 cells); `B` toggles
///   boosted tiles;
///   `R` reshuffles the ranking; `U` pulses a few tiles; `+` / `-` add or
///   remove tiles; `A` stops or starts the spotlight moving by itself.
struct WallStage {
    wall: macroquad_gamekit::Wall<u32>,
    tiles: Tiles<u32, WallLoaders>,
    loaders: WallLoaders,
    /// The ranking: keys in order.
    order: Vec<u32>,
    count: u32,
    spotlight: u32,
    spot_span: u8,
    boosted: bool,
    auto: bool,
    since: f64,
    scroll: f32,
    drag_from: Option<f32>,
    seed: u32,
}

/// The story's injected loaders: a stand-in image source that paints a face
/// for a URL after a latency of its own, and fails one in thirteen.
#[derive(Default)]
struct WallLoaders {
    asked: std::collections::HashMap<String, f64>,
    ready: std::collections::HashMap<String, (Texture2D, f64)>,
    failed: std::collections::HashSet<String>,
    clock: f64,
}

impl WallLoaders {
    fn update(&mut self, clock: f64) {
        self.clock = clock;
        let due: Vec<String> = self
            .asked
            .iter()
            .filter(|(url, at)| clock - **at >= wall_latency(url))
            .map(|(url, _)| url.clone())
            .collect();
        for url in due {
            self.asked.remove(&url);
            let n = wall_key(&url);
            if wall_hash(n).is_multiple_of(13) {
                self.failed.insert(url);
            } else {
                self.ready.insert(url, (wall_face(n), clock));
            }
        }
    }
}

impl ImageSource for WallLoaders {
    fn request(&mut self, url: &str, _priority: i32) {
        if !self.ready.contains_key(url) && !self.failed.contains(url) {
            self.asked.entry(url.to_string()).or_insert(self.clock);
        }
    }

    fn view(&mut self, url: &str) -> ImageView<'_> {
        if let Some((texture, at)) = self.ready.get(url) {
            return ImageView::Ready {
                texture,
                age: (self.clock - at) as f32,
            };
        }
        if self.failed.contains(url) {
            ImageView::Failed
        } else {
            ImageView::Loading
        }
    }
}

fn wall_hash(n: u32) -> u32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x
}

fn wall_key(url: &str) -> u32 {
    url.trim_start_matches("story://").parse().unwrap_or(0)
}

fn wall_latency(url: &str) -> f64 {
    0.25 + f64::from(wall_hash(wall_key(url) ^ 0xABCD) % 1500) / 1000.0
}

fn wall_hue(n: u32) -> Color {
    let h = (wall_hash(n) % 360) as f32 / 360.0;
    macroquad::color::hsl_to_rgb(h, 0.62, 0.58)
}

/// A small face on a flat colour, painted for key `n`.
fn wall_face(n: u32) -> Texture2D {
    const S: u16 = 64;
    let (bg, skin) = (wall_hue(n), wall_hue(n.wrapping_add(7)));
    let mut px = Vec::with_capacity(usize::from(S) * usize::from(S) * 4);
    for y in 0..S {
        for x in 0..S {
            let (fx, fy) = (f32::from(x) - 32.0, f32::from(y) - 36.0);
            let head = fx * fx + fy * fy < 22.0 * 22.0;
            let eye = |ex: f32| (fx - ex).powi(2) + (fy + 4.0).powi(2) < 16.0;
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

/// A live tile: a sweep going round on the wall's clock.
fn wall_live(ctx: TileCtx, ground: Color) {
    let r = ctx.rect;
    fill_rounded(r, ctx.radius, ground.with_alpha(0.25 * ctx.alpha));
    let c = r.center();
    let radius = r.w.min(r.h) * 0.3;
    let tau = std::f32::consts::TAU;
    let sweep = (ctx.time * 1.4).rem_euclid(tau);
    for i in 0..24 {
        let a = i as f32 / 24.0 * tau;
        let k = (1.0 - (sweep - a).rem_euclid(tau) / tau).powi(3);
        draw_circle(
            c.x + a.cos() * radius,
            c.y + a.sin() * radius,
            radius * 0.09,
            ground.with_alpha((0.25 + 0.75 * k) * ctx.alpha),
        );
    }
}

/// A gold ring while a tile is lifted (growing, or pulsed).
fn wall_ring(ctx: TileCtx) {
    if ctx.lift <= 0.01 {
        return;
    }
    let a = ctx.lift * ctx.alpha;
    stroke_rounded(ctx.rect, ctx.radius, 3.0, Color::new(1.0, 0.85, 0.35, a));
}

impl WallStage {
    fn new() -> Self {
        let mut stage = Self {
            wall: macroquad_gamekit::Wall::new(3),
            tiles: Tiles::new(),
            loaders: WallLoaders::default(),
            order: Vec::new(),
            count: 0,
            spotlight: 0,
            spot_span: 3,
            boosted: true,
            auto: true,
            since: 0.0,
            // `--scroll N`: start scrolled, for a capture of the clipped edge.
            scroll: macroquad_gamekit::harness::flag_num::<f32>("--scroll").unwrap_or(0.0),
            drag_from: None,
            seed: 1,
        };
        stage.set_count(120);
        stage
    }

    /// Grow or shrink the collection; new keys get a tile of their own kind.
    fn set_count(&mut self, count: u32) {
        let count = count.clamp(1, 400);
        for k in self.count..count {
            if k % 9 == 4 {
                let ground = wall_hue(k);
                self.tiles.insert(
                    k,
                    FnTile(move |ctx: TileCtx, _: &mut WallLoaders| wall_live(ctx, ground)),
                );
            } else if k % 17 == 11 {
                self.tiles.insert(k, ColourTile(wall_hue(k)));
            } else {
                self.tiles.insert(
                    k,
                    Layered(vec![
                        Box::new(ImageTile::new(
                            format!("story://{k}"),
                            wall_hue(k).with_alpha(0.35),
                        )),
                        Box::new(FnTile(|ctx: TileCtx, _: &mut WallLoaders| wall_ring(ctx))),
                    ]),
                );
            }
            self.order.push(k);
        }
        for k in count..self.count {
            self.tiles.remove(&k);
        }
        self.order.retain(|k| *k < count);
        if self.spotlight >= count {
            self.spotlight = 0;
        }
        self.count = count;
    }

    fn arrangement(&self) -> Vec<(u32, u8)> {
        let mut out = vec![(self.spotlight, self.spot_span)];
        let cols = self.wall.cols();
        out.extend(
            self.order
                .iter()
                .filter(|k| **k != self.spotlight)
                .map(|&k| {
                    let boost = self.boosted && wall_hash(k).is_multiple_of(7) && cols >= 4;
                    (k, if boost { 2 } else { 1 })
                }),
        );
        out
    }
}

impl KitStory {
    fn wall_story(&mut self, p: &Painter, area: Rect, g: &Gesture, dt: f32, t: f64) {
        let s = self.wall.get_or_insert_with(WallStage::new);
        let theme = p.theme.color;
        let head = Rect::new(area.x, area.y, area.w, 40.0);
        let field = Rect::new(area.x, area.y + 44.0, area.w, area.h - 44.0);

        // Keys.
        // Not the number keys: the storybook switches stories with those.
        if is_key_pressed(KeyCode::LeftBracket) {
            s.spot_span = s.spot_span.saturating_sub(1).max(1);
        }
        if is_key_pressed(KeyCode::RightBracket) {
            s.spot_span = (s.spot_span + 1).min(3);
        }
        if is_key_pressed(KeyCode::B) {
            s.boosted = !s.boosted;
        }
        if is_key_pressed(KeyCode::A) {
            s.auto = !s.auto;
        }
        if is_key_pressed(KeyCode::R) {
            // A new ranking: the same keys, scrambled.
            s.seed = s.seed.wrapping_add(1);
            let seed = s.seed;
            s.order
                .sort_by_key(|k| wall_hash(k ^ seed.wrapping_mul(0x51ED)));
        }
        if is_key_pressed(KeyCode::U) {
            for i in 0..5 {
                let k = wall_hash(s.seed.wrapping_add(i).wrapping_add(t as u32)) % s.count.min(30);
                s.wall.pulse(&k);
            }
        }
        if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
            s.set_count(s.count + 24);
        }
        if is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::KpSubtract) {
            s.set_count(s.count.saturating_sub(24));
        }

        // Scroll: wheel over the wall, or a drag.
        if field.contains(Vec2::from(mouse_position())) {
            s.scroll -= mouse_wheel().1 * 0.5;
        }
        match g.drag {
            Some(at) if field.contains(at) || s.drag_from.is_some() => {
                if let Some(from) = s.drag_from {
                    s.scroll += from - at.y;
                }
                s.drag_from = Some(at.y);
            }
            _ => s.drag_from = None,
        }

        // Columns from the width; the spotlight by tap or by itself.
        let gap = s.wall.style().gap;
        s.wall
            .set_cols(macroquad_gamekit::Wall::<u32>::cols_for(field.w, 92.0, gap));
        s.since += f64::from(dt);
        if let Some(tap) = g.tap
            && let Some(key) = s.wall.hit(field, s.scroll, tap)
        {
            s.spotlight = key;
            s.since = 0.0;
        } else if s.auto && s.since >= 3.0 {
            let at = s.order.iter().position(|k| *k == s.spotlight).unwrap_or(0);
            s.spotlight = s.order[(at + 1) % s.order.len().clamp(1, 12)];
            s.since = 0.0;
        }
        s.wall.arrange(s.arrangement());
        s.wall.update(dt);
        // Not before the wall has rows: a `--scroll` asked for at the start
        // would be clamped to nothing.
        if s.wall.rows() > 0 {
            s.scroll = s.scroll.clamp(0.0, s.wall.max_scroll(field));
        }
        s.wall
            .update_tiles(field, s.scroll, &mut s.tiles, &mut s.loaders, dt);
        s.loaders.update(t);

        // Draw: the wall clips its own scrolled rows, so nothing covers them.
        s.wall
            .draw_tiles(field, s.scroll, &mut s.tiles, &mut s.loaders);
        p.text(
            &format!(
                "{} tiles · {} cols · spotlight {} at {}x{} · boosted {} · auto {} · scrolled {:.0} of {:.0}",
                s.count,
                s.wall.cols(),
                s.spotlight,
                s.spot_span,
                s.spot_span,
                if s.boosted { "on" } else { "off" },
                if s.auto { "on" } else { "off" },
                s.scroll,
                s.wall.max_scroll(field),
            ),
            head.x + 4.0,
            head.y + 14.0,
            p.size(TextSize::Sm),
            theme.text_primary,
        );
        p.text(
            "tap: spotlight · drag/wheel: scroll · [ ]: spotlight size · B: boosts · R: reshuffle · U: pulse · + -: tiles · A: auto",
            head.x + 4.0,
            head.y + 32.0,
            p.size(TextSize::Xs),
            theme.text_muted,
        );
    }
}
